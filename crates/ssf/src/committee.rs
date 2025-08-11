use ethereum_types::{H256, U256};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use tracing::{info, debug};

use crate::{Result, SSFError};
use crate::validator::{SSFValidator, ValidatorSet};

/// Committee for Single Slot Finality
#[derive(Debug, Clone)]
pub struct Committee {
    index: usize,
    validators: Vec<ValidatorInfo>,
    epoch: u64,
    total_stake: U256,
    selection_strategy: CommitteeSelection,
}

#[derive(Debug, Clone)]
pub struct ValidatorInfo {
    pub index: u64,
    pub pubkey: Vec<u8>,
    pub stake: U256,
    pub effectiveness: f64,
}

#[derive(Debug, Clone)]
pub enum CommitteeSelection {
    /// Random selection with uniform distribution
    Random { seed: H256 },
    /// Stake-weighted selection
    StakeWeighted { min_stake: U256 },
    /// Performance-based selection
    PerformanceBased { min_effectiveness: f64 },
    /// Hybrid selection combining multiple factors
    Hybrid {
        stake_weight: f64,
        performance_weight: f64,
        randomness_weight: f64,
    },
}

impl Default for CommitteeSelection {
    fn default() -> Self {
        Self::Hybrid {
            stake_weight: 0.4,
            performance_weight: 0.3,
            randomness_weight: 0.3,
        }
    }
}

/// Committee manager for SSF
pub struct CommitteeManager {
    committees: Arc<RwLock<Vec<Committee>>>,
    validator_set: Arc<ValidatorSet>,
    config: CommitteeConfig,
    committee_assignments: Arc<RwLock<HashMap<u64, usize>>>, // validator_index -> committee_index
}

#[derive(Debug, Clone)]
pub struct CommitteeConfig {
    pub target_committee_size: usize,
    pub max_committees: usize,
    pub shuffle_period: u64, // epochs between reshuffling
    pub selection_strategy: CommitteeSelection,
}

impl Default for CommitteeConfig {
    fn default() -> Self {
        Self {
            target_committee_size: 128,
            max_committees: 64,
            shuffle_period: 256,
            selection_strategy: CommitteeSelection::default(),
        }
    }
}

impl Committee {
    pub fn new(index: usize, validators: Vec<ValidatorInfo>) -> Self {
        let total_stake = validators.iter().map(|v| v.stake).sum();
        
        Self {
            index,
            validators,
            epoch: 0,
            total_stake,
            selection_strategy: CommitteeSelection::default(),
        }
    }

    pub fn validators(&self) -> &[ValidatorInfo] {
        &self.validators
    }

    pub fn size(&self) -> usize {
        self.validators.len()
    }

    pub fn total_stake(&self) -> U256 {
        self.total_stake
    }

    pub fn get_validator(&self, index: u64) -> Option<&ValidatorInfo> {
        self.validators.iter().find(|v| v.index == index)
    }

    /// Calculate committee voting power
    pub fn voting_power(&self) -> f64 {
        let effectiveness_sum: f64 = self.validators.iter()
            .map(|v| v.effectiveness)
            .sum();
        
        let avg_effectiveness = effectiveness_sum / self.validators.len() as f64;
        
        // Combine stake and effectiveness for voting power
        let stake_factor = self.total_stake.as_u128() as f64 / 1e18; // Normalize to ETH
        stake_factor * avg_effectiveness
    }

    /// Check if committee has quorum
    pub fn has_quorum(&self, participating_validators: &[u64]) -> bool {
        let participating_stake: U256 = self.validators
            .iter()
            .filter(|v| participating_validators.contains(&v.index))
            .map(|v| v.stake)
            .sum();
        
        // 2/3 quorum
        participating_stake * 3 >= self.total_stake * 2
    }

    /// Get committee statistics
    pub fn get_stats(&self) -> CommitteeStats {
        let avg_stake = self.total_stake / self.validators.len();
        let avg_effectiveness = self.validators.iter()
            .map(|v| v.effectiveness)
            .sum::<f64>() / self.validators.len() as f64;
        
        let min_stake = self.validators.iter()
            .map(|v| v.stake)
            .min()
            .unwrap_or(U256::zero());
        
        let max_stake = self.validators.iter()
            .map(|v| v.stake)
            .max()
            .unwrap_or(U256::zero());
        
        CommitteeStats {
            index: self.index,
            size: self.validators.len(),
            total_stake: self.total_stake,
            average_stake: avg_stake,
            min_stake,
            max_stake,
            average_effectiveness: avg_effectiveness,
            voting_power: self.voting_power(),
        }
    }
}

impl CommitteeManager {
    pub fn new(validator_set: Arc<ValidatorSet>, config: CommitteeConfig) -> Self {
        Self {
            committees: Arc::new(RwLock::new(Vec::new())),
            validator_set,
            config,
            committee_assignments: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Form committees for an epoch
    pub fn form_committees(&self, epoch: u64, seed: H256) -> Result<Vec<Committee>> {
        let active_validators = self.validator_set.get_active_validators();
        
        if active_validators.is_empty() {
            return Err(SSFError::CommitteeError("No active validators".into()));
        }
        
        let committees = match self.config.selection_strategy {
            CommitteeSelection::Random { .. } => {
                self.form_random_committees(active_validators, seed)?
            }
            CommitteeSelection::StakeWeighted { min_stake } => {
                self.form_stake_weighted_committees(active_validators, min_stake)?
            }
            CommitteeSelection::PerformanceBased { min_effectiveness } => {
                self.form_performance_committees(active_validators, min_effectiveness)?
            }
            CommitteeSelection::Hybrid { stake_weight, performance_weight, randomness_weight } => {
                self.form_hybrid_committees(
                    active_validators,
                    stake_weight,
                    performance_weight,
                    randomness_weight,
                    seed,
                )?
            }
        };
        
        // Update committee assignments
        let mut assignments = self.committee_assignments.write().unwrap();
        assignments.clear();
        
        for (committee_index, committee) in committees.iter().enumerate() {
            for validator in &committee.validators {
                assignments.insert(validator.index, committee_index);
            }
        }
        
        // Store committees
        *self.committees.write().unwrap() = committees.clone();
        
        info!("Formed {} committees for epoch {}", committees.len(), epoch);
        
        Ok(committees)
    }

    /// Form committees with random selection
    fn form_random_committees(
        &self,
        mut validators: Vec<SSFValidator>,
        seed: H256,
    ) -> Result<Vec<Committee>> {
        use rand::{SeedableRng, seq::SliceRandom};
        use rand::rngs::StdRng;
        
        let mut rng = StdRng::from_seed(seed.0);
        validators.shuffle(&mut rng);
        
        let committee_count = (validators.len() / self.config.target_committee_size)
            .max(1)
            .min(self.config.max_committees);
        
        let validators_per_committee = validators.len() / committee_count;
        let mut committees = Vec::new();
        
        for i in 0..committee_count {
            let start = i * validators_per_committee;
            let end = if i == committee_count - 1 {
                validators.len()
            } else {
                (i + 1) * validators_per_committee
            };
            
            let committee_validators: Vec<ValidatorInfo> = validators[start..end]
                .iter()
                .map(|v| ValidatorInfo {
                    index: v.index(),
                    pubkey: vec![], // Simplified
                    stake: v.stake(),
                    effectiveness: 1.0, // Default
                })
                .collect();
            
            committees.push(Committee::new(i, committee_validators));
        }
        
        Ok(committees)
    }

    /// Form committees weighted by stake
    fn form_stake_weighted_committees(
        &self,
        validators: Vec<SSFValidator>,
        min_stake: U256,
    ) -> Result<Vec<Committee>> {
        // Filter validators by minimum stake
        let mut eligible: Vec<_> = validators
            .into_iter()
            .filter(|v| v.stake() >= min_stake)
            .collect();
        
        // Sort by stake descending
        eligible.sort_by_key(|v| std::cmp::Reverse(v.stake()));
        
        let committee_count = (eligible.len() / self.config.target_committee_size)
            .max(1)
            .min(self.config.max_committees);
        
        let mut committees = Vec::new();
        let mut current_committee = Vec::new();
        let mut committee_stake = U256::zero();
        let target_stake_per_committee = self.validator_set.total_stake() / committee_count;
        
        for validator in eligible {
            current_committee.push(ValidatorInfo {
                index: validator.index(),
                pubkey: vec![],
                stake: validator.stake(),
                effectiveness: 1.0,
            });
            committee_stake += validator.stake();
            
            if committee_stake >= target_stake_per_committee || 
               current_committee.len() >= self.config.target_committee_size {
                committees.push(Committee::new(committees.len(), current_committee));
                current_committee = Vec::new();
                committee_stake = U256::zero();
            }
        }
        
        // Add remaining validators
        if !current_committee.is_empty() {
            committees.push(Committee::new(committees.len(), current_committee));
        }
        
        Ok(committees)
    }

    /// Form committees based on validator performance
    fn form_performance_committees(
        &self,
        validators: Vec<SSFValidator>,
        min_effectiveness: f64,
    ) -> Result<Vec<Committee>> {
        // In production, would use actual performance metrics
        // For now, use random effectiveness
        let mut validator_infos: Vec<_> = validators
            .into_iter()
            .map(|v| {
                let effectiveness = 0.5 + (v.index() as f64 % 50.0) / 100.0; // Mock effectiveness
                (v, effectiveness)
            })
            .filter(|(_, eff)| *eff >= min_effectiveness)
            .collect();
        
        // Sort by effectiveness descending
        validator_infos.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let committee_count = (validator_infos.len() / self.config.target_committee_size)
            .max(1)
            .min(self.config.max_committees);
        
        let validators_per_committee = validator_infos.len() / committee_count;
        let mut committees = Vec::new();
        
        for i in 0..committee_count {
            let start = i * validators_per_committee;
            let end = if i == committee_count - 1 {
                validator_infos.len()
            } else {
                (i + 1) * validators_per_committee
            };
            
            let committee_validators: Vec<ValidatorInfo> = validator_infos[start..end]
                .iter()
                .map(|(v, eff)| ValidatorInfo {
                    index: v.index(),
                    pubkey: vec![],
                    stake: v.stake(),
                    effectiveness: *eff,
                })
                .collect();
            
            committees.push(Committee::new(i, committee_validators));
        }
        
        Ok(committees)
    }

    /// Form committees using hybrid selection
    fn form_hybrid_committees(
        &self,
        validators: Vec<SSFValidator>,
        stake_weight: f64,
        performance_weight: f64,
        randomness_weight: f64,
        seed: H256,
    ) -> Result<Vec<Committee>> {
        use rand::{SeedableRng, Rng};
        use rand::rngs::StdRng;
        
        let mut rng = StdRng::from_seed(seed.0);
        
        // Calculate scores for each validator
        let mut scored_validators: Vec<_> = validators
            .into_iter()
            .map(|v| {
                let stake_score = (v.stake().as_u128() as f64 / 1e18).min(100.0) / 100.0;
                let performance_score = 0.5 + (v.index() as f64 % 50.0) / 100.0; // Mock
                let random_score = rng.gen::<f64>();
                
                let total_score = stake_score * stake_weight +
                                 performance_score * performance_weight +
                                 random_score * randomness_weight;
                
                (v, total_score, performance_score)
            })
            .collect();
        
        // Sort by score descending
        scored_validators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        let committee_count = (scored_validators.len() / self.config.target_committee_size)
            .max(1)
            .min(self.config.max_committees);
        
        // Distribute validators to committees in round-robin fashion for balance
        let mut committees: Vec<Vec<ValidatorInfo>> = (0..committee_count)
            .map(|_| Vec::new())
            .collect();
        
        for (i, (validator, _, effectiveness)) in scored_validators.into_iter().enumerate() {
            let committee_index = i % committee_count;
            committees[committee_index].push(ValidatorInfo {
                index: validator.index(),
                pubkey: vec![],
                stake: validator.stake(),
                effectiveness,
            });
        }
        
        Ok(committees
            .into_iter()
            .enumerate()
            .map(|(i, validators)| Committee::new(i, validators))
            .collect())
    }

    /// Get committee for a validator
    pub fn get_validator_committee(&self, validator_index: u64) -> Option<usize> {
        self.committee_assignments.read().unwrap().get(&validator_index).cloned()
    }

    /// Get all committees
    pub fn get_committees(&self) -> Vec<Committee> {
        self.committees.read().unwrap().clone()
    }

    /// Get specific committee
    pub fn get_committee(&self, index: usize) -> Option<Committee> {
        self.committees.read().unwrap().get(index).cloned()
    }

    /// Check if committees need reshuffling
    pub fn needs_reshuffle(&self, current_epoch: u64, last_shuffle_epoch: u64) -> bool {
        current_epoch - last_shuffle_epoch >= self.config.shuffle_period
    }
}

#[derive(Debug, Clone)]
pub struct CommitteeStats {
    pub index: usize,
    pub size: usize,
    pub total_stake: U256,
    pub average_stake: U256,
    pub min_stake: U256,
    pub max_stake: U256,
    pub average_effectiveness: f64,
    pub voting_power: f64,
}