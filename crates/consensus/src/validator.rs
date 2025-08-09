use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Header, Transaction, Receipt};
use ethereum_storage::Database;
use ethereum_crypto::keccak256;
use std::sync::Arc;
use std::collections::HashMap;

use crate::{Result, ConsensusError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    Valid,
    Invalid(String),
    Unknown,
}

/// Block validator for checking block validity
pub struct BlockValidator<D: Database> {
    db: Arc<D>,
}

impl<D: Database> BlockValidator<D> {
    pub fn new(db: Arc<D>) -> Self {
        Self { db }
    }
    
    /// Validate a block
    pub async fn validate(&self, block: &Block) -> Result<ValidationResult> {
        // 1. Validate header
        self.validate_header(&block.header)?;
        
        // 2. Validate body
        self.validate_body(block)?;
        
        // 3. Validate state transition
        self.validate_state_transition(block).await?;
        
        Ok(ValidationResult::Valid)
    }
    
    /// Validate block header
    fn validate_header(&self, header: &Header) -> Result<()> {
        // Check timestamp
        if header.timestamp == 0 {
            return Err(ConsensusError::InvalidBlock(
                "Invalid timestamp".to_string()
            ));
        }
        
        // Check gas limit
        if header.gas_limit == U256::zero() {
            return Err(ConsensusError::InvalidBlock(
                "Gas limit cannot be zero".to_string()
            ));
        }
        
        if header.gas_used > header.gas_limit {
            return Err(ConsensusError::InvalidBlock(
                "Gas used exceeds gas limit".to_string()
            ));
        }
        
        // Check extra data size (max 32 bytes)
        if header.extra_data.len() > 32 {
            return Err(ConsensusError::InvalidBlock(
                "Extra data too large".to_string()
            ));
        }
        
        // Validate parent hash exists (except for genesis)
        if header.number > U256::zero() {
            let parent_key = format!("header:{}", hex::encode(header.parent_hash));
            if self.db.get(parent_key.as_bytes())?.is_none() {
                return Err(ConsensusError::InvalidBlock(
                    "Parent block not found".to_string()
                ));
            }
        }
        
        Ok(())
    }
    
    /// Validate block body
    fn validate_body(&self, block: &Block) -> Result<()> {
        // Calculate transaction root
        let tx_root = self.calculate_transaction_root(&block.body.transactions);
        if tx_root != block.header.transactions_root {
            return Err(ConsensusError::InvalidBlock(
                "Transaction root mismatch".to_string()
            ));
        }
        
        // Calculate uncle hash
        let uncle_hash = self.calculate_uncle_hash(&block.body.uncles);
        if uncle_hash != block.header.uncles_hash {
            return Err(ConsensusError::InvalidBlock(
                "Uncle hash mismatch".to_string()
            ));
        }
        
        // Validate transactions
        for tx in &block.body.transactions {
            self.validate_transaction(tx)?;
        }
        
        // Validate uncles
        for uncle in &block.body.uncles {
            self.validate_uncle(uncle, &block.header)?;
        }
        
        Ok(())
    }
    
    /// Validate individual transaction
    fn validate_transaction(&self, tx: &Transaction) -> Result<()> {
        // Check signature validity
        if !tx.signature.is_valid() {
            return Err(ConsensusError::InvalidBlock(
                "Invalid transaction signature".to_string()
            ));
        }
        
        // Check gas price
        if tx.gas_price == U256::zero() && tx.max_fee_per_gas.is_none() {
            return Err(ConsensusError::InvalidBlock(
                "Transaction must have gas price".to_string()
            ));
        }
        
        // Check gas limit
        if tx.gas_limit == U256::zero() {
            return Err(ConsensusError::InvalidBlock(
                "Transaction gas limit cannot be zero".to_string()
            ));
        }
        
        // Check nonce (would need account state)
        // Check balance (would need account state)
        
        Ok(())
    }
    
    /// Validate uncle block
    fn validate_uncle(&self, uncle: &Header, parent: &Header) -> Result<()> {
        // Uncle must be within 7 blocks of parent
        let max_uncle_depth = U256::from(7);
        
        if uncle.number >= parent.number {
            return Err(ConsensusError::InvalidBlock(
                "Uncle block number too high".to_string()
            ));
        }
        
        if parent.number - uncle.number > max_uncle_depth {
            return Err(ConsensusError::InvalidBlock(
                "Uncle too old".to_string()
            ));
        }
        
        // Uncle cannot be direct ancestor
        // Additional uncle validation...
        
        Ok(())
    }
    
    /// Validate state transition
    async fn validate_state_transition(&self, block: &Block) -> Result<()> {
        // This would execute all transactions and verify state root
        // For now, we'll do basic validation
        
        // Check that receipts root matches
        if block.header.receipts_root != self.calculate_receipts_root(&[]) {
            // In real implementation, would calculate from actual receipts
        }
        
        // Check state root matches after execution
        // This requires full EVM execution
        
        Ok(())
    }
    
    /// Calculate transaction root from transactions
    fn calculate_transaction_root(&self, transactions: &[Transaction]) -> H256 {
        if transactions.is_empty() {
            return H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6,
                              0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
                              0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0,
                              0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]);
        }
        
        // Build Merkle Patricia Trie of transactions
        // For now, return hash of concatenated tx hashes
        let mut data = Vec::new();
        for tx in transactions {
            data.extend_from_slice(&tx.hash().0);
        }
        H256(keccak256(&data))
    }
    
    /// Calculate uncle hash from uncle headers
    fn calculate_uncle_hash(&self, uncles: &[Header]) -> H256 {
        if uncles.is_empty() {
            return H256::from([0x1d, 0xcc, 0x4d, 0xe8, 0xde, 0xc7, 0x5d, 0x7a,
                              0xab, 0x85, 0xb5, 0x67, 0xb6, 0xcc, 0xd4, 0x1a,
                              0xd3, 0x12, 0x45, 0x1b, 0x94, 0x8a, 0x74, 0x13,
                              0xf0, 0xa1, 0x42, 0xfd, 0x40, 0xd4, 0x93, 0x47]);
        }
        
        let mut data = Vec::new();
        for uncle in uncles {
            data.extend_from_slice(&uncle.hash().0);
        }
        H256(keccak256(&data))
    }
    
    /// Calculate receipts root from receipts
    fn calculate_receipts_root(&self, receipts: &[Receipt]) -> H256 {
        if receipts.is_empty() {
            return H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6,
                              0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
                              0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0,
                              0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]);
        }
        
        // Build Merkle Patricia Trie of receipts
        H256::zero()
    }
    
    /// Check if a block is valid for import
    pub async fn check_block_import(&self, block: &Block) -> Result<bool> {
        // Check if block already exists
        let hash = block.header.hash();
        let key = format!("header:{}", hex::encode(hash));
        
        if self.db.get(key.as_bytes())?.is_some() {
            return Ok(false); // Block already exists
        }
        
        // Validate block
        let result = self.validate(block).await?;
        
        Ok(result == ValidationResult::Valid)
    }
}

/// Transaction validator
pub struct TransactionValidator {
    chain_id: u64,
}

impl TransactionValidator {
    pub fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }
    
    /// Validate transaction for mempool inclusion
    pub fn validate_for_mempool(&self, tx: &Transaction) -> Result<()> {
        // Check basic transaction validity
        if !tx.signature.is_valid() {
            return Err(ConsensusError::InvalidBlock(
                "Invalid signature".to_string()
            ));
        }
        
        // Check chain ID (EIP-155)
        if let Some(chain_id) = tx.chain_id {
            if chain_id != self.chain_id {
                return Err(ConsensusError::InvalidBlock(
                    format!("Wrong chain ID: expected {}, got {}", 
                            self.chain_id, chain_id)
                ));
            }
        }
        
        // Check gas limit
        if tx.gas_limit > U256::from(30_000_000) {
            return Err(ConsensusError::InvalidBlock(
                "Gas limit too high".to_string()
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validation_result() {
        let result = ValidationResult::Valid;
        assert_eq!(result, ValidationResult::Valid);
        
        let invalid = ValidationResult::Invalid("test".to_string());
        assert_ne!(invalid, ValidationResult::Valid);
    }
}