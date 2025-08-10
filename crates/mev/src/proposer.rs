use async_trait::async_trait;
use ethereum_types::{Address, H256, U256};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::{MevError, Result};

/// Proposer API for validators
#[async_trait]
pub trait ProposerApi: Send + Sync {
    async fn register_validator(&self, registration: ValidatorRegistration) -> Result<()>;
    async fn get_preferences(&self, pubkey: &[u8]) -> Result<ValidatorPreferences>;
    async fn update_preferences(&self, pubkey: &[u8], preferences: ValidatorPreferences) -> Result<()>;
}

/// Validator registration
#[derive(Debug, Clone)]
pub struct ValidatorRegistration {
    pub pubkey: Vec<u8>,
    pub fee_recipient: Address,
    pub gas_limit: u64,
    pub timestamp: u64,
    pub signature: Vec<u8>,
}

/// Validator preferences for MEV extraction
#[derive(Debug, Clone)]
pub struct ValidatorPreferences {
    pub min_bid: U256,
    pub preferred_relays: Vec<String>,
    pub censorship_resistance: bool,
    pub max_mev_share: u8, // Percentage (0-100)
    pub allow_reverts: bool,
}

impl Default for ValidatorPreferences {
    fn default() -> Self {
        Self {
            min_bid: U256::zero(),
            preferred_relays: Vec::new(),
            censorship_resistance: false,
            max_mev_share: 90, // 90% to proposer, 10% to builder
            allow_reverts: false,
        }
    }
}

/// Proposer registry
pub struct ProposerRegistry {
    validators: Arc<RwLock<HashMap<Vec<u8>, ValidatorInfo>>>,
    preferences: Arc<RwLock<HashMap<Vec<u8>, ValidatorPreferences>>>,
}

#[derive(Debug, Clone)]
struct ValidatorInfo {
    registration: ValidatorRegistration,
    last_slot: u64,
    total_blocks: u64,
    total_mev_earned: U256,
}

impl ProposerRegistry {
    pub fn new() -> Self {
        Self {
            validators: Arc::new(RwLock::new(HashMap::new())),
            preferences: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub fn register(&self, registration: ValidatorRegistration) -> Result<()> {
        let mut validators = self.validators.write().unwrap();
        
        let info = ValidatorInfo {
            registration: registration.clone(),
            last_slot: 0,
            total_blocks: 0,
            total_mev_earned: U256::zero(),
        };
        
        validators.insert(registration.pubkey.clone(), info);
        
        // Set default preferences if not exists
        let mut preferences = self.preferences.write().unwrap();
        preferences.entry(registration.pubkey)
            .or_insert_with(ValidatorPreferences::default);
        
        Ok(())
    }
    
    pub fn get_validator(&self, pubkey: &[u8]) -> Option<ValidatorRegistration> {
        let validators = self.validators.read().unwrap();
        validators.get(pubkey).map(|info| info.registration.clone())
    }
    
    pub fn get_preferences(&self, pubkey: &[u8]) -> ValidatorPreferences {
        let preferences = self.preferences.read().unwrap();
        preferences.get(pubkey)
            .cloned()
            .unwrap_or_default()
    }
    
    pub fn update_preferences(
        &self,
        pubkey: &[u8],
        new_preferences: ValidatorPreferences,
    ) -> Result<()> {
        let mut preferences = self.preferences.write().unwrap();
        preferences.insert(pubkey.to_vec(), new_preferences);
        Ok(())
    }
    
    pub fn record_block(&self, pubkey: &[u8], slot: u64, mev_earned: U256) {
        let mut validators = self.validators.write().unwrap();
        
        if let Some(info) = validators.get_mut(pubkey) {
            info.last_slot = slot;
            info.total_blocks += 1;
            info.total_mev_earned += mev_earned;
        }
    }
    
    pub fn get_statistics(&self, pubkey: &[u8]) -> Option<ProposerStatistics> {
        let validators = self.validators.read().unwrap();
        
        validators.get(pubkey).map(|info| ProposerStatistics {
            total_blocks: info.total_blocks,
            total_mev_earned: info.total_mev_earned,
            last_slot: info.last_slot,
            average_mev_per_block: if info.total_blocks > 0 {
                info.total_mev_earned / U256::from(info.total_blocks)
            } else {
                U256::zero()
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProposerStatistics {
    pub total_blocks: u64,
    pub total_mev_earned: U256,
    pub last_slot: u64,
    pub average_mev_per_block: U256,
}

/// Proposer service implementation
pub struct ProposerService {
    registry: Arc<ProposerRegistry>,
}

impl ProposerService {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ProposerRegistry::new()),
        }
    }
}

#[async_trait]
impl ProposerApi for ProposerService {
    async fn register_validator(&self, registration: ValidatorRegistration) -> Result<()> {
        self.registry.register(registration)
    }
    
    async fn get_preferences(&self, pubkey: &[u8]) -> Result<ValidatorPreferences> {
        Ok(self.registry.get_preferences(pubkey))
    }
    
    async fn update_preferences(
        &self,
        pubkey: &[u8],
        preferences: ValidatorPreferences,
    ) -> Result<()> {
        self.registry.update_preferences(pubkey, preferences)
    }
}