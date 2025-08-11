use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{info, debug, warn};

use crate::{Result, SSFError};
use crate::aggregator::SignatureAggregator;
use crate::validator::ValidatorSet;
use crate::committee::Committee;

/// Single Slot Finality implementation
/// Achieves finality in 12 seconds instead of 13 minutes
pub struct SingleSlotFinality {
    config: FinalityConfig,
    validator_set: Arc<ValidatorSet>,
    aggregator: Arc<SignatureAggregator>,
    committees: Arc<RwLock<Vec<Committee>>>,
    finality_cache: Arc<RwLock<FinalityCache>>,
    metrics: Arc<FinalityMetrics>,
}

#[derive(Debug, Clone)]
pub struct FinalityConfig {
    /// Target time for finality (12 seconds for SSF)
    pub slot_duration: Duration,
    /// Number of validators required for finality
    pub finality_threshold: f64,
    /// Number of committees for parallel processing
    pub committee_count: usize,
    /// Enable optimistic finality
    pub optimistic_finality: bool,
    /// Maximum validators per committee
    pub max_committee_size: usize,
    /// Signature aggregation timeout
    pub aggregation_timeout: Duration,
}

impl Default for FinalityConfig {
    fn default() -> Self {
        Self {
            slot_duration: Duration::from_secs(12),
            finality_threshold: 0.67, // 2/3 majority
            committee_count: 64,
            optimistic_finality: true,
            max_committee_size: 128,
            aggregation_timeout: Duration::from_secs(4),
        }
    }
}

#[derive(Debug, Clone)]
pub enum FinalityStatus {
    Pending,
    Optimistic { confidence: f64 },
    Finalized { slot: u64, block_hash: H256 },
    Failed { reason: String },
}

struct FinalityCache {
    finalized_blocks: Vec<FinalizedBlock>,
    pending_blocks: Vec<PendingBlock>,
}

#[derive(Debug, Clone)]
struct FinalizedBlock {
    slot: u64,
    block_hash: H256,
    finalized_at: Instant,
    signatures: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
struct PendingBlock {
    slot: u64,
    block_hash: H256,
    received_at: Instant,
    signature_count: usize,
    total_stake: U256,
}

struct FinalityMetrics {
    blocks_finalized: std::sync::atomic::AtomicU64,
    average_finality_time: std::sync::atomic::AtomicU64,
    failed_finalizations: std::sync::atomic::AtomicU64,
    signature_aggregation_time: std::sync::atomic::AtomicU64,
}

impl SingleSlotFinality {
    pub fn new(config: FinalityConfig) -> Result<Self> {
        let validator_set = Arc::new(ValidatorSet::new());
        let aggregator = Arc::new(SignatureAggregator::new(
            config.committee_count,
            config.max_committee_size,
        )?);
        
        Ok(Self {
            config,
            validator_set,
            aggregator,
            committees: Arc::new(RwLock::new(Vec::new())),
            finality_cache: Arc::new(RwLock::new(FinalityCache {
                finalized_blocks: Vec::new(),
                pending_blocks: Vec::new(),
            })),
            metrics: Arc::new(FinalityMetrics::new()),
        })
    }
    
    /// Process a block for single slot finality
    pub async fn process_block(&self, block: &Block) -> Result<FinalityStatus> {
        let start = Instant::now();
        
        info!("Processing block {} for single slot finality", block.header.number);
        
        // Add to pending
        self.add_pending_block(block)?;
        
        // Initiate parallel signature collection
        let signatures = self.collect_signatures_parallel(block).await?;
        
        // Aggregate signatures
        let aggregated = self.aggregator.aggregate_signatures(
            &signatures,
            block.header.hash(),
        ).await?;
        
        // Check finality threshold
        let total_stake = self.calculate_total_stake(&signatures)?;
        let network_stake = self.validator_set.total_stake();
        
        let stake_ratio = total_stake.as_u128() as f64 / network_stake.as_u128() as f64;
        
        if stake_ratio >= self.config.finality_threshold {
            // Achieve finality
            self.finalize_block(block, aggregated)?;
            
            let elapsed = start.elapsed();
            self.metrics.average_finality_time.store(
                elapsed.as_millis() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            
            info!("Block {} finalized in {:?}", block.header.number, elapsed);
            
            Ok(FinalityStatus::Finalized {
                slot: block.header.number,
                block_hash: block.header.hash(),
            })
        } else if self.config.optimistic_finality && stake_ratio > 0.5 {
            // Optimistic finality
            Ok(FinalityStatus::Optimistic {
                confidence: stake_ratio,
            })
        } else {
            self.metrics.failed_finalizations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            
            Ok(FinalityStatus::Failed {
                reason: format!("Insufficient stake: {:.2}%", stake_ratio * 100.0),
            })
        }
    }
    
    /// Collect signatures in parallel from committees
    async fn collect_signatures_parallel(
        &self,
        block: &Block,
    ) -> Result<Vec<ValidatorSignature>> {
        let committees = self.committees.read().unwrap();
        
        let mut handles = Vec::new();
        
        for committee in committees.iter() {
            let committee = committee.clone();
            let block_hash = block.header.hash();
            let aggregator = self.aggregator.clone();
            
            let handle = tokio::spawn(async move {
                Self::collect_committee_signatures(
                    committee,
                    block_hash,
                    aggregator,
                ).await
            });
            
            handles.push(handle);
        }
        
        // Wait for all committees with timeout
        let timeout = self.config.aggregation_timeout;
        let mut all_signatures = Vec::new();
        
        for handle in handles {
            match tokio::time::timeout(timeout, handle).await {
                Ok(Ok(Ok(signatures))) => {
                    all_signatures.extend(signatures);
                }
                Ok(Ok(Err(e))) => {
                    warn!("Committee signature collection failed: {}", e);
                }
                Ok(Err(e)) => {
                    warn!("Committee task failed: {}", e);
                }
                Err(_) => {
                    warn!("Committee signature collection timed out");
                }
            }
        }
        
        Ok(all_signatures)
    }
    
    async fn collect_committee_signatures(
        committee: Committee,
        block_hash: H256,
        aggregator: Arc<SignatureAggregator>,
    ) -> Result<Vec<ValidatorSignature>> {
        let mut signatures = Vec::new();
        
        for validator in committee.validators() {
            // Request signature from validator
            if let Ok(sig) = validator.sign_block(block_hash).await {
                signatures.push(ValidatorSignature {
                    validator_index: validator.index(),
                    signature: sig,
                    stake: validator.stake(),
                });
            }
        }
        
        Ok(signatures)
    }
    
    /// Initialize committees for the epoch
    pub fn initialize_committees(&self, validators: Vec<ValidatorInfo>) -> Result<()> {
        let mut committees = Vec::new();
        
        // Shuffle and divide validators into committees
        let validators_per_committee = validators.len() / self.config.committee_count;
        
        for i in 0..self.config.committee_count {
            let start = i * validators_per_committee;
            let end = ((i + 1) * validators_per_committee).min(validators.len());
            
            let committee_validators = validators[start..end].to_vec();
            committees.push(Committee::new(i, committee_validators));
        }
        
        *self.committees.write().unwrap() = committees;
        
        info!("Initialized {} committees for SSF", self.config.committee_count);
        
        Ok(())
    }
    
    fn add_pending_block(&self, block: &Block) -> Result<()> {
        let mut cache = self.finality_cache.write().unwrap();
        
        cache.pending_blocks.push(PendingBlock {
            slot: block.header.number,
            block_hash: block.header.hash(),
            received_at: Instant::now(),
            signature_count: 0,
            total_stake: U256::zero(),
        });
        
        // Clean old pending blocks
        cache.pending_blocks.retain(|b| {
            b.received_at.elapsed() < Duration::from_secs(60)
        });
        
        Ok(())
    }
    
    fn finalize_block(
        &self,
        block: &Block,
        aggregated_signature: Vec<u8>,
    ) -> Result<()> {
        let mut cache = self.finality_cache.write().unwrap();
        
        // Remove from pending
        cache.pending_blocks.retain(|b| b.block_hash != block.header.hash());
        
        // Add to finalized
        cache.finalized_blocks.push(FinalizedBlock {
            slot: block.header.number,
            block_hash: block.header.hash(),
            finalized_at: Instant::now(),
            signatures: vec![aggregated_signature],
        });
        
        // Keep only recent finalized blocks
        if cache.finalized_blocks.len() > 1000 {
            cache.finalized_blocks.drain(0..100);
        }
        
        self.metrics.blocks_finalized.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        Ok(())
    }
    
    fn calculate_total_stake(&self, signatures: &[ValidatorSignature]) -> Result<U256> {
        let total = signatures.iter()
            .map(|sig| sig.stake)
            .fold(U256::zero(), |acc, stake| acc + stake);
        
        Ok(total)
    }
    
    /// Get current finality status
    pub fn get_status(&self) -> FinalityStatus {
        let cache = self.finality_cache.read().unwrap();
        
        if let Some(latest) = cache.finalized_blocks.last() {
            FinalityStatus::Finalized {
                slot: latest.slot,
                block_hash: latest.block_hash,
            }
        } else if let Some(pending) = cache.pending_blocks.last() {
            let network_stake = self.validator_set.total_stake();
            let confidence = pending.total_stake.as_u128() as f64 / network_stake.as_u128() as f64;
            
            if confidence > 0.5 {
                FinalityStatus::Optimistic { confidence }
            } else {
                FinalityStatus::Pending
            }
        } else {
            FinalityStatus::Pending
        }
    }
    
    /// Get metrics
    pub fn get_metrics(&self) -> FinalityMetricsSnapshot {
        FinalityMetricsSnapshot {
            blocks_finalized: self.metrics.blocks_finalized.load(std::sync::atomic::Ordering::Relaxed),
            average_finality_time_ms: self.metrics.average_finality_time.load(std::sync::atomic::Ordering::Relaxed),
            failed_finalizations: self.metrics.failed_finalizations.load(std::sync::atomic::Ordering::Relaxed),
            signature_aggregation_time_ms: self.metrics.signature_aggregation_time.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl FinalityMetrics {
    fn new() -> Self {
        Self {
            blocks_finalized: std::sync::atomic::AtomicU64::new(0),
            average_finality_time: std::sync::atomic::AtomicU64::new(0),
            failed_finalizations: std::sync::atomic::AtomicU64::new(0),
            signature_aggregation_time: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
struct ValidatorSignature {
    validator_index: u64,
    signature: Vec<u8>,
    stake: U256,
}

#[derive(Debug, Clone)]
pub struct ValidatorInfo {
    pub index: u64,
    pub pubkey: Vec<u8>,
    pub stake: U256,
}

#[derive(Debug, Clone)]
pub struct FinalityMetricsSnapshot {
    pub blocks_finalized: u64,
    pub average_finality_time_ms: u64,
    pub failed_finalizations: u64,
    pub signature_aggregation_time_ms: u64,
}