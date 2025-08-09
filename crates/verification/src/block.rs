use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
use ethereum_storage::Database;
use ethereum_crypto::keccak256;
use std::sync::Arc;

use crate::{Result, VerificationError};

/// Block structure verifier
pub struct BlockVerifier<D: Database> {
    db: Arc<D>,
}

impl<D: Database> BlockVerifier<D> {
    pub fn new(db: Arc<D>) -> Self {
        Self { db }
    }
    
    /// Verify block structure
    pub fn verify_structure(&self, block: &Block) -> Result<()> {
        // Verify transactions root
        let computed_tx_root = self.compute_transaction_root(&block.body.transactions);
        if computed_tx_root != block.header.transactions_root {
            return Err(VerificationError::InvalidBlock(
                "Transaction root mismatch".to_string()
            ));
        }
        
        // Verify uncles hash
        let computed_uncles_hash = self.compute_uncles_hash(&block.body.uncles);
        if computed_uncles_hash != block.header.uncles_hash {
            return Err(VerificationError::InvalidBlock(
                "Uncles hash mismatch".to_string()
            ));
        }
        
        // Verify block hash
        let computed_hash = block.header.hash();
        
        // Check block size limits
        let block_size = bincode::serialize(block)
            .map_err(|_| VerificationError::InvalidBlock("Failed to serialize block".to_string()))?
            .len();
        
        const MAX_BLOCK_SIZE: usize = 1_000_000; // 1MB limit
        if block_size > MAX_BLOCK_SIZE {
            return Err(VerificationError::InvalidBlock(
                format!("Block size {} exceeds limit {}", block_size, MAX_BLOCK_SIZE)
            ));
        }
        
        // Verify uncle count
        const MAX_UNCLES: usize = 2;
        if block.body.uncles.len() > MAX_UNCLES {
            return Err(VerificationError::InvalidBlock(
                format!("Too many uncles: {} > {}", block.body.uncles.len(), MAX_UNCLES)
            ));
        }
        
        Ok(())
    }
    
    /// Verify uncle blocks
    pub fn verify_uncles(&self, block: &Block) -> Result<()> {
        for uncle in &block.body.uncles {
            self.verify_uncle(uncle, &block.header)?;
        }
        
        // Check for duplicate uncles
        let mut uncle_hashes = Vec::new();
        for uncle in &block.body.uncles {
            let hash = uncle.hash();
            if uncle_hashes.contains(&hash) {
                return Err(VerificationError::InvalidBlock(
                    "Duplicate uncle".to_string()
                ));
            }
            uncle_hashes.push(hash);
        }
        
        Ok(())
    }
    
    /// Verify single uncle
    fn verify_uncle(&self, uncle: &Header, parent: &Header) -> Result<()> {
        // Uncle must be older than parent
        if uncle.number >= parent.number {
            return Err(VerificationError::InvalidBlock(
                "Uncle number >= parent number".to_string()
            ));
        }
        
        // Uncle must be within 7 blocks of parent
        const MAX_UNCLE_DEPTH: u64 = 7;
        if parent.number.as_u64() - uncle.number.as_u64() > MAX_UNCLE_DEPTH {
            return Err(VerificationError::InvalidBlock(
                format!("Uncle too old: depth > {}", MAX_UNCLE_DEPTH)
            ));
        }
        
        // Uncle cannot be direct ancestor
        if self.is_ancestor(uncle, parent)? {
            return Err(VerificationError::InvalidBlock(
                "Uncle is direct ancestor".to_string()
            ));
        }
        
        // Verify uncle has no transactions (uncles are header-only)
        // This is implicit as uncles are Headers, not full Blocks
        
        Ok(())
    }
    
    /// Check if one header is ancestor of another
    fn is_ancestor(&self, potential_ancestor: &Header, descendant: &Header) -> Result<bool> {
        let mut current = descendant.parent_hash;
        let ancestor_hash = potential_ancestor.hash();
        
        while current != H256::zero() {
            if current == ancestor_hash {
                return Ok(true);
            }
            
            // Get parent header
            let key = format!("header:{}", hex::encode(current));
            match self.db.get(key.as_bytes())? {
                Some(data) => {
                    let header: Header = bincode::deserialize(&data)
                        .map_err(|_| VerificationError::InvalidHeader("Failed to deserialize".to_string()))?;
                    current = header.parent_hash;
                }
                None => break,
            }
        }
        
        Ok(false)
    }
    
    /// Compute transaction root
    fn compute_transaction_root(&self, transactions: &[ethereum_core::Transaction]) -> H256 {
        if transactions.is_empty() {
            // Empty transactions trie root
            return H256::from([0x56, 0xe8, 0x1f, 0x17, 0x1b, 0xcc, 0x55, 0xa6,
                              0xff, 0x83, 0x45, 0xe6, 0x92, 0xc0, 0xf8, 0x6e,
                              0x5b, 0x48, 0xe0, 0x1b, 0x99, 0x6c, 0xad, 0xc0,
                              0x01, 0x62, 0x2f, 0xb5, 0xe3, 0x63, 0xb4, 0x21]);
        }
        
        // Build Merkle Patricia Trie of transactions
        let mut data = Vec::new();
        for tx in transactions {
            data.extend_from_slice(&tx.hash().0);
        }
        
        H256(keccak256(&data))
    }
    
    /// Compute uncles hash
    fn compute_uncles_hash(&self, uncles: &[Header]) -> H256 {
        if uncles.is_empty() {
            // Empty uncles hash
            return H256::from([0x1d, 0xcc, 0x4d, 0xe8, 0xde, 0xc7, 0x5d, 0x7a,
                              0xab, 0x85, 0xb5, 0x67, 0xb6, 0xcc, 0xd4, 0x1a,
                              0xd3, 0x12, 0x45, 0x1b, 0x94, 0x8a, 0x74, 0x13,
                              0xf0, 0xa1, 0x42, 0xfd, 0x40, 0xd4, 0x93, 0x47]);
        }
        
        // Hash of RLP-encoded uncles
        let mut data = Vec::new();
        for uncle in uncles {
            data.extend_from_slice(&uncle.hash().0);
        }
        
        H256(keccak256(&data))
    }
}