use ethereum_types::{H256, U256};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, debug, warn};
use blst::{min_pk::*, BLST_ERROR};

use crate::{Result, SSFError};

/// Signature aggregator for Single Slot Finality
/// Handles parallel signature collection and BLS aggregation
pub struct SignatureAggregator {
    strategy: AggregationStrategy,
    committee_count: usize,
    max_committee_size: usize,
    aggregation_trees: Arc<RwLock<HashMap<H256, AggregationTree>>>,
    signature_cache: Arc<RwLock<SignatureCache>>,
}

#[derive(Debug, Clone)]
pub enum AggregationStrategy {
    /// Tree-based aggregation for efficiency
    TreeBased { branching_factor: usize },
    /// Direct aggregation for simplicity
    Direct,
    /// Optimistic aggregation with early finality
    Optimistic { threshold: f64 },
    /// Adaptive strategy based on network conditions
    Adaptive,
}

impl Default for AggregationStrategy {
    fn default() -> Self {
        Self::TreeBased { branching_factor: 8 }
    }
}

struct AggregationTree {
    root: AggregationNode,
    height: usize,
    total_signatures: usize,
}

struct AggregationNode {
    signatures: Vec<Signature>,
    aggregated: Option<AggregateSignature>,
    children: Vec<Box<AggregationNode>>,
    weight: U256,
}

struct SignatureCache {
    recent_signatures: HashMap<H256, Vec<CachedSignature>>,
    cache_size: usize,
}

#[derive(Clone)]
struct CachedSignature {
    validator_index: u64,
    signature: Vec<u8>,
    stake: U256,
    timestamp: std::time::Instant,
}

impl SignatureAggregator {
    pub fn new(committee_count: usize, max_committee_size: usize) -> Result<Self> {
        Ok(Self {
            strategy: AggregationStrategy::default(),
            committee_count,
            max_committee_size,
            aggregation_trees: Arc::new(RwLock::new(HashMap::new())),
            signature_cache: Arc::new(RwLock::new(SignatureCache {
                recent_signatures: HashMap::new(),
                cache_size: 10000,
            })),
        })
    }

    /// Aggregate signatures for a block
    pub async fn aggregate_signatures(
        &self,
        signatures: &[ValidatorSignature],
        block_hash: H256,
    ) -> Result<Vec<u8>> {
        match self.strategy {
            AggregationStrategy::TreeBased { branching_factor } => {
                self.aggregate_tree_based(signatures, block_hash, branching_factor).await
            }
            AggregationStrategy::Direct => {
                self.aggregate_direct(signatures, block_hash).await
            }
            AggregationStrategy::Optimistic { threshold } => {
                self.aggregate_optimistic(signatures, block_hash, threshold).await
            }
            AggregationStrategy::Adaptive => {
                self.aggregate_adaptive(signatures, block_hash).await
            }
        }
    }

    /// Tree-based aggregation for efficiency
    async fn aggregate_tree_based(
        &self,
        signatures: &[ValidatorSignature],
        block_hash: H256,
        branching_factor: usize,
    ) -> Result<Vec<u8>> {
        let tree = self.build_aggregation_tree(signatures, branching_factor)?;
        
        // Store tree for potential reuse
        {
            let mut trees = self.aggregation_trees.write().unwrap();
            trees.insert(block_hash, tree);
            
            // Clean old trees
            if trees.len() > 100 {
                let oldest = trees.keys().next().cloned();
                if let Some(key) = oldest {
                    trees.remove(&key);
                }
            }
        }
        
        // Aggregate from tree root
        self.aggregate_from_tree(block_hash)
    }

    /// Direct aggregation without tree structure
    async fn aggregate_direct(
        &self,
        signatures: &[ValidatorSignature],
        block_hash: H256,
    ) -> Result<Vec<u8>> {
        let mut agg_sig = match AggregateSignature::from_signature(&signatures[0].to_signature()?) {
            Ok(sig) => sig,
            Err(_) => return Err(SSFError::AggregationError("Failed to create aggregate".into())),
        };

        for sig in &signatures[1..] {
            let signature = sig.to_signature()?;
            if let Err(e) = agg_sig.add_signature(&signature, false) {
                warn!("Failed to add signature: {:?}", e);
            }
        }

        Ok(agg_sig.to_signature().compress())
    }

    /// Optimistic aggregation with early finality
    async fn aggregate_optimistic(
        &self,
        signatures: &[ValidatorSignature],
        block_hash: H256,
        threshold: f64,
    ) -> Result<Vec<u8>> {
        let total_stake: U256 = signatures.iter().map(|s| s.stake).sum();
        let mut accumulated_stake = U256::zero();
        let mut agg_sig: Option<AggregateSignature> = None;

        for sig in signatures {
            accumulated_stake += sig.stake;
            
            match &mut agg_sig {
                None => {
                    agg_sig = Some(AggregateSignature::from_signature(&sig.to_signature()?)?);
                }
                Some(agg) => {
                    agg.add_signature(&sig.to_signature()?, false)?;
                }
            }

            // Check if we've reached threshold
            let ratio = accumulated_stake.as_u128() as f64 / total_stake.as_u128() as f64;
            if ratio >= threshold {
                info!("Optimistic threshold reached at {:.2}%", ratio * 100.0);
                break;
            }
        }

        agg_sig
            .ok_or_else(|| SSFError::AggregationError("No signatures to aggregate".into()))
            .map(|sig| sig.to_signature().compress())
    }

    /// Adaptive aggregation based on network conditions
    async fn aggregate_adaptive(
        &self,
        signatures: &[ValidatorSignature],
        block_hash: H256,
    ) -> Result<Vec<u8>> {
        // Analyze signature distribution
        let sig_count = signatures.len();
        let avg_stake = signatures.iter()
            .map(|s| s.stake.as_u128())
            .sum::<u128>() / sig_count as u128;

        // Choose strategy based on conditions
        if sig_count > 1000 {
            // Use tree-based for large sets
            self.aggregate_tree_based(signatures, block_hash, 16).await
        } else if avg_stake > 100_000_000_000_000_000_000 {
            // Use optimistic for high-stake validators
            self.aggregate_optimistic(signatures, block_hash, 0.67).await
        } else {
            // Use direct for small sets
            self.aggregate_direct(signatures, block_hash).await
        }
    }

    /// Build aggregation tree from signatures
    fn build_aggregation_tree(
        &self,
        signatures: &[ValidatorSignature],
        branching_factor: usize,
    ) -> Result<AggregationTree> {
        let height = ((signatures.len() as f64).log2() / (branching_factor as f64).log2()).ceil() as usize;
        
        let root = self.build_tree_node(signatures, branching_factor, 0, height)?;
        
        Ok(AggregationTree {
            root,
            height,
            total_signatures: signatures.len(),
        })
    }

    fn build_tree_node(
        &self,
        signatures: &[ValidatorSignature],
        branching_factor: usize,
        level: usize,
        max_height: usize,
    ) -> Result<AggregationNode> {
        if signatures.is_empty() {
            return Ok(AggregationNode {
                signatures: vec![],
                aggregated: None,
                children: vec![],
                weight: U256::zero(),
            });
        }

        if level >= max_height || signatures.len() <= branching_factor {
            // Leaf node
            let sigs: Vec<Signature> = signatures
                .iter()
                .filter_map(|s| s.to_signature().ok())
                .collect();
            
            let aggregated = if !sigs.is_empty() {
                let mut agg = AggregateSignature::from_signature(&sigs[0]).ok()?;
                for sig in &sigs[1..] {
                    agg.add_signature(sig, false).ok()?;
                }
                Some(agg)
            } else {
                None
            };

            let weight = signatures.iter().map(|s| s.stake).sum();

            return Ok(AggregationNode {
                signatures: sigs,
                aggregated,
                children: vec![],
                weight,
            });
        }

        // Internal node
        let chunk_size = (signatures.len() + branching_factor - 1) / branching_factor;
        let mut children = Vec::new();
        let mut total_weight = U256::zero();

        for chunk in signatures.chunks(chunk_size) {
            let child = self.build_tree_node(chunk, branching_factor, level + 1, max_height)?;
            total_weight += child.weight;
            children.push(Box::new(child));
        }

        // Aggregate children
        let mut aggregated = None;
        for child in &children {
            if let Some(ref child_agg) = child.aggregated {
                match &mut aggregated {
                    None => aggregated = Some(child_agg.clone()),
                    Some(agg) => {
                        // Combine aggregated signatures
                        // Note: This is simplified - real implementation would handle properly
                    }
                }
            }
        }

        Ok(AggregationNode {
            signatures: vec![],
            aggregated,
            children,
            weight: total_weight,
        })
    }

    /// Aggregate from stored tree
    fn aggregate_from_tree(&self, block_hash: H256) -> Result<Vec<u8>> {
        let trees = self.aggregation_trees.read().unwrap();
        let tree = trees.get(&block_hash)
            .ok_or_else(|| SSFError::AggregationError("Tree not found".into()))?;

        tree.root.aggregated
            .as_ref()
            .map(|agg| agg.to_signature().compress())
            .ok_or_else(|| SSFError::AggregationError("No aggregated signature".into()))
    }

    /// Cache signatures for potential reuse
    pub fn cache_signatures(&self, block_hash: H256, signatures: Vec<ValidatorSignature>) {
        let mut cache = self.signature_cache.write().unwrap();
        
        let cached: Vec<CachedSignature> = signatures
            .into_iter()
            .map(|sig| CachedSignature {
                validator_index: sig.validator_index,
                signature: sig.signature,
                stake: sig.stake,
                timestamp: std::time::Instant::now(),
            })
            .collect();

        cache.recent_signatures.insert(block_hash, cached);

        // Clean old entries
        if cache.recent_signatures.len() > cache.cache_size {
            let oldest = cache.recent_signatures.keys().next().cloned();
            if let Some(key) = oldest {
                cache.recent_signatures.remove(&key);
            }
        }
    }

    /// Get cached signatures if available
    pub fn get_cached_signatures(&self, block_hash: &H256) -> Option<Vec<ValidatorSignature>> {
        let cache = self.signature_cache.read().unwrap();
        cache.recent_signatures.get(block_hash).map(|cached| {
            cached.iter().map(|c| ValidatorSignature {
                validator_index: c.validator_index,
                signature: c.signature.clone(),
                stake: c.stake,
            }).collect()
        })
    }

    /// Clear aggregation cache
    pub fn clear_cache(&self) {
        self.aggregation_trees.write().unwrap().clear();
        self.signature_cache.write().unwrap().recent_signatures.clear();
    }
}

#[derive(Debug, Clone)]
pub struct ValidatorSignature {
    pub validator_index: u64,
    pub signature: Vec<u8>,
    pub stake: U256,
}

impl ValidatorSignature {
    fn to_signature(&self) -> Result<Signature> {
        Signature::from_bytes(&self.signature)
            .map_err(|e| SSFError::SignatureError(format!("Invalid signature: {:?}", e)))
    }
}