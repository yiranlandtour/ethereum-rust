use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Header, Transaction};
use ethereum_crypto::{Signature, recover_address};
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};

use crate::{Result, ConsensusError, ConsensusConfig};
use crate::engine::{ConsensusEngine, EngineError};

/// Clique Proof of Authority consensus implementation
pub struct Clique {
    config: ConsensusConfig,
    signers: Vec<Address>,
    recent_signers: VecDeque<(U256, Address)>,
    proposals: HashMap<Address, bool>, // true = add, false = remove
    votes: HashMap<Address, HashMap<Address, bool>>,
}

impl Clique {
    pub fn new(config: ConsensusConfig) -> Self {
        let signers = config.validators.clone();
        
        Self {
            config,
            signers,
            recent_signers: VecDeque::new(),
            proposals: HashMap::new(),
            votes: HashMap::new(),
        }
    }
    
    /// Check if a signer is authorized
    fn is_authorized(&self, signer: &Address) -> bool {
        self.signers.contains(signer)
    }
    
    /// Get the signer of a block
    fn get_signer(&self, header: &Header) -> Result<Address> {
        // Extract signature from extra data
        if header.extra_data.len() < 65 {
            return Err(ConsensusError::InvalidSignature(
                "Missing signature in extra data".to_string()
            ));
        }
        
        let extra_len = header.extra_data.len();
        let sig_bytes = &header.extra_data[extra_len - 65..];
        
        let signature = Signature::from_bytes(sig_bytes)
            .map_err(|_| ConsensusError::InvalidSignature("Invalid signature format".to_string()))?;
        
        // Create signing hash (header without signature)
        let signing_hash = self.signing_hash(header);
        
        // Recover signer address
        let signer = recover_address(&signing_hash, &signature)
            .map_err(|_| ConsensusError::InvalidSignature("Failed to recover signer".to_string()))?;
        
        Ok(signer)
    }
    
    /// Calculate signing hash for a header
    fn signing_hash(&self, header: &Header) -> [u8; 32] {
        // Create a copy of header without signature for hashing
        let mut signing_header = header.clone();
        
        // Remove signature from extra data
        if signing_header.extra_data.len() >= 65 {
            let new_len = signing_header.extra_data.len() - 65;
            signing_header.extra_data.truncate(new_len);
        }
        
        ethereum_crypto::keccak256(&bincode::serialize(&signing_header).unwrap())
    }
    
    /// Check if a signer has signed recently
    fn has_signed_recently(&self, signer: &Address, block_number: U256) -> bool {
        let limit = (self.signers.len() / 2) as u64;
        
        for (num, recent_signer) in &self.recent_signers {
            if block_number - num <= U256::from(limit) && recent_signer == signer {
                return true;
            }
        }
        
        false
    }
    
    /// Update recent signers list
    fn update_recent_signers(&mut self, block_number: U256, signer: Address) {
        // Add new signer
        self.recent_signers.push_back((block_number, signer));
        
        // Remove old signers outside the window
        let limit = (self.signers.len() / 2) as u64;
        while let Some((num, _)) = self.recent_signers.front() {
            if block_number - num > U256::from(limit) {
                self.recent_signers.pop_front();
            } else {
                break;
            }
        }
    }
    
    /// Process voting proposal in block
    fn process_vote(&mut self, header: &Header, signer: Address) -> Result<()> {
        // Check if header contains a vote (non-zero beneficiary)
        if header.author == Address::zero() {
            return Ok(()); // No vote
        }
        
        let proposal = header.author;
        let vote = header.nonce != 0; // nonce != 0 means add, nonce == 0 means remove
        
        // Record vote
        self.votes.entry(signer)
            .or_insert_with(HashMap::new)
            .insert(proposal, vote);
        
        // Check if proposal has enough votes
        let threshold = (self.signers.len() / 2) + 1;
        let mut add_votes = 0;
        let mut remove_votes = 0;
        
        for (_, votes) in &self.votes {
            if let Some(&v) = votes.get(&proposal) {
                if v {
                    add_votes += 1;
                } else {
                    remove_votes += 1;
                }
            }
        }
        
        // Apply changes if threshold reached
        if add_votes >= threshold && !self.signers.contains(&proposal) {
            self.signers.push(proposal);
            self.clear_votes_for(&proposal);
            tracing::info!("Added new signer: {:?}", proposal);
        } else if remove_votes >= threshold && self.signers.contains(&proposal) {
            self.signers.retain(|s| s != &proposal);
            self.clear_votes_for(&proposal);
            tracing::info!("Removed signer: {:?}", proposal);
        }
        
        Ok(())
    }
    
    /// Clear all votes for a specific address
    fn clear_votes_for(&mut self, address: &Address) {
        for votes in self.votes.values_mut() {
            votes.remove(address);
        }
    }
    
    /// Calculate the next timestamp when a signer can produce a block
    fn calculate_next_timestamp(&self, parent: &Header, signer: &Address) -> u64 {
        let period = self.config.block_period;
        let parent_time = parent.timestamp;
        
        // Check if signer is in-turn
        let signer_index = self.signers.iter().position(|s| s == signer).unwrap_or(0);
        let turn = (parent.number.as_u64() + 1) % self.signers.len() as u64;
        
        if signer_index == turn as usize {
            // In-turn signer can produce immediately
            parent_time + period
        } else {
            // Out-of-turn signers must wait additional time
            parent_time + period + (period / 2)
        }
    }
}

#[async_trait]
impl ConsensusEngine for Clique {
    fn validate_block(&self, block: &Block) -> Result<()> {
        let header = &block.header;
        
        // Get block signer
        let signer = self.get_signer(header)?;
        
        // Check if signer is authorized
        if !self.is_authorized(&signer) {
            return Err(ConsensusError::InvalidBlock(
                format!("Unauthorized signer: {:?}", signer)
            ));
        }
        
        // Check if signer has signed recently
        if self.has_signed_recently(&signer, header.number) {
            return Err(ConsensusError::InvalidBlock(
                "Signer has signed too recently".to_string()
            ));
        }
        
        // Validate timestamp
        if header.number > U256::zero() {
            let period = self.config.block_period;
            
            // Check minimum time between blocks
            if let Ok(parent_header) = self.get_parent_header(header) {
                if header.timestamp < parent_header.timestamp + period {
                    return Err(ConsensusError::InvalidBlock(
                        "Block too early".to_string()
                    ));
                }
            }
        }
        
        // Validate difficulty (should be 1 or 2 in Clique)
        if header.difficulty != U256::from(1) && header.difficulty != U256::from(2) {
            return Err(ConsensusError::InvalidBlock(
                "Invalid difficulty for Clique".to_string()
            ));
        }
        
        Ok(())
    }
    
    fn verify_seal(&self, header: &Header) -> Result<()> {
        // Verify signature exists and is valid
        let _ = self.get_signer(header)?;
        Ok(())
    }
    
    async fn produce_block(
        &self,
        parent: &Header,
        transactions: Vec<Transaction>,
        beneficiary: Address,
    ) -> Result<Block> {
        // Calculate difficulty (1 for in-turn, 2 for out-of-turn)
        let block_number = parent.number + U256::one();
        let turn = (block_number.as_u64()) % self.signers.len() as u64;
        let difficulty = if self.signers[turn as usize] == beneficiary {
            U256::from(2) // In-turn
        } else {
            U256::from(1) // Out-of-turn
        };
        
        let header = Header {
            parent_hash: parent.hash(),
            uncles_hash: H256::from([0x1d, 0xcc, 0x4d, 0xe8, 0xde, 0xc7, 0x5d, 0x7a,
                                     0xab, 0x85, 0xb5, 0x67, 0xb6, 0xcc, 0xd4, 0x1a,
                                     0xd3, 0x12, 0x45, 0x1b, 0x94, 0x8a, 0x74, 0x13,
                                     0xf0, 0xa1, 0x42, 0xfd, 0x40, 0xd4, 0x93, 0x47]),
            author: beneficiary,
            state_root: H256::zero(),
            transactions_root: H256::zero(),
            receipts_root: H256::zero(),
            bloom: Default::default(),
            difficulty,
            number: block_number,
            gas_limit: parent.gas_limit,
            gas_used: U256::zero(),
            timestamp: self.calculate_next_timestamp(parent, &beneficiary),
            extra_data: self.extra_data(),
            mix_hash: H256::zero(),
            nonce: 0,
        };
        
        let body = ethereum_core::BlockBody {
            transactions,
            uncles: vec![], // No uncles in Clique
        };
        
        Ok(Block { header, body })
    }
    
    async fn seal_block(&self, mut block: Block) -> Result<Block> {
        // Sign the block
        let signing_hash = self.signing_hash(&block.header);
        
        // In real implementation, would sign with signer's private key
        let signature = Signature::default();
        
        // Append signature to extra data
        block.header.extra_data.extend_from_slice(&signature.to_bytes());
        
        Ok(block)
    }
    
    fn get_validators(&self) -> Vec<Address> {
        self.signers.clone()
    }
    
    fn is_validator(&self, address: &Address) -> bool {
        self.signers.contains(address)
    }
    
    async fn finalize(&self, _block: &Block) -> Result<()> {
        // No explicit finalization in Clique
        Ok(())
    }
    
    fn block_reward(&self, _block_number: U256) -> U256 {
        // No block rewards in Clique PoA
        U256::zero()
    }
    
    fn is_ready(&self) -> bool {
        !self.signers.is_empty()
    }
    
    fn extra_data(&self) -> Vec<u8> {
        // Clique extra data format: vanity (32 bytes) + signers (for genesis) + signature (65 bytes)
        let mut data = vec![0u8; 32]; // Vanity
        data
    }
    
    fn calculate_difficulty(&self, parent: &Header, _timestamp: u64) -> U256 {
        // Difficulty is 1 or 2 based on whether signer is in-turn
        let block_number = parent.number + U256::one();
        let turn = (block_number.as_u64()) % self.signers.len() as u64;
        
        // Without knowing the actual signer, default to out-of-turn
        U256::from(1)
    }
}

impl Clique {
    /// Helper to get parent header
    fn get_parent_header(&self, header: &Header) -> Result<Header> {
        // In real implementation, would fetch from database
        // For now, return a mock header
        Ok(Header {
            timestamp: header.timestamp.saturating_sub(self.config.block_period),
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clique_initialization() {
        let config = ConsensusConfig {
            engine_type: crate::EngineType::Clique,
            epoch_length: 30000,
            block_period: 15,
            validators: vec![
                Address::from([1u8; 20]),
                Address::from([2u8; 20]),
            ],
            genesis_validators: vec![],
        };
        
        let clique = Clique::new(config);
        
        assert_eq!(clique.signers.len(), 2);
        assert!(clique.is_authorized(&Address::from([1u8; 20])));
        assert!(clique.is_authorized(&Address::from([2u8; 20])));
        assert!(!clique.is_authorized(&Address::from([3u8; 20])));
    }
}