use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Header, Transaction};
use thiserror::Error;
use async_trait::async_trait;

use crate::Result;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Invalid seal: {0}")]
    InvalidSeal(String),
    
    #[error("Invalid difficulty: {0}")]
    InvalidDifficulty(String),
    
    #[error("Invalid timestamp: {0}")]
    InvalidTimestamp(String),
    
    #[error("Unauthorized block producer")]
    UnauthorizedProducer,
    
    #[error("Engine not ready")]
    NotReady,
}

/// Consensus engine trait that all consensus mechanisms must implement
#[async_trait]
pub trait ConsensusEngine: Send + Sync {
    /// Validate a block according to consensus rules
    fn validate_block(&self, block: &Block) -> Result<()>;
    
    /// Verify block seal (signature/proof)
    fn verify_seal(&self, header: &Header) -> Result<()>;
    
    /// Produce a new block
    async fn produce_block(
        &self,
        parent: &Header,
        transactions: Vec<Transaction>,
        beneficiary: Address,
    ) -> Result<Block>;
    
    /// Seal a block (add signature/proof)
    async fn seal_block(&self, block: Block) -> Result<Block>;
    
    /// Get current validators/block producers
    fn get_validators(&self) -> Vec<Address>;
    
    /// Check if an address is a validator
    fn is_validator(&self, address: &Address) -> bool;
    
    /// Finalize a block
    async fn finalize(&self, block: &Block) -> Result<()>;
    
    /// Get block reward for a given block
    fn block_reward(&self, block_number: U256) -> U256;
    
    /// Check if engine is ready to produce blocks
    fn is_ready(&self) -> bool;
    
    /// Get consensus-specific extra data for block header
    fn extra_data(&self) -> Vec<u8>;
    
    /// Calculate difficulty for next block
    fn calculate_difficulty(&self, parent: &Header, timestamp: u64) -> U256;
}

/// Consensus parameters for different networks
#[derive(Debug, Clone)]
pub struct ChainSpec {
    pub chain_id: u64,
    pub genesis_hash: H256,
    pub byzantium_block: Option<U256>,
    pub constantinople_block: Option<U256>,
    pub petersburg_block: Option<U256>,
    pub istanbul_block: Option<U256>,
    pub berlin_block: Option<U256>,
    pub london_block: Option<U256>,
    pub merge_block: Option<U256>,
    pub shanghai_block: Option<U256>,
}

impl Default for ChainSpec {
    fn default() -> Self {
        // Ethereum mainnet parameters
        Self {
            chain_id: 1,
            genesis_hash: H256::zero(),
            byzantium_block: Some(U256::from(4_370_000)),
            constantinople_block: Some(U256::from(7_280_000)),
            petersburg_block: Some(U256::from(7_280_000)),
            istanbul_block: Some(U256::from(9_069_000)),
            berlin_block: Some(U256::from(12_244_000)),
            london_block: Some(U256::from(12_965_000)),
            merge_block: Some(U256::from(15_537_394)),
            shanghai_block: Some(U256::from(17_034_870)),
        }
    }
}

/// Check if a fork is active at a given block
pub fn is_fork_active(fork_block: Option<U256>, block_number: U256) -> bool {
    fork_block.map_or(false, |fork| block_number >= fork)
}