use ethereum_types::{H256, Address, U256};
use std::collections::HashMap;
use crate::Result;

/// Model marketplace for zkML models
pub struct ModelMarketplace {
    listings: HashMap<H256, ModelListing>,
}

impl ModelMarketplace {
    pub fn new() -> Self {
        Self {
            listings: HashMap::new(),
        }
    }
    
    pub fn list_model(&mut self, listing: ModelListing) -> Result<H256> {
        let id = listing.id;
        self.listings.insert(id, listing);
        Ok(id)
    }
    
    pub fn get_listing(&self, id: &H256) -> Option<&ModelListing> {
        self.listings.get(id)
    }
}

#[derive(Debug, Clone)]
pub struct ModelListing {
    pub id: H256,
    pub model_hash: H256,
    pub owner: Address,
    pub price: U256,
    pub description: String,
    pub category: ModelCategory,
    pub performance_metrics: PerformanceMetrics,
}

#[derive(Debug, Clone)]
pub enum ModelCategory {
    Classification,
    Regression,
    NLP,
    ComputerVision,
    ReinforcementLearning,
    GenerativeAI,
}

#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub accuracy: f64,
    pub inference_time_ms: u64,
    pub model_size_bytes: u64,
    pub proof_generation_time_ms: u64,
}