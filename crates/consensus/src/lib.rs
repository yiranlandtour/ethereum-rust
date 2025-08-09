use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Header, Transaction};
use ethereum_storage::Database;
use std::sync::Arc;
use std::collections::HashMap;
use thiserror::Error;

pub mod engine;
pub mod validator;
pub mod fork_choice;
pub mod pos;
pub mod clique;

pub use engine::{ConsensusEngine, EngineError};
pub use validator::{BlockValidator, ValidationResult};
pub use fork_choice::{ForkChoice, ForkChoiceRule};
pub use pos::ProofOfStake;
pub use clique::Clique;

#[derive(Debug, Error)]
pub enum ConsensusError {
    #[error("Invalid block: {0}")]
    InvalidBlock(String),
    
    #[error("Invalid signature: {0}")]
    InvalidSignature(String),
    
    #[error("Invalid validator: {0}")]
    InvalidValidator(String),
    
    #[error("Fork choice error: {0}")]
    ForkChoiceError(String),
    
    #[error("Engine error: {0}")]
    EngineError(#[from] EngineError),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] ethereum_storage::StorageError),
}

pub type Result<T> = std::result::Result<T, ConsensusError>;

/// Main consensus configuration
#[derive(Debug, Clone)]
pub struct ConsensusConfig {
    pub engine_type: EngineType,
    pub epoch_length: u64,
    pub block_period: u64,
    pub validators: Vec<Address>,
    pub genesis_validators: Vec<Address>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineType {
    ProofOfStake,
    ProofOfAuthority,
    Clique,
}

/// Main consensus interface
pub struct Consensus<D: Database> {
    engine: Box<dyn ConsensusEngine>,
    validator: BlockValidator<D>,
    fork_choice: ForkChoice<D>,
    config: ConsensusConfig,
    db: Arc<D>,
}

impl<D: Database + 'static> Consensus<D> {
    pub fn new(
        config: ConsensusConfig,
        db: Arc<D>,
    ) -> Self {
        let engine: Box<dyn ConsensusEngine> = match config.engine_type {
            EngineType::ProofOfStake => {
                Box::new(ProofOfStake::new(config.clone()))
            }
            EngineType::Clique => {
                Box::new(Clique::new(config.clone()))
            }
            EngineType::ProofOfAuthority => {
                // For now, use Clique for PoA
                Box::new(Clique::new(config.clone()))
            }
        };
        
        let validator = BlockValidator::new(db.clone());
        let fork_choice = ForkChoice::new(db.clone());
        
        Self {
            engine,
            validator,
            fork_choice,
            config,
            db,
        }
    }
    
    /// Validate a block according to consensus rules
    pub async fn validate_block(&self, block: &Block) -> Result<ValidationResult> {
        // Engine-specific validation
        self.engine.validate_block(block)?;
        
        // General block validation
        let result = self.validator.validate(block).await?;
        
        Ok(result)
    }
    
    /// Produce a new block
    pub async fn produce_block(
        &self,
        parent: &Header,
        transactions: Vec<Transaction>,
        beneficiary: Address,
    ) -> Result<Block> {
        let block = self.engine.produce_block(
            parent,
            transactions,
            beneficiary,
        ).await?;
        
        Ok(block)
    }
    
    /// Apply fork choice rule to select canonical chain
    pub async fn apply_fork_choice(
        &self,
        blocks: Vec<Block>,
    ) -> Result<Block> {
        let canonical = self.fork_choice.select_head(blocks).await?;
        Ok(canonical)
    }
    
    /// Get current validators
    pub fn get_validators(&self) -> Vec<Address> {
        self.engine.get_validators()
    }
    
    /// Check if an address is a validator
    pub fn is_validator(&self, address: &Address) -> bool {
        self.engine.is_validator(address)
    }
    
    /// Finalize a block
    pub async fn finalize_block(&self, block: &Block) -> Result<()> {
        self.engine.finalize(block).await?;
        
        // Store finalized block
        let key = format!("finalized:{}", hex::encode(block.header.hash()));
        self.db.put(
            key.as_bytes(),
            &bincode::serialize(block).unwrap(),
        )?;
        
        Ok(())
    }
    
    /// Get finality information
    pub async fn get_finality_info(&self) -> Result<FinalityInfo> {
        let finalized = self.get_finalized_block().await?;
        let justified = self.get_justified_block().await?;
        
        Ok(FinalityInfo {
            finalized_block: finalized,
            justified_block: justified,
            finalized_epoch: self.calculate_epoch(finalized.header.number),
            justified_epoch: self.calculate_epoch(justified.header.number),
        })
    }
    
    async fn get_finalized_block(&self) -> Result<Block> {
        // Get latest finalized block from database
        // For now, return a mock block
        Ok(Block::default())
    }
    
    async fn get_justified_block(&self) -> Result<Block> {
        // Get latest justified block from database
        Ok(Block::default())
    }
    
    fn calculate_epoch(&self, block_number: U256) -> u64 {
        (block_number / U256::from(self.config.epoch_length)).as_u64()
    }
}

#[derive(Debug, Clone)]
pub struct FinalityInfo {
    pub finalized_block: Block,
    pub justified_block: Block,
    pub finalized_epoch: u64,
    pub justified_epoch: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_consensus_config() {
        let config = ConsensusConfig {
            engine_type: EngineType::ProofOfStake,
            epoch_length: 32,
            block_period: 12,
            validators: vec![],
            genesis_validators: vec![],
        };
        
        assert_eq!(config.engine_type, EngineType::ProofOfStake);
        assert_eq!(config.epoch_length, 32);
    }
}
