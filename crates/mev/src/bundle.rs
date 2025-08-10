use ethereum_types::{Address, H256, U256};
use ethereum_core::Transaction;
use ethereum_crypto::keccak256;
use serde::{Deserialize, Serialize};

use crate::{MevError, Result};

/// MEV bundle containing a sequence of transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub transactions: Vec<BundleTransaction>,
    pub block_number: u64,
    pub min_block: u64,
    pub max_block: u64,
    pub reverting_tx_hashes: Vec<H256>,
    pub replacement_uuid: Option<String>,
}

impl Bundle {
    pub fn new(transactions: Vec<BundleTransaction>, block_number: u64) -> Self {
        Self {
            transactions,
            block_number,
            min_block: block_number,
            max_block: block_number + 1,
            reverting_tx_hashes: Vec::new(),
            replacement_uuid: None,
        }
    }
    
    pub fn hash(&self) -> H256 {
        let mut data = Vec::new();
        for tx in &self.transactions {
            data.extend_from_slice(tx.hash().as_bytes());
        }
        keccak256(&data)
    }
    
    pub fn total_gas(&self) -> u64 {
        self.transactions
            .iter()
            .map(|tx| tx.gas_limit())
            .sum()
    }
    
    pub fn scoring_value(&self, base_fee: U256) -> U256 {
        let mut value = U256::zero();
        for tx in &self.transactions {
            value += tx.effective_tip(base_fee) * U256::from(tx.gas_limit());
        }
        value
    }
    
    pub fn validate(&self) -> Result<()> {
        if self.transactions.is_empty() {
            return Err(MevError::InvalidBundle("Bundle has no transactions".to_string()));
        }
        
        if self.max_block < self.min_block {
            return Err(MevError::InvalidBundle("Invalid block range".to_string()));
        }
        
        // Check for duplicate transactions
        let mut seen = std::collections::HashSet::new();
        for tx in &self.transactions {
            let hash = tx.hash();
            if !seen.insert(hash) {
                return Err(MevError::InvalidBundle(format!(
                    "Duplicate transaction: {:?}",
                    hash
                )));
            }
        }
        
        Ok(())
    }
    
    pub fn can_revert(&self, tx_hash: &H256) -> bool {
        self.reverting_tx_hashes.contains(tx_hash)
    }
}

/// Transaction within a bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleTransaction {
    pub transaction: Transaction,
    pub signer: Option<Address>,
    pub can_revert: bool,
}

impl BundleTransaction {
    pub fn new(transaction: Transaction) -> Self {
        Self {
            transaction,
            signer: None,
            can_revert: false,
        }
    }
    
    pub fn with_signer(mut self, signer: Address) -> Self {
        self.signer = Some(signer);
        self
    }
    
    pub fn allow_revert(mut self) -> Self {
        self.can_revert = true;
        self
    }
    
    pub fn hash(&self) -> H256 {
        self.transaction.hash()
    }
    
    pub fn gas_limit(&self) -> u64 {
        self.transaction.gas_limit().as_u64()
    }
    
    pub fn effective_tip(&self, base_fee: U256) -> U256 {
        let max_fee = self.transaction.gas_price();
        if max_fee > base_fee {
            max_fee - base_fee
        } else {
            U256::zero()
        }
    }
    
    pub fn sender(&self) -> Result<Address> {
        if let Some(signer) = self.signer {
            Ok(signer)
        } else {
            self.transaction.sender()
                .map_err(|e| MevError::InvalidBundle(format!("Failed to recover sender: {:?}", e)))
        }
    }
}

/// Bundle pool for managing pending bundles
#[derive(Debug, Clone)]
pub struct BundlePool {
    bundles: Vec<Bundle>,
    max_bundles: usize,
}

impl BundlePool {
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }
    
    pub fn with_capacity(max_bundles: usize) -> Self {
        Self {
            bundles: Vec::new(),
            max_bundles,
        }
    }
    
    pub fn add(&mut self, bundle: Bundle) -> Result<()> {
        bundle.validate()?;
        
        if self.bundles.len() >= self.max_bundles {
            // Remove lowest value bundle
            self.evict_lowest_value(U256::from(1_000_000_000)); // 1 Gwei base fee assumption
        }
        
        self.bundles.push(bundle);
        Ok(())
    }
    
    pub fn get_bundles_for_block(&self, block_number: u64) -> Vec<&Bundle> {
        self.bundles
            .iter()
            .filter(|b| b.min_block <= block_number && block_number <= b.max_block)
            .collect()
    }
    
    pub fn remove_expired(&mut self, current_block: u64) {
        self.bundles.retain(|b| b.max_block >= current_block);
    }
    
    pub fn sort_by_value(&mut self, base_fee: U256) {
        self.bundles.sort_by(|a, b| {
            b.scoring_value(base_fee).cmp(&a.scoring_value(base_fee))
        });
    }
    
    fn evict_lowest_value(&mut self, base_fee: U256) {
        if self.bundles.is_empty() {
            return;
        }
        
        let mut min_value = U256::max_value();
        let mut min_index = 0;
        
        for (i, bundle) in self.bundles.iter().enumerate() {
            let value = bundle.scoring_value(base_fee);
            if value < min_value {
                min_value = value;
                min_index = i;
            }
        }
        
        self.bundles.remove(min_index);
    }
    
    pub fn len(&self) -> usize {
        self.bundles.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.bundles.is_empty()
    }
    
    pub fn clear(&mut self) {
        self.bundles.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_core::LegacyTransaction;
    
    #[test]
    fn test_bundle_creation() {
        let tx = Transaction::Legacy(LegacyTransaction {
            nonce: U256::zero(),
            gas_price: U256::from(20_000_000_000u64),
            gas_limit: U256::from(21_000),
            to: Some(Address::zero()),
            value: U256::from(1_000_000_000_000_000_000u64),
            data: ethereum_types::Bytes::new(),
            v: 27,
            r: U256::zero(),
            s: U256::zero(),
        });
        
        let bundle_tx = BundleTransaction::new(tx);
        let bundle = Bundle::new(vec![bundle_tx], 100);
        
        assert_eq!(bundle.block_number, 100);
        assert_eq!(bundle.min_block, 100);
        assert_eq!(bundle.max_block, 101);
    }
    
    #[test]
    fn test_bundle_validation() {
        let bundle = Bundle::new(vec![], 100);
        assert!(bundle.validate().is_err());
        
        let tx = Transaction::Legacy(LegacyTransaction {
            nonce: U256::zero(),
            gas_price: U256::from(20_000_000_000u64),
            gas_limit: U256::from(21_000),
            to: Some(Address::zero()),
            value: U256::zero(),
            data: ethereum_types::Bytes::new(),
            v: 27,
            r: U256::zero(),
            s: U256::zero(),
        });
        
        let bundle_tx = BundleTransaction::new(tx);
        let mut bundle = Bundle::new(vec![bundle_tx], 100);
        bundle.max_block = 99; // Invalid range
        assert!(bundle.validate().is_err());
    }
    
    #[test]
    fn test_bundle_pool() {
        let mut pool = BundlePool::with_capacity(10);
        
        for i in 0..5 {
            let tx = Transaction::Legacy(LegacyTransaction {
                nonce: U256::from(i),
                gas_price: U256::from(20_000_000_000u64),
                gas_limit: U256::from(21_000),
                to: Some(Address::zero()),
                value: U256::zero(),
                data: ethereum_types::Bytes::new(),
                v: 27,
                r: U256::from(i),
                s: U256::from(i),
            });
            
            let bundle_tx = BundleTransaction::new(tx);
            let bundle = Bundle::new(vec![bundle_tx], 100 + i as u64);
            pool.add(bundle).unwrap();
        }
        
        assert_eq!(pool.len(), 5);
        
        let bundles = pool.get_bundles_for_block(102);
        assert!(bundles.len() > 0);
        
        pool.remove_expired(105);
        assert!(pool.len() < 5);
    }
}