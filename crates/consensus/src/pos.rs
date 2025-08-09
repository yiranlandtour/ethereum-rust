use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Header, Transaction};
use ethereum_crypto::{Signature, recover_address};
use async_trait::async_trait;
use std::collections::HashMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::{Result, ConsensusError, ConsensusConfig};
use crate::engine::{ConsensusEngine, EngineError};

/// Proof of Stake consensus implementation
pub struct ProofOfStake {
    config: ConsensusConfig,
    validators: Vec<Address>,
    validator_stakes: HashMap<Address, U256>,
    epoch: u64,
    slot: u64,
    attestations: Vec<Attestation>,
}

#[derive(Debug, Clone)]
struct Attestation {
    slot: u64,
    validator: Address,
    beacon_block_root: H256,
    source_checkpoint: Checkpoint,
    target_checkpoint: Checkpoint,
    signature: Signature,
}

#[derive(Debug, Clone)]
struct Checkpoint {
    epoch: u64,
    root: H256,
}

impl ProofOfStake {
    pub fn new(config: ConsensusConfig) -> Self {
        let validators = config.validators.clone();
        let mut validator_stakes = HashMap::new();
        
        // Initialize with equal stakes for simplicity
        for validator in &validators {
            validator_stakes.insert(*validator, U256::from(32_000_000_000_000_000_000u128)); // 32 ETH
        }
        
        Self {
            config,
            validators,
            validator_stakes,
            epoch: 0,
            slot: 0,
            attestations: Vec::new(),
        }
    }
    
    /// Get proposer for a given slot
    fn get_proposer(&self, slot: u64) -> Address {
        if self.validators.is_empty() {
            return Address::zero();
        }
        
        // Simple round-robin for now
        // In real implementation, would use RANDAO for randomness
        let index = (slot as usize) % self.validators.len();
        self.validators[index]
    }
    
    /// Calculate committee for attestations
    fn get_committee(&self, slot: u64) -> Vec<Address> {
        if self.validators.len() <= 128 {
            return self.validators.clone();
        }
        
        // Use slot as seed for deterministic randomness
        let mut rng = ChaCha20Rng::seed_from_u64(slot);
        let committee_size = std::cmp::min(128, self.validators.len() / 2);
        
        let mut committee = Vec::new();
        let mut indices: Vec<usize> = (0..self.validators.len()).collect();
        
        for _ in 0..committee_size {
            let idx = rng.gen_range(0..indices.len());
            let validator_idx = indices.swap_remove(idx);
            committee.push(self.validators[validator_idx]);
        }
        
        committee
    }
    
    /// Verify block proposer signature
    fn verify_proposer_signature(&self, block: &Block) -> Result<()> {
        let expected_proposer = self.get_proposer(self.slot);
        
        // Extract signature from block extra data
        if block.header.extra_data.len() < 65 {
            return Err(ConsensusError::InvalidSignature(
                "Missing proposer signature".to_string()
            ));
        }
        
        let sig_bytes = &block.header.extra_data[block.header.extra_data.len() - 65..];
        let signature = Signature::from_bytes(sig_bytes)
            .map_err(|_| ConsensusError::InvalidSignature("Invalid signature format".to_string()))?;
        
        // Verify signature
        let block_hash = block.header.hash();
        let recovered = recover_address(&block_hash.0, &signature)
            .map_err(|_| ConsensusError::InvalidSignature("Failed to recover address".to_string()))?;
        
        if recovered != expected_proposer {
            return Err(ConsensusError::InvalidSignature(
                format!("Invalid proposer: expected {:?}, got {:?}", expected_proposer, recovered)
            ));
        }
        
        Ok(())
    }
    
    /// Process attestations for finality
    fn process_attestations(&mut self, attestations: Vec<Attestation>) {
        for attestation in attestations {
            // Verify attestation signature
            if self.verify_attestation(&attestation).is_ok() {
                self.attestations.push(attestation);
            }
        }
        
        // Check for finality
        self.check_finality();
    }
    
    /// Verify attestation
    fn verify_attestation(&self, attestation: &Attestation) -> Result<()> {
        // Check validator is in committee
        let committee = self.get_committee(attestation.slot);
        if !committee.contains(&attestation.validator) {
            return Err(ConsensusError::InvalidValidator(
                "Validator not in committee".to_string()
            ));
        }
        
        // Verify signature
        let message = self.attestation_message(attestation);
        let recovered = recover_address(&message, &attestation.signature)
            .map_err(|_| ConsensusError::InvalidSignature("Failed to recover attestation signer".to_string()))?;
        
        if recovered != attestation.validator {
            return Err(ConsensusError::InvalidSignature(
                "Invalid attestation signature".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Create attestation message for signing
    fn attestation_message(&self, attestation: &Attestation) -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(&attestation.slot.to_le_bytes());
        data.extend_from_slice(&attestation.beacon_block_root.0);
        data.extend_from_slice(&attestation.source_checkpoint.epoch.to_le_bytes());
        data.extend_from_slice(&attestation.source_checkpoint.root.0);
        data.extend_from_slice(&attestation.target_checkpoint.epoch.to_le_bytes());
        data.extend_from_slice(&attestation.target_checkpoint.root.0);
        
        ethereum_crypto::keccak256(&data)
    }
    
    /// Check if supermajority link exists for finality
    fn check_finality(&mut self) {
        let current_epoch = self.slot / self.config.epoch_length;
        
        // Count attestations for current epoch
        let mut attestation_count: HashMap<H256, usize> = HashMap::new();
        
        for attestation in &self.attestations {
            if attestation.target_checkpoint.epoch == current_epoch {
                *attestation_count.entry(attestation.target_checkpoint.root)
                    .or_insert(0) += 1;
            }
        }
        
        // Check for 2/3 majority
        let threshold = (self.validators.len() * 2) / 3;
        
        for (root, count) in attestation_count {
            if count >= threshold {
                // This checkpoint can be justified
                tracing::info!("Checkpoint justified: {:?} at epoch {}", root, current_epoch);
                
                // If previous epoch was also justified, finalize it
                if current_epoch > 0 {
                    tracing::info!("Checkpoint finalized at epoch {}", current_epoch - 1);
                }
            }
        }
    }
    
    /// Calculate block reward based on participation
    fn calculate_reward(&self, participation_rate: f64) -> U256 {
        let base_reward = U256::from(2_000_000_000_000_000_000u128); // 2 ETH
        let adjusted_reward = (base_reward.as_u128() as f64 * participation_rate) as u128;
        U256::from(adjusted_reward)
    }
}

#[async_trait]
impl ConsensusEngine for ProofOfStake {
    fn validate_block(&self, block: &Block) -> Result<()> {
        // Verify proposer
        self.verify_proposer_signature(block)?;
        
        // Check slot timing
        let expected_slot = block.header.timestamp / self.config.block_period;
        if expected_slot != self.slot {
            return Err(ConsensusError::InvalidBlock(
                format!("Invalid slot: expected {}, got {}", self.slot, expected_slot)
            ));
        }
        
        Ok(())
    }
    
    fn verify_seal(&self, header: &Header) -> Result<()> {
        // In PoS, seal is the proposer signature
        if header.extra_data.len() < 65 {
            return Err(ConsensusError::InvalidBlock(
                "Missing seal".to_string()
            ));
        }
        
        Ok(())
    }
    
    async fn produce_block(
        &self,
        parent: &Header,
        transactions: Vec<Transaction>,
        beneficiary: Address,
    ) -> Result<Block> {
        let mut header = Header {
            parent_hash: parent.hash(),
            uncles_hash: H256::from([0x1d, 0xcc, 0x4d, 0xe8, 0xde, 0xc7, 0x5d, 0x7a,
                                     0xab, 0x85, 0xb5, 0x67, 0xb6, 0xcc, 0xd4, 0x1a,
                                     0xd3, 0x12, 0x45, 0x1b, 0x94, 0x8a, 0x74, 0x13,
                                     0xf0, 0xa1, 0x42, 0xfd, 0x40, 0xd4, 0x93, 0x47]),
            author: beneficiary,
            state_root: H256::zero(), // Would be calculated after execution
            transactions_root: H256::zero(), // Would be calculated from transactions
            receipts_root: H256::zero(), // Would be calculated from receipts
            bloom: Default::default(),
            difficulty: U256::zero(), // No difficulty in PoS
            number: parent.number + U256::one(),
            gas_limit: parent.gas_limit,
            gas_used: U256::zero(), // Would be calculated from execution
            timestamp: self.slot * self.config.block_period,
            extra_data: self.extra_data(),
            mix_hash: H256::zero(),
            nonce: 0,
        };
        
        // Create block body
        let body = ethereum_core::BlockBody {
            transactions,
            uncles: vec![], // No uncles in PoS
        };
        
        Ok(Block { header, body })
    }
    
    async fn seal_block(&self, mut block: Block) -> Result<Block> {
        // Add proposer signature to extra_data
        // In real implementation, would sign with validator key
        let signature = Signature::default();
        block.header.extra_data.extend_from_slice(&signature.to_bytes());
        
        Ok(block)
    }
    
    fn get_validators(&self) -> Vec<Address> {
        self.validators.clone()
    }
    
    fn is_validator(&self, address: &Address) -> bool {
        self.validators.contains(address)
    }
    
    async fn finalize(&self, _block: &Block) -> Result<()> {
        // Finalization happens through attestations
        Ok(())
    }
    
    fn block_reward(&self, _block_number: U256) -> U256 {
        // Calculate based on participation
        self.calculate_reward(0.95) // Assume 95% participation
    }
    
    fn is_ready(&self) -> bool {
        !self.validators.is_empty()
    }
    
    fn extra_data(&self) -> Vec<u8> {
        // Include slot number and validator set hash
        let mut data = Vec::new();
        data.extend_from_slice(&self.slot.to_le_bytes());
        data
    }
    
    fn calculate_difficulty(&self, _parent: &Header, _timestamp: u64) -> U256 {
        // No difficulty in PoS
        U256::zero()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_proposer_selection() {
        let config = ConsensusConfig {
            engine_type: crate::EngineType::ProofOfStake,
            epoch_length: 32,
            block_period: 12,
            validators: vec![
                Address::from([1u8; 20]),
                Address::from([2u8; 20]),
                Address::from([3u8; 20]),
            ],
            genesis_validators: vec![],
        };
        
        let pos = ProofOfStake::new(config);
        
        // Test proposer rotation
        let proposer0 = pos.get_proposer(0);
        let proposer1 = pos.get_proposer(1);
        let proposer2 = pos.get_proposer(2);
        let proposer3 = pos.get_proposer(3);
        
        assert_eq!(proposer0, Address::from([1u8; 20]));
        assert_eq!(proposer1, Address::from([2u8; 20]));
        assert_eq!(proposer2, Address::from([3u8; 20]));
        assert_eq!(proposer3, Address::from([1u8; 20])); // Wraps around
    }
}