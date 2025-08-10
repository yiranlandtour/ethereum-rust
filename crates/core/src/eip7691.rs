use ethereum_types::{H256, U256, U64};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// EIP-7691: Blob throughput increase
/// Increases the blob gas target and maximum from 3/6 to 6/9 blobs per block

// Pre-EIP-7691 constants
pub const OLD_TARGET_BLOB_GAS_PER_BLOCK: u64 = 393216; // 3 blobs
pub const OLD_MAX_BLOB_GAS_PER_BLOCK: u64 = 786432;    // 6 blobs

// EIP-7691 constants
pub const TARGET_BLOB_GAS_PER_BLOCK: u64 = 786432;     // 6 blobs (3 * 131072 * 2)
pub const MAX_BLOB_GAS_PER_BLOCK: u64 = 1179648;       // 9 blobs (6 * 131072 * 1.5)
pub const BLOB_GAS_PER_BLOB: u64 = 131072;             // 2^17
pub const TARGET_BLOBS_PER_BLOCK: u64 = 6;
pub const MAX_BLOBS_PER_BLOCK: u64 = 9;

// Blob pricing constants
pub const MIN_BLOB_BASE_FEE: u64 = 1;
pub const BLOB_BASE_FEE_UPDATE_FRACTION: u64 = 3338477;

#[derive(Debug, Error)]
pub enum Eip7691Error {
    #[error("Too many blobs: {0} exceeds maximum {1}")]
    TooManyBlobs(usize, u64),
    
    #[error("Invalid blob gas: {0} is not a multiple of {1}")]
    InvalidBlobGas(u64, u64),
    
    #[error("Blob gas exceeds maximum: {0} > {1}")]
    ExcessBlobGas(u64, u64),
    
    #[error("Invalid blob versioned hash")]
    InvalidVersionedHash,
}

pub type Result<T> = std::result::Result<T, Eip7691Error>;

/// Configuration for blob gas parameters
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobGasConfig {
    pub target_blob_gas_per_block: u64,
    pub max_blob_gas_per_block: u64,
    pub blob_gas_per_blob: u64,
    pub min_blob_base_fee: u64,
    pub blob_base_fee_update_fraction: u64,
}

impl BlobGasConfig {
    /// Pre-EIP-7691 configuration
    pub fn pre_7691() -> Self {
        Self {
            target_blob_gas_per_block: OLD_TARGET_BLOB_GAS_PER_BLOCK,
            max_blob_gas_per_block: OLD_MAX_BLOB_GAS_PER_BLOCK,
            blob_gas_per_blob: BLOB_GAS_PER_BLOB,
            min_blob_base_fee: MIN_BLOB_BASE_FEE,
            blob_base_fee_update_fraction: BLOB_BASE_FEE_UPDATE_FRACTION,
        }
    }
    
    /// Post-EIP-7691 configuration
    pub fn post_7691() -> Self {
        Self {
            target_blob_gas_per_block: TARGET_BLOB_GAS_PER_BLOCK,
            max_blob_gas_per_block: MAX_BLOB_GAS_PER_BLOCK,
            blob_gas_per_blob: BLOB_GAS_PER_BLOB,
            min_blob_base_fee: MIN_BLOB_BASE_FEE,
            blob_base_fee_update_fraction: BLOB_BASE_FEE_UPDATE_FRACTION,
        }
    }
    
    pub fn target_blobs_per_block(&self) -> u64 {
        self.target_blob_gas_per_block / self.blob_gas_per_blob
    }
    
    pub fn max_blobs_per_block(&self) -> u64 {
        self.max_blob_gas_per_block / self.blob_gas_per_blob
    }
}

/// Calculate excess blob gas for the next block
pub fn calculate_excess_blob_gas(
    parent_excess_blob_gas: u64,
    parent_blob_gas_used: u64,
    config: &BlobGasConfig,
) -> u64 {
    let excess_blob_gas = parent_excess_blob_gas + parent_blob_gas_used;
    
    if excess_blob_gas < config.target_blob_gas_per_block {
        0
    } else {
        excess_blob_gas - config.target_blob_gas_per_block
    }
}

/// Calculate blob base fee from excess blob gas
pub fn calculate_blob_base_fee(excess_blob_gas: u64, config: &BlobGasConfig) -> U256 {
    fake_exponential(
        U256::from(config.min_blob_base_fee),
        U256::from(excess_blob_gas),
        U256::from(config.blob_base_fee_update_fraction),
    )
}

/// Fake exponential function for blob base fee calculation
fn fake_exponential(factor: U256, numerator: U256, denominator: U256) -> U256 {
    let mut output = U256::zero();
    let mut accum = factor * denominator;
    
    let mut i = U256::one();
    while accum > U256::zero() {
        output += accum;
        accum = (accum * numerator) / (denominator * i);
        i += U256::one();
        
        if i > U256::from(256) {
            break;
        }
    }
    
    output / denominator
}

/// Blob transaction data with EIP-7691 support
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobTransactionData {
    pub blob_versioned_hashes: Vec<H256>,
    pub max_fee_per_blob_gas: U256,
}

impl BlobTransactionData {
    pub fn new(blob_versioned_hashes: Vec<H256>, max_fee_per_blob_gas: U256) -> Result<Self> {
        let config = BlobGasConfig::post_7691();
        
        if blob_versioned_hashes.len() > config.max_blobs_per_block() as usize {
            return Err(Eip7691Error::TooManyBlobs(
                blob_versioned_hashes.len(),
                config.max_blobs_per_block(),
            ));
        }
        
        // Verify all hashes have correct version
        for hash in &blob_versioned_hashes {
            if hash.as_bytes()[0] != 0x01 {
                return Err(Eip7691Error::InvalidVersionedHash);
            }
        }
        
        Ok(Self {
            blob_versioned_hashes,
            max_fee_per_blob_gas,
        })
    }
    
    pub fn blob_count(&self) -> usize {
        self.blob_versioned_hashes.len()
    }
    
    pub fn blob_gas_used(&self) -> u64 {
        self.blob_count() as u64 * BLOB_GAS_PER_BLOB
    }
    
    pub fn validate_against_config(&self, config: &BlobGasConfig) -> Result<()> {
        if self.blob_count() > config.max_blobs_per_block() as usize {
            return Err(Eip7691Error::TooManyBlobs(
                self.blob_count(),
                config.max_blobs_per_block(),
            ));
        }
        
        let gas_used = self.blob_gas_used();
        if gas_used > config.max_blob_gas_per_block {
            return Err(Eip7691Error::ExcessBlobGas(
                gas_used,
                config.max_blob_gas_per_block,
            ));
        }
        
        Ok(())
    }
    
    pub fn calculate_cost(&self, blob_base_fee: U256) -> U256 {
        blob_base_fee * U256::from(self.blob_gas_used())
    }
    
    pub fn can_afford(&self, blob_base_fee: U256) -> bool {
        self.max_fee_per_blob_gas >= blob_base_fee
    }
}

/// Block header extensions for EIP-7691
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobGasInfo {
    pub blob_gas_used: U64,
    pub excess_blob_gas: U64,
}

impl BlobGasInfo {
    pub fn new(blob_gas_used: u64, excess_blob_gas: u64) -> Self {
        Self {
            blob_gas_used: U64::from(blob_gas_used),
            excess_blob_gas: U64::from(excess_blob_gas),
        }
    }
    
    pub fn validate(&self, config: &BlobGasConfig) -> Result<()> {
        let blob_gas_used = self.blob_gas_used.as_u64();
        
        // Check if blob gas is a multiple of BLOB_GAS_PER_BLOB
        if blob_gas_used % config.blob_gas_per_blob != 0 {
            return Err(Eip7691Error::InvalidBlobGas(
                blob_gas_used,
                config.blob_gas_per_blob,
            ));
        }
        
        // Check if blob gas doesn't exceed maximum
        if blob_gas_used > config.max_blob_gas_per_block {
            return Err(Eip7691Error::ExcessBlobGas(
                blob_gas_used,
                config.max_blob_gas_per_block,
            ));
        }
        
        Ok(())
    }
    
    pub fn blob_base_fee(&self, config: &BlobGasConfig) -> U256 {
        calculate_blob_base_fee(self.excess_blob_gas.as_u64(), config)
    }
}

/// Blob pool for managing pending blob transactions
pub struct BlobPool {
    transactions: Vec<(H256, BlobTransactionData, U256)>, // (tx_hash, blob_data, priority_fee)
    config: BlobGasConfig,
    max_pool_size: usize,
}

impl BlobPool {
    pub fn new(config: BlobGasConfig, max_pool_size: usize) -> Self {
        Self {
            transactions: Vec::new(),
            config,
            max_pool_size,
        }
    }
    
    pub fn add_transaction(
        &mut self,
        tx_hash: H256,
        blob_data: BlobTransactionData,
        priority_fee: U256,
    ) -> Result<()> {
        blob_data.validate_against_config(&self.config)?;
        
        if self.transactions.len() >= self.max_pool_size {
            // Remove lowest priority transaction
            self.evict_lowest_priority();
        }
        
        self.transactions.push((tx_hash, blob_data, priority_fee));
        self.sort_by_priority();
        
        Ok(())
    }
    
    pub fn get_best_transactions(
        &self,
        blob_base_fee: U256,
        max_blobs: u64,
    ) -> Vec<(H256, BlobTransactionData)> {
        let mut selected = Vec::new();
        let mut total_blobs = 0u64;
        
        for (tx_hash, blob_data, _) in &self.transactions {
            if !blob_data.can_afford(blob_base_fee) {
                continue;
            }
            
            let blob_count = blob_data.blob_count() as u64;
            if total_blobs + blob_count > max_blobs {
                continue;
            }
            
            selected.push((*tx_hash, blob_data.clone()));
            total_blobs += blob_count;
        }
        
        selected
    }
    
    pub fn remove_transaction(&mut self, tx_hash: &H256) -> Option<BlobTransactionData> {
        if let Some(pos) = self.transactions.iter().position(|(h, _, _)| h == tx_hash) {
            let (_, blob_data, _) = self.transactions.remove(pos);
            Some(blob_data)
        } else {
            None
        }
    }
    
    fn sort_by_priority(&mut self) {
        self.transactions.sort_by(|a, b| b.2.cmp(&a.2));
    }
    
    fn evict_lowest_priority(&mut self) {
        if !self.transactions.is_empty() {
            self.transactions.pop();
        }
    }
    
    pub fn size(&self) -> usize {
        self.transactions.len()
    }
    
    pub fn total_blobs(&self) -> usize {
        self.transactions
            .iter()
            .map(|(_, blob_data, _)| blob_data.blob_count())
            .sum()
    }
}

/// Migrate blob gas parameters during fork transition
pub fn migrate_blob_gas_at_fork(
    pre_fork_excess: u64,
    pre_fork_used: u64,
) -> (u64, u64) {
    let old_config = BlobGasConfig::pre_7691();
    let new_config = BlobGasConfig::post_7691();
    
    // Calculate what the excess would be under new parameters
    let scaled_excess = pre_fork_excess * new_config.target_blob_gas_per_block 
        / old_config.target_blob_gas_per_block;
    
    let scaled_used = pre_fork_used * new_config.blob_gas_per_blob 
        / old_config.blob_gas_per_blob;
    
    (scaled_excess, scaled_used)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_blob_gas_config() {
        let old_config = BlobGasConfig::pre_7691();
        assert_eq!(old_config.target_blobs_per_block(), 3);
        assert_eq!(old_config.max_blobs_per_block(), 6);
        
        let new_config = BlobGasConfig::post_7691();
        assert_eq!(new_config.target_blobs_per_block(), 6);
        assert_eq!(new_config.max_blobs_per_block(), 9);
    }
    
    #[test]
    fn test_excess_blob_gas_calculation() {
        let config = BlobGasConfig::post_7691();
        
        // No excess when under target
        let excess = calculate_excess_blob_gas(0, 393216, &config); // 3 blobs used
        assert_eq!(excess, 0);
        
        // Excess when over target
        let excess = calculate_excess_blob_gas(0, 1179648, &config); // 9 blobs used
        assert_eq!(excess, 393216); // 3 blobs worth of excess
    }
    
    #[test]
    fn test_blob_transaction_validation() {
        let hashes = vec![
            H256::from([0x01; 32]),
            H256::from([0x01; 32]),
            H256::from([0x01; 32]),
        ];
        
        let blob_tx = BlobTransactionData::new(
            hashes,
            U256::from(1_000_000_000u64),
        ).unwrap();
        
        assert_eq!(blob_tx.blob_count(), 3);
        assert_eq!(blob_tx.blob_gas_used(), 3 * BLOB_GAS_PER_BLOB);
    }
    
    #[test]
    fn test_blob_pool() {
        let config = BlobGasConfig::post_7691();
        let mut pool = BlobPool::new(config, 100);
        
        let blob_data = BlobTransactionData::new(
            vec![H256::from([0x01; 32])],
            U256::from(1_000_000_000u64),
        ).unwrap();
        
        pool.add_transaction(
            H256::from([1u8; 32]),
            blob_data,
            U256::from(10_000_000_000u64),
        ).unwrap();
        
        assert_eq!(pool.size(), 1);
        assert_eq!(pool.total_blobs(), 1);
    }
    
    #[test]
    fn test_migration() {
        let (new_excess, new_used) = migrate_blob_gas_at_fork(393216, 393216);
        assert_eq!(new_excess, 786432); // Doubled
        assert_eq!(new_used, 393216); // Same actual blob count
    }
}