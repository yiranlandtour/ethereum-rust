use async_trait::async_trait;
use ethereum_types::{Address, H256, U256};
use ethereum_core::{Block, Transaction};
use ethereum_engine::types::{ExecutionPayloadV3, PayloadAttributesV3};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{info, debug, warn};

use crate::{MevError, Result, Bundle, Auction};

/// Block builder configuration
#[derive(Debug, Clone)]
pub struct BuilderConfig {
    pub builder_address: Address,
    pub min_bid_increment: U256,
    pub max_gas_limit: u64,
    pub coinbase_transfer_gas: u64,
    pub bundle_timeout_ms: u64,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            builder_address: Address::zero(),
            min_bid_increment: U256::from(1_000_000_000u64), // 1 Gwei
            max_gas_limit: 30_000_000,
            coinbase_transfer_gas: 21_000,
            bundle_timeout_ms: 500,
        }
    }
}

/// Block builder API
#[async_trait]
pub trait BuilderApi: Send + Sync {
    async fn build_block(&self, attributes: PayloadAttributesV3) -> Result<ExecutionPayloadV3>;
    async fn simulate_block(&self, block: &Block) -> Result<SimulationResult>;
    async fn submit_bundle(&self, bundle: Bundle) -> Result<BundleSubmissionResult>;
}

/// Block simulation result
#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub gas_used: u64,
    pub base_fee: U256,
    pub priority_fees: U256,
    pub mev_revenue: U256,
    pub coinbase_transfers: Vec<CoinbaseTransfer>,
}

#[derive(Debug, Clone)]
pub struct CoinbaseTransfer {
    pub from: Address,
    pub amount: U256,
    pub gas_used: u64,
}

#[derive(Debug, Clone)]
pub struct BundleSubmissionResult {
    pub bundle_hash: H256,
    pub simulation: SimulationResult,
    pub included: bool,
}

/// Main block builder implementation
pub struct BlockBuilder {
    config: BuilderConfig,
    bundles: Arc<RwLock<BundlePool>>,
    auction: Arc<Auction>,
    mempool: Arc<RwLock<Vec<Transaction>>>,
}

impl BlockBuilder {
    pub fn new(config: BuilderConfig) -> Self {
        Self {
            config,
            bundles: Arc::new(RwLock::new(BundlePool::new())),
            auction: Arc::new(Auction::new()),
            mempool: Arc::new(RwLock::new(Vec::new())),
        }
    }
    
    pub fn add_bundle(&self, bundle: Bundle) -> Result<()> {
        let mut pool = self.bundles.write().unwrap();
        pool.add(bundle)?;
        Ok(())
    }
    
    pub fn add_transaction(&self, tx: Transaction) {
        let mut mempool = self.mempool.write().unwrap();
        mempool.push(tx);
    }
    
    /// Build optimal block with MEV extraction
    pub fn build_optimal_block(
        &self,
        parent: &Block,
        attributes: &PayloadAttributesV3,
    ) -> Result<Block> {
        let mut block = self.create_empty_block(parent, attributes)?;
        let mut gas_used = 0u64;
        let gas_limit = self.config.max_gas_limit.min(attributes.gas_limit.as_u64());
        
        // Phase 1: Include profitable bundles
        let bundles = self.select_bundles(gas_limit)?;
        for bundle in bundles {
            let bundle_gas = self.estimate_bundle_gas(&bundle)?;
            if gas_used + bundle_gas <= gas_limit {
                self.apply_bundle(&mut block, bundle)?;
                gas_used += bundle_gas;
            }
        }
        
        // Phase 2: Fill remaining space with mempool transactions
        let mempool = self.mempool.read().unwrap();
        let sorted_txs = self.sort_transactions_by_priority(&mempool);
        
        for tx in sorted_txs {
            let tx_gas = self.estimate_transaction_gas(&tx)?;
            if gas_used + tx_gas <= gas_limit {
                block.transactions.push(tx.clone());
                gas_used += tx_gas;
            }
        }
        
        // Phase 3: Add coinbase transfer if profitable
        if let Some(transfer) = self.calculate_coinbase_transfer(&block)? {
            if gas_used + self.config.coinbase_transfer_gas <= gas_limit {
                block.transactions.push(transfer);
                gas_used += self.config.coinbase_transfer_gas;
            }
        }
        
        block.header.gas_used = U256::from(gas_used);
        Ok(block)
    }
    
    fn create_empty_block(
        &self,
        parent: &Block,
        attributes: &PayloadAttributesV3,
    ) -> Result<Block> {
        use ethereum_core::Header;
        
        let header = Header {
            parent_hash: parent.hash(),
            uncles_hash: H256::zero(),
            beneficiary: attributes.suggested_fee_recipient,
            state_root: H256::zero(),
            transactions_root: H256::zero(),
            receipts_root: H256::zero(),
            logs_bloom: ethereum_types::Bloom::zero(),
            difficulty: U256::zero(),
            number: parent.header.number + 1,
            gas_limit: U256::from(attributes.gas_limit.as_u64()),
            gas_used: U256::zero(),
            timestamp: attributes.timestamp.as_u64(),
            extra_data: ethereum_types::Bytes::from(b"MEV-Builder".to_vec()),
            mix_hash: attributes.prev_randao,
            nonce: [0u8; 8],
            base_fee_per_gas: Some(self.calculate_base_fee(parent)),
            withdrawals_root: Some(H256::zero()),
            blob_gas_used: Some(ethereum_types::U64::zero()),
            excess_blob_gas: Some(ethereum_types::U64::zero()),
            parent_beacon_block_root: Some(attributes.parent_beacon_block_root),
        };
        
        Ok(Block {
            header,
            transactions: Vec::new(),
            uncles: Vec::new(),
            withdrawals: Some(attributes.withdrawals.clone()),
        })
    }
    
    fn calculate_base_fee(&self, parent: &Block) -> U256 {
        let parent_base_fee = parent.header.base_fee_per_gas.unwrap_or(U256::from(1_000_000_000));
        let parent_gas_used = parent.header.gas_used;
        let parent_gas_target = parent.header.gas_limit / 2;
        
        if parent_gas_used == parent_gas_target {
            return parent_base_fee;
        }
        
        let elasticity_multiplier = 2;
        let base_fee_change_denominator = 8;
        
        if parent_gas_used > parent_gas_target {
            let gas_used_delta = parent_gas_used - parent_gas_target;
            let base_fee_delta = parent_base_fee * gas_used_delta 
                / parent_gas_target / base_fee_change_denominator;
            parent_base_fee + base_fee_delta.max(U256::one())
        } else {
            let gas_used_delta = parent_gas_target - parent_gas_used;
            let base_fee_delta = parent_base_fee * gas_used_delta 
                / parent_gas_target / base_fee_change_denominator;
            parent_base_fee.saturating_sub(base_fee_delta).max(U256::one())
        }
    }
    
    fn select_bundles(&self, gas_limit: u64) -> Result<Vec<Bundle>> {
        let pool = self.bundles.read().unwrap();
        let all_bundles = pool.get_all();
        
        // Run auction to select optimal bundle combination
        let auction_result = self.auction.run(all_bundles, gas_limit)?;
        
        Ok(auction_result.winning_bundles)
    }
    
    fn apply_bundle(&self, block: &mut Block, bundle: Bundle) -> Result<()> {
        for tx in bundle.transactions {
            block.transactions.push(tx.transaction);
        }
        Ok(())
    }
    
    fn sort_transactions_by_priority(&self, txs: &[Transaction]) -> Vec<Transaction> {
        let mut sorted = txs.to_vec();
        sorted.sort_by(|a, b| {
            let a_priority = self.calculate_priority_fee(a);
            let b_priority = self.calculate_priority_fee(b);
            b_priority.cmp(&a_priority)
        });
        sorted
    }
    
    fn calculate_priority_fee(&self, tx: &Transaction) -> U256 {
        match tx {
            Transaction::Legacy(tx) => tx.gas_price,
            Transaction::Eip2930(tx) => tx.gas_price,
            Transaction::Eip1559(tx) => tx.max_priority_fee_per_gas,
            Transaction::Eip4844(tx) => tx.max_priority_fee_per_gas,
            Transaction::Eip7702(tx) => tx.max_priority_fee_per_gas,
        }
    }
    
    fn estimate_bundle_gas(&self, bundle: &Bundle) -> Result<u64> {
        let mut total_gas = 0u64;
        for tx in &bundle.transactions {
            total_gas += self.estimate_transaction_gas(&tx.transaction)?;
        }
        Ok(total_gas)
    }
    
    fn estimate_transaction_gas(&self, tx: &Transaction) -> Result<u64> {
        // Simple estimation - in production would use actual EVM simulation
        Ok(tx.gas_limit().as_u64())
    }
    
    fn calculate_coinbase_transfer(&self, block: &Block) -> Result<Option<Transaction>> {
        // Calculate total MEV revenue and create coinbase transfer if profitable
        let mev_revenue = self.calculate_mev_revenue(block)?;
        
        if mev_revenue > U256::zero() {
            // Create a simple transfer transaction to coinbase
            // In production, this would be more sophisticated
            Ok(None)
        } else {
            Ok(None)
        }
    }
    
    fn calculate_mev_revenue(&self, block: &Block) -> Result<U256> {
        // Calculate MEV revenue from block
        // This would analyze sandwich attacks, arbitrage, liquidations, etc.
        Ok(U256::zero())
    }
}

#[async_trait]
impl BuilderApi for BlockBuilder {
    async fn build_block(&self, attributes: PayloadAttributesV3) -> Result<ExecutionPayloadV3> {
        // Get parent block (would come from storage in production)
        let parent = Block::default();
        
        let block = self.build_optimal_block(&parent, &attributes)?;
        
        // Convert block to execution payload
        let payload = self.block_to_payload(block, attributes)?;
        
        Ok(payload)
    }
    
    async fn simulate_block(&self, block: &Block) -> Result<SimulationResult> {
        let gas_used = block.header.gas_used.as_u64();
        let base_fee = block.header.base_fee_per_gas.unwrap_or(U256::zero());
        
        // Calculate fees
        let mut priority_fees = U256::zero();
        for tx in &block.transactions {
            priority_fees += self.calculate_priority_fee(tx) * tx.gas_limit();
        }
        
        let mev_revenue = self.calculate_mev_revenue(block)?;
        
        Ok(SimulationResult {
            gas_used,
            base_fee,
            priority_fees,
            mev_revenue,
            coinbase_transfers: Vec::new(),
        })
    }
    
    async fn submit_bundle(&self, bundle: Bundle) -> Result<BundleSubmissionResult> {
        let bundle_hash = bundle.hash();
        
        // Simulate bundle
        let mut test_block = Block::default();
        self.apply_bundle(&mut test_block, bundle.clone())?;
        let simulation = self.simulate_block(&test_block).await?;
        
        // Add to pool
        self.add_bundle(bundle)?;
        
        Ok(BundleSubmissionResult {
            bundle_hash,
            simulation,
            included: false, // Will be updated when block is built
        })
    }
}

impl BlockBuilder {
    fn block_to_payload(&self, block: Block, attributes: PayloadAttributesV3) -> Result<ExecutionPayloadV3> {
        Ok(ExecutionPayloadV3 {
            parent_hash: block.header.parent_hash,
            fee_recipient: block.header.beneficiary,
            state_root: block.header.state_root,
            receipts_root: block.header.receipts_root,
            logs_bloom: block.header.logs_bloom,
            prev_randao: block.header.mix_hash,
            block_number: ethereum_types::U64::from(block.header.number),
            gas_limit: ethereum_types::U64::from(block.header.gas_limit.as_u64()),
            gas_used: ethereum_types::U64::from(block.header.gas_used.as_u64()),
            timestamp: ethereum_types::U64::from(block.header.timestamp),
            extra_data: block.header.extra_data,
            base_fee_per_gas: block.header.base_fee_per_gas.unwrap_or(U256::zero()),
            block_hash: block.hash(),
            transactions: block.transactions
                .iter()
                .map(|tx| ethereum_types::Bytes::from(ethereum_rlp::encode(tx)))
                .collect(),
            withdrawals: attributes.withdrawals,
            blob_gas_used: block.header.blob_gas_used.unwrap_or(ethereum_types::U64::zero()),
            excess_blob_gas: block.header.excess_blob_gas.unwrap_or(ethereum_types::U64::zero()),
        })
    }
}

/// Bundle pool for managing MEV bundles
pub struct BundlePool {
    bundles: HashMap<H256, Bundle>,
    max_bundles: usize,
}

impl BundlePool {
    pub fn new() -> Self {
        Self {
            bundles: HashMap::new(),
            max_bundles: 1000,
        }
    }
    
    pub fn add(&mut self, bundle: Bundle) -> Result<()> {
        if self.bundles.len() >= self.max_bundles {
            // Remove oldest bundle
            if let Some(oldest) = self.bundles.keys().next().cloned() {
                self.bundles.remove(&oldest);
            }
        }
        
        let hash = bundle.hash();
        self.bundles.insert(hash, bundle);
        Ok(())
    }
    
    pub fn get(&self, hash: &H256) -> Option<&Bundle> {
        self.bundles.get(hash)
    }
    
    pub fn get_all(&self) -> Vec<Bundle> {
        self.bundles.values().cloned().collect()
    }
    
    pub fn remove(&mut self, hash: &H256) -> Option<Bundle> {
        self.bundles.remove(hash)
    }
    
    pub fn clear_expired(&mut self, current_block: u64) {
        self.bundles.retain(|_, bundle| {
            bundle.max_block == 0 || bundle.max_block >= current_block
        });
    }
}