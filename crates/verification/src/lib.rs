use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Header, Transaction, Receipt};
use ethereum_storage::Database;
use ethereum_consensus::{Consensus, ConsensusConfig, EngineType};
use ethereum_evm::EVM;
use ethereum_trie::PatriciaTrie;
use std::sync::Arc;
use std::collections::HashMap;
use thiserror::Error;

pub mod block;
pub mod transaction;
pub mod header;
pub mod state;

pub use block::BlockVerifier;
pub use transaction::TransactionVerifier;
pub use header::HeaderVerifier;
pub use state::StateVerifier;

#[derive(Debug, Error)]
pub enum VerificationError {
    #[error("Invalid block: {0}")]
    InvalidBlock(String),
    
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
    
    #[error("Invalid header: {0}")]
    InvalidHeader(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Parent not found")]
    ParentNotFound,
    
    #[error("State root mismatch")]
    StateRootMismatch,
    
    #[error("Gas limit exceeded")]
    GasLimitExceeded,
    
    #[error("Consensus error: {0}")]
    ConsensusError(#[from] ethereum_consensus::ConsensusError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] ethereum_storage::StorageError),
}

pub type Result<T> = std::result::Result<T, VerificationError>;

/// Main verification engine
pub struct VerificationEngine<D: Database> {
    db: Arc<D>,
    consensus: Arc<Consensus<D>>,
    evm: Arc<EVM<D>>,
    config: VerificationConfig,
}

#[derive(Debug, Clone)]
pub struct VerificationConfig {
    pub max_block_gas: U256,
    pub min_gas_price: U256,
    pub chain_id: u64,
    pub validate_state_root: bool,
    pub validate_receipts_root: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            max_block_gas: U256::from(30_000_000),
            min_gas_price: U256::from(1_000_000_000), // 1 gwei
            chain_id: 1,
            validate_state_root: true,
            validate_receipts_root: true,
        }
    }
}

impl<D: Database + 'static> VerificationEngine<D> {
    pub fn new(
        db: Arc<D>,
        consensus_config: ConsensusConfig,
        verification_config: VerificationConfig,
    ) -> Self {
        let consensus = Arc::new(Consensus::new(consensus_config, db.clone()));
        let evm = Arc::new(EVM::new(db.clone()));
        
        Self {
            db,
            consensus,
            evm,
            config: verification_config,
        }
    }
    
    /// Verify a complete block
    pub async fn verify_block(&self, block: &Block) -> Result<()> {
        // 1. Verify header
        let header_verifier = HeaderVerifier::new(self.db.clone());
        header_verifier.verify(&block.header).await?;
        
        // 2. Verify consensus rules
        self.consensus.validate_block(block).await
            .map_err(|e| VerificationError::ConsensusError(e))?;
        
        // 3. Verify transactions
        let tx_verifier = TransactionVerifier::new(self.config.chain_id);
        for tx in &block.body.transactions {
            tx_verifier.verify(tx)?;
        }
        
        // 4. Verify block structure
        let block_verifier = BlockVerifier::new(self.db.clone());
        block_verifier.verify_structure(block)?;
        
        // 5. Verify state transition
        if self.config.validate_state_root {
            self.verify_state_transition(block).await?;
        }
        
        Ok(())
    }
    
    /// Verify state transition by executing block
    async fn verify_state_transition(&self, block: &Block) -> Result<()> {
        // Get parent state
        let parent_state_root = self.get_parent_state_root(&block.header)?;
        
        // Initialize state from parent
        let mut state = PatriciaTrie::new_with_root(self.db.clone(), parent_state_root);
        
        // Execute transactions
        let mut receipts = Vec::new();
        let mut cumulative_gas = U256::zero();
        
        for tx in &block.body.transactions {
            // Execute transaction
            let receipt = self.execute_transaction(
                tx,
                &mut state,
                &block.header,
                cumulative_gas,
            ).await?;
            
            cumulative_gas = cumulative_gas + receipt.gas_used;
            receipts.push(receipt);
        }
        
        // Apply block rewards
        self.apply_block_rewards(&mut state, &block.header).await?;
        
        // Verify final state root
        let computed_state_root = state.commit().await?;
        if computed_state_root != block.header.state_root {
            return Err(VerificationError::StateRootMismatch);
        }
        
        // Verify receipts root
        if self.config.validate_receipts_root {
            let computed_receipts_root = self.compute_receipts_root(&receipts);
            if computed_receipts_root != block.header.receipts_root {
                return Err(VerificationError::InvalidBlock(
                    "Receipts root mismatch".to_string()
                ));
            }
        }
        
        // Verify gas used
        if cumulative_gas != block.header.gas_used {
            return Err(VerificationError::InvalidBlock(
                format!("Gas used mismatch: expected {}, got {}", 
                        block.header.gas_used, cumulative_gas)
            ));
        }
        
        Ok(())
    }
    
    /// Execute a single transaction
    async fn execute_transaction(
        &self,
        tx: &Transaction,
        state: &mut PatriciaTrie<D>,
        header: &Header,
        cumulative_gas: U256,
    ) -> Result<Receipt> {
        // Create EVM context
        let context = ethereum_evm::Context {
            block_number: header.number,
            timestamp: header.timestamp,
            gas_limit: header.gas_limit,
            coinbase: header.author,
            difficulty: header.difficulty,
            chain_id: self.config.chain_id,
        };
        
        // Execute transaction in EVM
        let result = self.evm.execute_transaction(tx, state, &context).await
            .map_err(|e| VerificationError::InvalidTransaction(e.to_string()))?;
        
        // Create receipt
        let receipt = Receipt {
            status: if result.success { 1 } else { 0 },
            cumulative_gas_used: cumulative_gas + result.gas_used,
            logs_bloom: result.logs_bloom,
            logs: result.logs,
            gas_used: result.gas_used,
            contract_address: result.contract_address,
        };
        
        Ok(receipt)
    }
    
    /// Apply block rewards
    async fn apply_block_rewards(
        &self,
        state: &mut PatriciaTrie<D>,
        header: &Header,
    ) -> Result<()> {
        let reward = self.consensus.get_engine().block_reward(header.number);
        
        // Add reward to coinbase account
        let coinbase_key = header.author.as_bytes();
        let mut account = state.get(coinbase_key).await?
            .map(|data| bincode::deserialize(&data).unwrap())
            .unwrap_or_else(|| ethereum_core::Account::default());
        
        account.balance = account.balance + reward;
        
        state.insert(
            coinbase_key,
            bincode::serialize(&account).unwrap(),
        ).await?;
        
        Ok(())
    }
    
    /// Get parent state root
    fn get_parent_state_root(&self, header: &Header) -> Result<H256> {
        if header.number == U256::zero() {
            // Genesis block
            return Ok(H256::zero());
        }
        
        let parent_key = format!("header:{}", hex::encode(header.parent_hash));
        let parent_data = self.db.get(parent_key.as_bytes())?
            .ok_or(VerificationError::ParentNotFound)?;
        
        let parent_header: Header = bincode::deserialize(&parent_data)
            .map_err(|_| VerificationError::InvalidHeader("Failed to deserialize parent".to_string()))?;
        
        Ok(parent_header.state_root)
    }
    
    /// Compute receipts root
    fn compute_receipts_root(&self, receipts: &[Receipt]) -> H256 {
        if receipts.is_empty() {
            return H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6,
                              0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
                              0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0,
                              0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]);
        }
        
        // Build Merkle Patricia Trie of receipts
        let mut data = Vec::new();
        for receipt in receipts {
            data.extend_from_slice(&ethereum_crypto::keccak256(
                &bincode::serialize(receipt).unwrap()
            ));
        }
        
        H256(ethereum_crypto::keccak256(&data))
    }
    
    /// Verify a batch of blocks
    pub async fn verify_blocks(&self, blocks: Vec<Block>) -> Result<Vec<Block>> {
        let mut verified = Vec::new();
        
        for block in blocks {
            match self.verify_block(&block).await {
                Ok(()) => verified.push(block),
                Err(e) => {
                    tracing::warn!("Block verification failed: {}", e);
                }
            }
        }
        
        Ok(verified)
    }
    
    /// Quick validation without state execution
    pub async fn validate_block_header(&self, header: &Header) -> Result<()> {
        let header_verifier = HeaderVerifier::new(self.db.clone());
        header_verifier.verify(header).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_verification_config_default() {
        let config = VerificationConfig::default();
        assert_eq!(config.chain_id, 1);
        assert_eq!(config.max_block_gas, U256::from(30_000_000));
    }
}