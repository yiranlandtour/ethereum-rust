use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Transaction, Receipt};
use ethereum_storage::Database;
use ethereum_evm::{EVM, ExecutionResult};
use ethereum_trie::PatriciaTrie;
use std::sync::Arc;
use std::collections::HashMap;
use thiserror::Error;
use serde::{Serialize, Deserialize};

pub mod tracer;
pub mod debugger;
pub mod profiler;
pub mod state_diff;

pub use tracer::{Tracer, TraceConfig, TraceResult, CallTrace};
pub use debugger::{Debugger, Breakpoint, DebuggerState};
pub use profiler::{Profiler, GasProfile, OpcodeStats};
pub use state_diff::{StateDiff, AccountDiff, StorageDiff};

#[derive(Debug, Error)]
pub enum DebugError {
    #[error("Block not found")]
    BlockNotFound,
    
    #[error("Transaction not found")]
    TransactionNotFound,
    
    #[error("Invalid trace config")]
    InvalidTraceConfig,
    
    #[error("Execution error: {0}")]
    ExecutionError(String),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] ethereum_storage::StorageError),
    
    #[error("EVM error: {0}")]
    EvmError(String),
}

pub type Result<T> = std::result::Result<T, DebugError>;

/// Debug API implementation
pub struct DebugAPI<D: Database> {
    db: Arc<D>,
    evm: Arc<EVM<D>>,
    tracer: Tracer<D>,
    debugger: Debugger<D>,
    profiler: Profiler,
}

impl<D: Database + 'static> DebugAPI<D> {
    pub fn new(db: Arc<D>) -> Self {
        let evm = Arc::new(EVM::new(db.clone()));
        
        Self {
            db: db.clone(),
            evm: evm.clone(),
            tracer: Tracer::new(db.clone(), evm.clone()),
            debugger: Debugger::new(db.clone(), evm.clone()),
            profiler: Profiler::new(),
        }
    }
    
    /// Trace transaction execution
    pub async fn trace_transaction(
        &self,
        tx_hash: H256,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult> {
        // Get transaction and block
        let (tx, block) = self.get_transaction_and_block(tx_hash).await?;
        
        // Trace transaction
        self.tracer.trace_transaction(&tx, &block, config).await
    }
    
    /// Trace block execution
    pub async fn trace_block(
        &self,
        block_hash: H256,
        config: Option<TraceConfig>,
    ) -> Result<Vec<TraceResult>> {
        let block = self.get_block(block_hash).await?;
        
        // Trace all transactions in block
        self.tracer.trace_block(&block, config).await
    }
    
    /// Trace block by number
    pub async fn trace_block_by_number(
        &self,
        block_number: U256,
        config: Option<TraceConfig>,
    ) -> Result<Vec<TraceResult>> {
        let block_hash = self.get_block_hash_by_number(block_number).await?;
        self.trace_block(block_hash, config).await
    }
    
    /// Trace call
    pub async fn trace_call(
        &self,
        call: CallRequest,
        block_number: Option<U256>,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult> {
        let block_num = block_number.unwrap_or_else(|| self.get_latest_block_number());
        
        // Create transaction from call request
        let tx = self.call_to_transaction(call);
        
        // Get block for context
        let block_hash = self.get_block_hash_by_number(block_num).await?;
        let block = self.get_block(block_hash).await?;
        
        // Trace call
        self.tracer.trace_transaction(&tx, &block, config).await
    }
    
    /// Get transaction trace
    pub async fn get_transaction_trace(&self, tx_hash: H256) -> Result<CallTrace> {
        let result = self.trace_transaction(tx_hash, None).await?;
        
        match result {
            TraceResult::CallTrace(trace) => Ok(trace),
            _ => Err(DebugError::InvalidTraceConfig),
        }
    }
    
    /// Debug transaction with breakpoints
    pub async fn debug_transaction(
        &self,
        tx_hash: H256,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<DebuggerState> {
        let (tx, block) = self.get_transaction_and_block(tx_hash).await?;
        
        self.debugger.debug_transaction(&tx, &block, breakpoints).await
    }
    
    /// Get storage at specific block
    pub async fn get_storage_at(
        &self,
        address: Address,
        position: H256,
        block_number: Option<U256>,
    ) -> Result<H256> {
        let block_num = block_number.unwrap_or_else(|| self.get_latest_block_number());
        
        // Get state at block
        let state_root = self.get_state_root_at_block(block_num).await?;
        let state = PatriciaTrie::new_with_root(self.db.clone(), state_root);
        
        // Get account storage
        let account_key = address.as_bytes();
        let account_data = state.get(account_key).await
            .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
        
        if let Some(data) = account_data {
            let account: ethereum_core::Account = bincode::deserialize(&data)
                .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
            
            // Get storage value
            let storage_trie = PatriciaTrie::new_with_root(self.db.clone(), account.storage_root);
            let value = storage_trie.get(position.as_bytes()).await
                .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
            
            if let Some(v) = value {
                let bytes: [u8; 32] = v.try_into()
                    .map_err(|_| DebugError::ExecutionError("Invalid storage value".to_string()))?;
                return Ok(H256::from(bytes));
            }
        }
        
        Ok(H256::zero())
    }
    
    /// Get state diff for a transaction
    pub async fn get_state_diff(&self, tx_hash: H256) -> Result<StateDiff> {
        let (tx, block) = self.get_transaction_and_block(tx_hash).await?;
        
        // Execute transaction and track state changes
        let state_diff = state_diff::compute_state_diff(
            &tx,
            &block,
            self.db.clone(),
            self.evm.clone(),
        ).await?;
        
        Ok(state_diff)
    }
    
    /// Profile gas usage
    pub async fn profile_transaction(&self, tx_hash: H256) -> Result<GasProfile> {
        let (tx, block) = self.get_transaction_and_block(tx_hash).await?;
        
        let profile = self.profiler.profile_transaction(&tx, &block, self.evm.clone()).await?;
        
        Ok(profile)
    }
    
    /// Get opcode statistics
    pub async fn get_opcode_stats(&self, tx_hash: H256) -> Result<OpcodeStats> {
        let (tx, block) = self.get_transaction_and_block(tx_hash).await?;
        
        let stats = self.profiler.get_opcode_stats(&tx, &block, self.evm.clone()).await?;
        
        Ok(stats)
    }
    
    /// Get bad blocks (blocks that failed validation)
    pub async fn get_bad_blocks(&self) -> Result<Vec<Block>> {
        let mut bad_blocks = Vec::new();
        
        // Iterate through bad blocks storage
        let prefix = b"bad_block:";
        let iter = self.db.iter_prefix(prefix)?;
        
        for (_, value) in iter {
            if let Ok(block) = bincode::deserialize::<Block>(&value) {
                bad_blocks.push(block);
            }
        }
        
        Ok(bad_blocks)
    }
    
    /// Get block RLP
    pub async fn get_block_rlp(&self, block_hash: H256) -> Result<Vec<u8>> {
        let block = self.get_block(block_hash).await?;
        
        // Serialize block to RLP
        bincode::serialize(&block)
            .map_err(|e| DebugError::ExecutionError(e.to_string()))
    }
    
    /// Print block
    pub async fn print_block(&self, block_number: U256) -> Result<String> {
        let block_hash = self.get_block_hash_by_number(block_number).await?;
        let block = self.get_block(block_hash).await?;
        
        // Format block as string
        Ok(format!("{:#?}", block))
    }
    
    /// Get chain config
    pub async fn get_chain_config(&self) -> ChainConfig {
        ChainConfig {
            chain_id: 1,
            homestead_block: Some(U256::from(1_150_000)),
            dao_fork_block: Some(U256::from(1_920_000)),
            eip150_block: Some(U256::from(2_463_000)),
            eip158_block: Some(U256::from(2_675_000)),
            byzantium_block: Some(U256::from(4_370_000)),
            constantinople_block: Some(U256::from(7_280_000)),
            petersburg_block: Some(U256::from(7_280_000)),
            istanbul_block: Some(U256::from(9_069_000)),
            berlin_block: Some(U256::from(12_244_000)),
            london_block: Some(U256::from(12_965_000)),
            merge_block: Some(U256::from(15_537_394)),
        }
    }
    
    // Helper methods
    
    async fn get_transaction_and_block(&self, tx_hash: H256) -> Result<(Transaction, Block)> {
        // Get transaction
        let tx_key = format!("tx:{}", hex::encode(tx_hash));
        let tx_data = self.db.get(tx_key.as_bytes())?
            .ok_or(DebugError::TransactionNotFound)?;
        
        let tx: Transaction = bincode::deserialize(&tx_data)
            .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
        
        // Get block containing transaction
        let block_key = format!("tx:block:{}", hex::encode(tx_hash));
        let block_hash_data = self.db.get(block_key.as_bytes())?
            .ok_or(DebugError::TransactionNotFound)?;
        
        let block_hash = H256::from_slice(&block_hash_data);
        let block = self.get_block(block_hash).await?;
        
        Ok((tx, block))
    }
    
    async fn get_block(&self, block_hash: H256) -> Result<Block> {
        let key = format!("block:{}", hex::encode(block_hash));
        let data = self.db.get(key.as_bytes())?
            .ok_or(DebugError::BlockNotFound)?;
        
        bincode::deserialize(&data)
            .map_err(|e| DebugError::ExecutionError(e.to_string()))
    }
    
    async fn get_block_hash_by_number(&self, block_number: U256) -> Result<H256> {
        let key = format!("block:number:{}", block_number);
        let data = self.db.get(key.as_bytes())?
            .ok_or(DebugError::BlockNotFound)?;
        
        Ok(H256::from_slice(&data))
    }
    
    fn get_latest_block_number(&self) -> U256 {
        // Get from database
        U256::zero()
    }
    
    async fn get_state_root_at_block(&self, block_number: U256) -> Result<H256> {
        let block_hash = self.get_block_hash_by_number(block_number).await?;
        let block = self.get_block(block_hash).await?;
        Ok(block.header.state_root)
    }
    
    fn call_to_transaction(&self, call: CallRequest) -> Transaction {
        Transaction {
            nonce: 0,
            gas_price: call.gas_price,
            gas_limit: call.gas.unwrap_or(U256::from(8_000_000)),
            to: call.to,
            value: call.value.unwrap_or_else(U256::zero),
            input: call.data.unwrap_or_default(),
            signature: Default::default(),
            transaction_type: 0,
            chain_id: Some(1),
            access_list: None,
            max_fee_per_gas: None,
            max_priority_fee_per_gas: None,
        }
    }
}

/// Call request for debug_traceCall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRequest {
    pub from: Option<Address>,
    pub to: Option<Address>,
    pub gas: Option<U256>,
    pub gas_price: Option<U256>,
    pub value: Option<U256>,
    pub data: Option<Vec<u8>>,
}

/// Chain configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainConfig {
    pub chain_id: u64,
    pub homestead_block: Option<U256>,
    pub dao_fork_block: Option<U256>,
    pub eip150_block: Option<U256>,
    pub eip158_block: Option<U256>,
    pub byzantium_block: Option<U256>,
    pub constantinople_block: Option<U256>,
    pub petersburg_block: Option<U256>,
    pub istanbul_block: Option<U256>,
    pub berlin_block: Option<U256>,
    pub london_block: Option<U256>,
    pub merge_block: Option<U256>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_chain_config() {
        let config = ChainConfig {
            chain_id: 1,
            homestead_block: Some(U256::from(1_150_000)),
            dao_fork_block: None,
            eip150_block: None,
            eip158_block: None,
            byzantium_block: None,
            constantinople_block: None,
            petersburg_block: None,
            istanbul_block: None,
            berlin_block: None,
            london_block: None,
            merge_block: None,
        };
        
        assert_eq!(config.chain_id, 1);
    }
}