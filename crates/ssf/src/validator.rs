use ethereum_types::{H256, U256, Address};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, debug, warn};
use blst::min_pk::*;

use crate::{Result, SSFError};

/// SSF Validator representation
#[derive(Debug, Clone)]
pub struct SSFValidator {
    index: u64,
    pubkey: PublicKey,
    address: Address,
    stake: U256,
    status: ValidatorStatus,
    performance: ValidatorPerformance,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidatorStatus {
    Active,
    Pending,
    Exiting { exit_epoch: u64 },
    Slashed { slashed_at: u64 },
    Withdrawn,
}

#[derive(Debug, Clone)]
pub struct ValidatorPerformance {
    pub attestations_included: u64,
    pub attestations_missed: u64,
    pub blocks_proposed: u64,
    pub blocks_missed: u64,
    pub participation_rate: f64,
    pub effectiveness: f64,
}

impl Default for ValidatorPerformance {
    fn default() -> Self {
        Self {
            attestations_included: 0,
            attestations_missed: 0,
            blocks_proposed: 0,
            blocks_missed: 0,
            participation_rate: 1.0,
            effectiveness: 1.0,
        }
    }
}

/// Validator set management for SSF
pub struct ValidatorSet {
    validators: Arc<RwLock<HashMap<u64, SSFValidator>>>,
    active_validators: Arc<RwLock<Vec<u64>>>,
    total_stake: Arc<RwLock<U256>>,
    epoch: Arc<RwLock<u64>>,
    shuffling_seed: Arc<RwLock<H256>>,
}

impl ValidatorSet {
    pub fn new() -> Self {
        Self {
            validators: Arc::new(RwLock::new(HashMap::new())),
            active_validators: Arc::new(RwLock::new(Vec::new())),
            total_stake: Arc::new(RwLock::new(U256::zero())),
            epoch: Arc::new(RwLock::new(0)),
            shuffling_seed: Arc::new(RwLock::new(H256::zero())),
        }
    }

    /// Add a validator to the set
    pub fn add_validator(&self, validator: SSFValidator) -> Result<()> {
        let index = validator.index;
        let stake = validator.stake;
        
        {
            let mut validators = self.validators.write().unwrap();
            validators.insert(index, validator);
        }
        
        if self.is_active(index) {
            let mut active = self.active_validators.write().unwrap();
            if !active.contains(&index) {
                active.push(index);
                
                let mut total = self.total_stake.write().unwrap();
                *total = *total + stake;
            }
        }
        
        info!("Added validator {} with stake {}", index, stake);
        Ok(())
    }

    /// Remove a validator from the set
    pub fn remove_validator(&self, index: u64) -> Result<()> {
        let stake = {
            let mut validators = self.validators.write().unwrap();
            validators.remove(&index)
                .map(|v| v.stake)
                .ok_or_else(|| SSFError::ValidatorError("Validator not found".into()))?
        };
        
        {
            let mut active = self.active_validators.write().unwrap();
            if let Some(pos) = active.iter().position(|&x| x == index) {
                active.remove(pos);
                
                let mut total = self.total_stake.write().unwrap();
                *total = *total - stake;
            }
        }
        
        info!("Removed validator {}", index);
        Ok(())
    }

    /// Update validator status
    pub fn update_validator_status(&self, index: u64, status: ValidatorStatus) -> Result<()> {
        let mut validators = self.validators.write().unwrap();
        let validator = validators.get_mut(&index)
            .ok_or_else(|| SSFError::ValidatorError("Validator not found".into()))?;
        
        let was_active = validator.status == ValidatorStatus::Active;
        validator.status = status.clone();
        let is_active_now = status == ValidatorStatus::Active;
        
        // Update active list if status changed
        if was_active != is_active_now {
            drop(validators); // Release lock
            
            let mut active = self.active_validators.write().unwrap();
            if is_active_now {
                if !active.contains(&index) {
                    active.push(index);
                }
            } else {
                if let Some(pos) = active.iter().position(|&x| x == index) {
                    active.remove(pos);
                }
            }
        }
        
        Ok(())
    }

    /// Get validator by index
    pub fn get_validator(&self, index: u64) -> Option<SSFValidator> {
        self.validators.read().unwrap().get(&index).cloned()
    }

    /// Get all active validators
    pub fn get_active_validators(&self) -> Vec<SSFValidator> {
        let validators = self.validators.read().unwrap();
        let active = self.active_validators.read().unwrap();
        
        active.iter()
            .filter_map(|&index| validators.get(&index).cloned())
            .collect()
    }

    /// Get total stake
    pub fn total_stake(&self) -> U256 {
        *self.total_stake.read().unwrap()
    }

    /// Check if validator is active
    pub fn is_active(&self, index: u64) -> bool {
        self.validators.read().unwrap()
            .get(&index)
            .map(|v| v.status == ValidatorStatus::Active)
            .unwrap_or(false)
    }

    /// Shuffle validators for committee selection
    pub fn shuffle_validators(&self, seed: H256) -> Vec<u64> {
        let mut active = self.active_validators.read().unwrap().clone();
        
        // Fisher-Yates shuffle with seed
        let mut rng = StdRng::from_seed(seed.0);
        for i in (1..active.len()).rev() {
            let j = rng.gen_range(0..=i);
            active.swap(i, j);
        }
        
        *self.shuffling_seed.write().unwrap() = seed;
        active
    }

    /// Select validators for committees
    pub fn select_committee_validators(
        &self,
        committee_index: usize,
        committee_count: usize,
    ) -> Vec<SSFValidator> {
        let active = self.get_active_validators();
        let validators_per_committee = active.len() / committee_count;
        
        let start = committee_index * validators_per_committee;
        let end = ((committee_index + 1) * validators_per_committee).min(active.len());
        
        active[start..end].to_vec()
    }

    /// Update validator performance metrics
    pub fn update_performance(
        &self,
        index: u64,
        attestation_included: bool,
        block_proposed: Option<bool>,
    ) -> Result<()> {
        let mut validators = self.validators.write().unwrap();
        let validator = validators.get_mut(&index)
            .ok_or_else(|| SSFError::ValidatorError("Validator not found".into()))?;
        
        if attestation_included {
            validator.performance.attestations_included += 1;
        } else {
            validator.performance.attestations_missed += 1;
        }
        
        if let Some(proposed) = block_proposed {
            if proposed {
                validator.performance.blocks_proposed += 1;
            } else {
                validator.performance.blocks_missed += 1;
            }
        }
        
        // Recalculate rates
        let total_attestations = validator.performance.attestations_included + 
                                validator.performance.attestations_missed;
        if total_attestations > 0 {
            validator.performance.participation_rate = 
                validator.performance.attestations_included as f64 / total_attestations as f64;
        }
        
        let total_blocks = validator.performance.blocks_proposed + 
                          validator.performance.blocks_missed;
        if total_blocks > 0 {
            validator.performance.effectiveness = 
                validator.performance.blocks_proposed as f64 / total_blocks as f64;
        }
        
        Ok(())
    }

    /// Slash a validator
    pub fn slash_validator(&self, index: u64, epoch: u64) -> Result<()> {
        self.update_validator_status(index, ValidatorStatus::Slashed { slashed_at: epoch })?;
        
        // Reduce stake (typically by a percentage)
        let mut validators = self.validators.write().unwrap();
        if let Some(validator) = validators.get_mut(&index) {
            let penalty = validator.stake / 32; // 1/32 penalty
            validator.stake = validator.stake - penalty;
            
            let mut total = self.total_stake.write().unwrap();
            *total = *total - penalty;
            
            warn!("Validator {} slashed at epoch {} with penalty {}", index, epoch, penalty);
        }
        
        Ok(())
    }

    /// Process validator exit
    pub fn initiate_exit(&self, index: u64, exit_epoch: u64) -> Result<()> {
        self.update_validator_status(index, ValidatorStatus::Exiting { exit_epoch })?;
        info!("Validator {} initiated exit at epoch {}", index, exit_epoch);
        Ok(())
    }

    /// Process epoch transition
    pub fn process_epoch_transition(&self, new_epoch: u64) -> Result<()> {
        *self.epoch.write().unwrap() = new_epoch;
        
        // Process pending exits
        let mut to_exit = Vec::new();
        {
            let validators = self.validators.read().unwrap();
            for (&index, validator) in validators.iter() {
                if let ValidatorStatus::Exiting { exit_epoch } = validator.status {
                    if exit_epoch <= new_epoch {
                        to_exit.push(index);
                    }
                }
            }
        }
        
        for index in to_exit {
            self.update_validator_status(index, ValidatorStatus::Withdrawn)?;
            info!("Validator {} withdrawn at epoch {}", index, new_epoch);
        }
        
        Ok(())
    }

    /// Get validator set statistics
    pub fn get_statistics(&self) -> ValidatorSetStats {
        let validators = self.validators.read().unwrap();
        let active_count = self.active_validators.read().unwrap().len();
        
        let mut total_effectiveness = 0.0;
        let mut total_participation = 0.0;
        let mut slashed_count = 0;
        let mut exiting_count = 0;
        
        for validator in validators.values() {
            match validator.status {
                ValidatorStatus::Active => {
                    total_effectiveness += validator.performance.effectiveness;
                    total_participation += validator.performance.participation_rate;
                }
                ValidatorStatus::Slashed { .. } => slashed_count += 1,
                ValidatorStatus::Exiting { .. } => exiting_count += 1,
                _ => {}
            }
        }
        
        ValidatorSetStats {
            total_validators: validators.len(),
            active_validators: active_count,
            total_stake: *self.total_stake.read().unwrap(),
            average_effectiveness: if active_count > 0 { 
                total_effectiveness / active_count as f64 
            } else { 
                0.0 
            },
            average_participation: if active_count > 0 { 
                total_participation / active_count as f64 
            } else { 
                0.0 
            },
            slashed_validators: slashed_count,
            exiting_validators: exiting_count,
            current_epoch: *self.epoch.read().unwrap(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValidatorSetStats {
    pub total_validators: usize,
    pub active_validators: usize,
    pub total_stake: U256,
    pub average_effectiveness: f64,
    pub average_participation: f64,
    pub slashed_validators: usize,
    pub exiting_validators: usize,
    pub current_epoch: u64,
}

impl SSFValidator {
    pub fn new(
        index: u64,
        pubkey: PublicKey,
        address: Address,
        stake: U256,
    ) -> Self {
        Self {
            index,
            pubkey,
            address,
            stake,
            status: ValidatorStatus::Pending,
            performance: ValidatorPerformance::default(),
        }
    }

    pub fn index(&self) -> u64 {
        self.index
    }

    pub fn stake(&self) -> U256 {
        self.stake
    }

    pub async fn sign_block(&self, block_hash: H256) -> Result<Vec<u8>> {
        // In production, this would use secure key management
        // For now, return a dummy signature
        Ok(vec![0u8; 96])
    }
}

// Import for RNG
use rand::{SeedableRng, Rng};
use rand::rngs::StdRng;

/// Public key wrapper for BLS
#[derive(Debug, Clone)]
pub struct PublicKey {
    inner: Vec<u8>,
}

impl PublicKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            inner: bytes.to_vec(),
        })
    }
}