use ethereum_types::{Address, H256, U256};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use crate::{Bundle, MevError, Result};

/// MEV auction for selecting optimal bundle combinations
pub struct Auction {
    config: AuctionConfig,
}

#[derive(Debug, Clone)]
pub struct AuctionConfig {
    pub min_bid_increment: U256,
    pub max_bundles_per_block: usize,
    pub auction_duration: Duration,
    pub gas_limit: u64,
}

impl Default for AuctionConfig {
    fn default() -> Self {
        Self {
            min_bid_increment: U256::from(1_000_000_000), // 1 Gwei
            max_bundles_per_block: 5,
            auction_duration: Duration::from_millis(500),
            gas_limit: 30_000_000,
        }
    }
}

/// Bid in the auction
#[derive(Debug, Clone)]
pub struct Bid {
    pub bidder: Address,
    pub amount: U256,
    pub bundle: Bundle,
    pub timestamp: Instant,
}

impl Bid {
    pub fn new(bidder: Address, amount: U256, bundle: Bundle) -> Self {
        Self {
            bidder,
            amount,
            bundle,
            timestamp: Instant::now(),
        }
    }
    
    pub fn value_per_gas(&self) -> U256 {
        if self.bundle.total_gas() == 0 {
            U256::zero()
        } else {
            self.amount / U256::from(self.bundle.total_gas())
        }
    }
}

/// Auction result
#[derive(Debug, Clone)]
pub struct AuctionResult {
    pub winning_bundles: Vec<Bundle>,
    pub total_value: U256,
    pub gas_used: u64,
    pub excluded_bundles: Vec<(Bundle, ExclusionReason)>,
}

#[derive(Debug, Clone)]
pub enum ExclusionReason {
    Conflict,
    InsufficientValue,
    GasLimit,
    Invalid,
}

impl Auction {
    pub fn new() -> Self {
        Self {
            config: AuctionConfig::default(),
        }
    }
    
    pub fn with_config(config: AuctionConfig) -> Self {
        Self { config }
    }
    
    /// Run the auction to select optimal bundles
    pub fn run(&self, bundles: Vec<Bundle>, gas_limit: u64) -> Result<AuctionResult> {
        let start = Instant::now();
        let deadline = start + self.config.auction_duration;
        
        // Validate and score bundles
        let mut valid_bundles = Vec::new();
        let mut excluded = Vec::new();
        
        for bundle in bundles {
            match bundle.validate() {
                Ok(_) => valid_bundles.push(bundle),
                Err(_) => excluded.push((bundle, ExclusionReason::Invalid)),
            }
        }
        
        // Sort by value
        valid_bundles.sort_by(|a, b| {
            let base_fee = U256::from(30_000_000_000u64); // Estimate
            b.scoring_value(base_fee).cmp(&a.scoring_value(base_fee))
        });
        
        // Greedy selection with conflict detection
        let mut selected = Vec::new();
        let mut total_gas = 0u64;
        let mut total_value = U256::zero();
        let mut used_addresses = HashSet::new();
        
        for bundle in valid_bundles {
            if Instant::now() > deadline {
                break;
            }
            
            let bundle_gas = bundle.total_gas();
            
            // Check gas limit
            if total_gas + bundle_gas > gas_limit {
                excluded.push((bundle, ExclusionReason::GasLimit));
                continue;
            }
            
            // Check for conflicts
            if self.has_conflict(&bundle, &used_addresses) {
                excluded.push((bundle, ExclusionReason::Conflict));
                continue;
            }
            
            // Check minimum value
            let base_fee = U256::from(30_000_000_000u64);
            let bundle_value = bundle.scoring_value(base_fee);
            if bundle_value < self.config.min_bid_increment {
                excluded.push((bundle, ExclusionReason::InsufficientValue));
                continue;
            }
            
            // Add to selected
            self.update_used_addresses(&bundle, &mut used_addresses)?;
            selected.push(bundle);
            total_gas += bundle_gas;
            total_value += bundle_value;
            
            if selected.len() >= self.config.max_bundles_per_block {
                break;
            }
        }
        
        Ok(AuctionResult {
            winning_bundles: selected,
            total_value,
            gas_used: total_gas,
            excluded_bundles: excluded,
        })
    }
    
    fn has_conflict(&self, bundle: &Bundle, used_addresses: &HashSet<Address>) -> bool {
        for tx in &bundle.transactions {
            if let Ok(sender) = tx.sender() {
                if used_addresses.contains(&sender) {
                    return true;
                }
            }
        }
        false
    }
    
    fn update_used_addresses(
        &self,
        bundle: &Bundle,
        used_addresses: &mut HashSet<Address>,
    ) -> Result<()> {
        for tx in &bundle.transactions {
            let sender = tx.sender()?;
            used_addresses.insert(sender);
        }
        Ok(())
    }
}

/// Sealed bid auction for privacy
pub struct SealedBidAuction {
    config: AuctionConfig,
    sealed_bids: HashMap<H256, SealedBid>,
    revealed_bids: Vec<Bid>,
}

#[derive(Debug, Clone)]
struct SealedBid {
    commitment: H256,
    submitted_at: Instant,
}

impl SealedBidAuction {
    pub fn new(config: AuctionConfig) -> Self {
        Self {
            config,
            sealed_bids: HashMap::new(),
            revealed_bids: Vec::new(),
        }
    }
    
    /// Submit a sealed bid
    pub fn submit_sealed(&mut self, commitment: H256) -> Result<()> {
        if self.sealed_bids.len() >= 1000 {
            return Err(MevError::AuctionFailed("Too many sealed bids".to_string()));
        }
        
        self.sealed_bids.insert(
            commitment,
            SealedBid {
                commitment,
                submitted_at: Instant::now(),
            },
        );
        
        Ok(())
    }
    
    /// Reveal a sealed bid
    pub fn reveal(&mut self, bid: Bid, nonce: H256) -> Result<()> {
        let commitment = self.compute_commitment(&bid, nonce);
        
        if !self.sealed_bids.contains_key(&commitment) {
            return Err(MevError::AuctionFailed("Invalid commitment".to_string()));
        }
        
        self.sealed_bids.remove(&commitment);
        self.revealed_bids.push(bid);
        
        Ok(())
    }
    
    /// Finalize the auction
    pub fn finalize(&self) -> Result<AuctionResult> {
        let auction = Auction::with_config(self.config.clone());
        let bundles: Vec<Bundle> = self.revealed_bids
            .iter()
            .map(|bid| bid.bundle.clone())
            .collect();
        
        auction.run(bundles, self.config.gas_limit)
    }
    
    fn compute_commitment(&self, bid: &Bid, nonce: H256) -> H256 {
        use ethereum_crypto::keccak256;
        
        let mut data = Vec::new();
        data.extend_from_slice(bid.bidder.as_bytes());
        data.extend_from_slice(&bid.amount.to_be_bytes::<32>());
        data.extend_from_slice(&bid.bundle.hash().as_bytes());
        data.extend_from_slice(nonce.as_bytes());
        
        keccak256(&data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BundleTransaction;
    use ethereum_core::{Transaction, LegacyTransaction};
    
    fn create_test_bundle(nonce: u64) -> Bundle {
        let tx = Transaction::Legacy(LegacyTransaction {
            nonce: U256::from(nonce),
            gas_price: U256::from(50_000_000_000u64),
            gas_limit: U256::from(21_000),
            to: Some(Address::zero()),
            value: U256::zero(),
            data: ethereum_types::Bytes::new(),
            v: 27,
            r: U256::from(nonce),
            s: U256::from(nonce),
        });
        
        Bundle::new(vec![BundleTransaction::new(tx)], 100)
    }
    
    #[test]
    fn test_auction_basic() {
        let auction = Auction::new();
        let bundles = vec![
            create_test_bundle(1),
            create_test_bundle(2),
            create_test_bundle(3),
        ];
        
        let result = auction.run(bundles, 100_000).unwrap();
        assert!(!result.winning_bundles.is_empty());
        assert!(result.gas_used > 0);
    }
    
    #[test]
    fn test_auction_gas_limit() {
        let auction = Auction::new();
        let bundles = vec![
            create_test_bundle(1),
            create_test_bundle(2),
            create_test_bundle(3),
        ];
        
        let result = auction.run(bundles, 50_000).unwrap();
        assert!(result.gas_used <= 50_000);
    }
    
    #[test]
    fn test_sealed_bid_auction() {
        let mut auction = SealedBidAuction::new(AuctionConfig::default());
        
        let bid = Bid::new(
            Address::from([1u8; 20]),
            U256::from(1_000_000_000_000u64),
            create_test_bundle(1),
        );
        
        let nonce = H256::from([42u8; 32]);
        let commitment = auction.compute_commitment(&bid, nonce);
        
        auction.submit_sealed(commitment).unwrap();
        auction.reveal(bid, nonce).unwrap();
        
        let result = auction.finalize().unwrap();
        assert_eq!(result.winning_bundles.len(), 1);
    }
}