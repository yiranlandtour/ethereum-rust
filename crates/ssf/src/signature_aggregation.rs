use ethereum_types::{H256, U256};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use tracing::{info, debug, warn};
use blst::{min_pk::*, BLST_ERROR};

use crate::{Result, SSFError};

/// BLS signature aggregation for SSF
pub struct BLSAggregation {
    domain: Vec<u8>,
    public_keys: Arc<RwLock<HashMap<u64, PublicKey>>>,
    signature_cache: Arc<RwLock<SignatureCache>>,
    aggregation_strategy: AggregationStrategy,
}

#[derive(Debug, Clone)]
pub enum AggregationStrategy {
    /// Sequential aggregation
    Sequential,
    /// Parallel aggregation with worker threads
    Parallel { workers: usize },
    /// Batch aggregation for efficiency
    Batched { batch_size: usize },
    /// Tree-based aggregation
    Tree { depth: usize },
}

impl Default for AggregationStrategy {
    fn default() -> Self {
        Self::Parallel { workers: 4 }
    }
}

#[derive(Debug, Clone)]
pub struct AggregatedSignature {
    pub signature: Vec<u8>,
    pub public_keys: Vec<Vec<u8>>,
    pub message: Vec<u8>,
    pub participants: Vec<u64>,
    pub total_stake: U256,
}

struct SignatureCache {
    aggregated: HashMap<H256, AggregatedSignature>,
    individual: HashMap<(u64, H256), IndividualSignature>,
    max_size: usize,
}

#[derive(Clone)]
struct IndividualSignature {
    signature: Signature,
    public_key: PublicKey,
    stake: U256,
    timestamp: std::time::Instant,
}

impl BLSAggregation {
    pub fn new(domain: Vec<u8>) -> Result<Self> {
        Ok(Self {
            domain,
            public_keys: Arc::new(RwLock::new(HashMap::new())),
            signature_cache: Arc::new(RwLock::new(SignatureCache {
                aggregated: HashMap::new(),
                individual: HashMap::new(),
                max_size: 10000,
            })),
            aggregation_strategy: AggregationStrategy::default(),
        })
    }

    /// Register a validator's public key
    pub fn register_public_key(&self, validator_index: u64, pubkey_bytes: &[u8]) -> Result<()> {
        let pubkey = PublicKey::key_validate(pubkey_bytes)
            .map_err(|e| SSFError::SignatureError(format!("Invalid public key: {:?}", e)))?;
        
        self.public_keys.write().unwrap().insert(validator_index, pubkey);
        
        debug!("Registered public key for validator {}", validator_index);
        Ok(())
    }

    /// Aggregate signatures based on strategy
    pub fn aggregate(
        &self,
        signatures: Vec<SignatureData>,
        message: &[u8],
    ) -> Result<AggregatedSignature> {
        match self.aggregation_strategy {
            AggregationStrategy::Sequential => {
                self.aggregate_sequential(signatures, message)
            }
            AggregationStrategy::Parallel { workers } => {
                self.aggregate_parallel(signatures, message, workers)
            }
            AggregationStrategy::Batched { batch_size } => {
                self.aggregate_batched(signatures, message, batch_size)
            }
            AggregationStrategy::Tree { depth } => {
                self.aggregate_tree(signatures, message, depth)
            }
        }
    }

    /// Sequential aggregation
    fn aggregate_sequential(
        &self,
        signatures: Vec<SignatureData>,
        message: &[u8],
    ) -> Result<AggregatedSignature> {
        if signatures.is_empty() {
            return Err(SSFError::SignatureError("No signatures to aggregate".into()));
        }

        let mut aggregate_signature = None;
        let mut aggregate_pubkey = None;
        let mut participants = Vec::new();
        let mut total_stake = U256::zero();
        let mut public_keys = Vec::new();

        for sig_data in signatures {
            let signature = Signature::sig_validate(&sig_data.signature, false)
                .map_err(|e| SSFError::SignatureError(format!("Invalid signature: {:?}", e)))?;

            let pubkey = self.get_public_key(sig_data.validator_index)?;

            // Verify signature
            if !self.verify_signature(&signature, &pubkey, message)? {
                warn!("Invalid signature from validator {}", sig_data.validator_index);
                continue;
            }

            // Add to aggregate
            match aggregate_signature {
                None => {
                    aggregate_signature = Some(AggregateSignature::from_signature(&signature));
                    aggregate_pubkey = Some(AggregatePublicKey::from_public_key(&pubkey));
                }
                Some(ref mut agg_sig) => {
                    if let Err(e) = agg_sig.add_signature(&signature, false) {
                        warn!("Failed to add signature: {:?}", e);
                        continue;
                    }
                    if let Some(ref mut agg_pk) = aggregate_pubkey {
                        agg_pk.add_public_key(&pubkey, false)?;
                    }
                }
            }

            participants.push(sig_data.validator_index);
            total_stake += sig_data.stake;
            public_keys.push(pubkey.to_bytes());
        }

        let final_signature = aggregate_signature
            .ok_or_else(|| SSFError::SignatureError("Failed to create aggregate".into()))?
            .to_signature()
            .compress();

        Ok(AggregatedSignature {
            signature: final_signature,
            public_keys,
            message: message.to_vec(),
            participants,
            total_stake,
        })
    }

    /// Parallel aggregation with worker threads
    fn aggregate_parallel(
        &self,
        signatures: Vec<SignatureData>,
        message: &[u8],
        workers: usize,
    ) -> Result<AggregatedSignature> {
        use std::sync::mpsc;
        use std::thread;

        if signatures.is_empty() {
            return Err(SSFError::SignatureError("No signatures to aggregate".into()));
        }

        let chunk_size = (signatures.len() + workers - 1) / workers;
        let (tx, rx) = mpsc::channel();

        let mut handles = Vec::new();
        let message = message.to_vec();

        for chunk in signatures.chunks(chunk_size) {
            let chunk = chunk.to_vec();
            let tx = tx.clone();
            let message = message.clone();
            let pubkeys = self.public_keys.clone();

            let handle = thread::spawn(move || {
                let result = Self::aggregate_chunk(chunk, &message, pubkeys);
                tx.send(result).unwrap();
            });

            handles.push(handle);
        }

        drop(tx); // Close sender

        // Collect results
        let mut partial_aggregates = Vec::new();
        for result in rx {
            partial_aggregates.push(result?);
        }

        // Merge partial aggregates
        self.merge_partial_aggregates(partial_aggregates, message.as_slice())
    }

    /// Batch aggregation for efficiency
    fn aggregate_batched(
        &self,
        signatures: Vec<SignatureData>,
        message: &[u8],
        batch_size: usize,
    ) -> Result<AggregatedSignature> {
        let mut batches = Vec::new();
        
        for batch in signatures.chunks(batch_size) {
            let batch_result = self.aggregate_sequential(batch.to_vec(), message)?;
            batches.push(batch_result);
        }

        // Merge batches
        self.merge_aggregated_signatures(batches)
    }

    /// Tree-based aggregation
    fn aggregate_tree(
        &self,
        signatures: Vec<SignatureData>,
        message: &[u8],
        depth: usize,
    ) -> Result<AggregatedSignature> {
        if signatures.len() <= 2 || depth == 0 {
            return self.aggregate_sequential(signatures, message);
        }

        let mid = signatures.len() / 2;
        let (left, right) = signatures.split_at(mid);

        // Recursively aggregate subtrees
        let left_agg = self.aggregate_tree(left.to_vec(), message, depth - 1)?;
        let right_agg = self.aggregate_tree(right.to_vec(), message, depth - 1)?;

        // Merge results
        self.merge_aggregated_signatures(vec![left_agg, right_agg])
    }

    /// Aggregate a chunk of signatures (helper for parallel processing)
    fn aggregate_chunk(
        chunk: Vec<SignatureData>,
        message: &[u8],
        public_keys: Arc<RwLock<HashMap<u64, PublicKey>>>,
    ) -> Result<PartialAggregate> {
        let mut aggregate_signature = None;
        let mut participants = Vec::new();
        let mut total_stake = U256::zero();

        for sig_data in chunk {
            let signature = Signature::sig_validate(&sig_data.signature, false)
                .map_err(|e| SSFError::SignatureError(format!("Invalid signature: {:?}", e)))?;

            // Get public key
            let pubkey = public_keys
                .read()
                .unwrap()
                .get(&sig_data.validator_index)
                .cloned()
                .ok_or_else(|| SSFError::SignatureError("Public key not found".into()))?;

            // Add to aggregate
            match aggregate_signature {
                None => {
                    aggregate_signature = Some(AggregateSignature::from_signature(&signature));
                }
                Some(ref mut agg) => {
                    agg.add_signature(&signature, false)
                        .map_err(|e| SSFError::SignatureError(format!("Failed to add: {:?}", e)))?;
                }
            }

            participants.push(sig_data.validator_index);
            total_stake += sig_data.stake;
        }

        Ok(PartialAggregate {
            signature: aggregate_signature,
            participants,
            total_stake,
        })
    }

    /// Merge partial aggregates from parallel processing
    fn merge_partial_aggregates(
        &self,
        partials: Vec<PartialAggregate>,
        message: &[u8],
    ) -> Result<AggregatedSignature> {
        let mut final_aggregate = None;
        let mut all_participants = Vec::new();
        let mut total_stake = U256::zero();

        for partial in partials {
            if let Some(sig) = partial.signature {
                match final_aggregate {
                    None => final_aggregate = Some(sig),
                    Some(ref mut agg) => {
                        // Merge aggregates
                        let sig_bytes = sig.to_signature().compress();
                        let temp_sig = Signature::from_bytes(&sig_bytes)
                            .map_err(|e| SSFError::SignatureError(format!("Merge failed: {:?}", e)))?;
                        agg.add_signature(&temp_sig, false)
                            .map_err(|e| SSFError::SignatureError(format!("Merge failed: {:?}", e)))?;
                    }
                }
            }
            all_participants.extend(partial.participants);
            total_stake += partial.total_stake;
        }

        let final_signature = final_aggregate
            .ok_or_else(|| SSFError::SignatureError("No signatures aggregated".into()))?
            .to_signature()
            .compress();

        Ok(AggregatedSignature {
            signature: final_signature,
            public_keys: Vec::new(), // Would need to track these properly
            message: message.to_vec(),
            participants: all_participants,
            total_stake,
        })
    }

    /// Merge already aggregated signatures
    fn merge_aggregated_signatures(
        &self,
        aggregated: Vec<AggregatedSignature>,
    ) -> Result<AggregatedSignature> {
        if aggregated.is_empty() {
            return Err(SSFError::SignatureError("No signatures to merge".into()));
        }

        if aggregated.len() == 1 {
            return Ok(aggregated.into_iter().next().unwrap());
        }

        let mut final_sig = AggregateSignature::from_signature(
            &Signature::from_bytes(&aggregated[0].signature)
                .map_err(|e| SSFError::SignatureError(format!("Invalid aggregate: {:?}", e)))?
        );

        let mut all_participants = aggregated[0].participants.clone();
        let mut all_pubkeys = aggregated[0].public_keys.clone();
        let mut total_stake = aggregated[0].total_stake;

        for agg in &aggregated[1..] {
            let sig = Signature::from_bytes(&agg.signature)
                .map_err(|e| SSFError::SignatureError(format!("Invalid aggregate: {:?}", e)))?;
            final_sig.add_signature(&sig, false)
                .map_err(|e| SSFError::SignatureError(format!("Merge failed: {:?}", e)))?;

            all_participants.extend(&agg.participants);
            all_pubkeys.extend(&agg.public_keys);
            total_stake += agg.total_stake;
        }

        Ok(AggregatedSignature {
            signature: final_sig.to_signature().compress(),
            public_keys: all_pubkeys,
            message: aggregated[0].message.clone(),
            participants: all_participants,
            total_stake,
        })
    }

    /// Verify an individual signature
    fn verify_signature(
        &self,
        signature: &Signature,
        public_key: &PublicKey,
        message: &[u8],
    ) -> Result<bool> {
        let result = signature.verify(true, message, &self.domain, &[], public_key, true);
        Ok(result == BLST_ERROR::BLST_SUCCESS)
    }

    /// Verify an aggregated signature
    pub fn verify_aggregate(
        &self,
        aggregate: &AggregatedSignature,
    ) -> Result<bool> {
        let signature = Signature::from_bytes(&aggregate.signature)
            .map_err(|e| SSFError::SignatureError(format!("Invalid aggregate: {:?}", e)))?;

        // Reconstruct aggregate public key
        let mut agg_pubkey = None;
        for pubkey_bytes in &aggregate.public_keys {
            let pubkey = PublicKey::key_validate(pubkey_bytes)
                .map_err(|e| SSFError::SignatureError(format!("Invalid pubkey: {:?}", e)))?;

            match agg_pubkey {
                None => agg_pubkey = Some(AggregatePublicKey::from_public_key(&pubkey)),
                Some(ref mut agg) => agg.add_public_key(&pubkey, false)?,
            }
        }

        let agg_pubkey = agg_pubkey
            .ok_or_else(|| SSFError::SignatureError("No public keys".into()))?;

        let result = signature.verify(
            true,
            &aggregate.message,
            &self.domain,
            &[],
            &agg_pubkey.to_public_key(),
            true,
        );

        Ok(result == BLST_ERROR::BLST_SUCCESS)
    }

    /// Get public key for validator
    fn get_public_key(&self, validator_index: u64) -> Result<PublicKey> {
        self.public_keys
            .read()
            .unwrap()
            .get(&validator_index)
            .cloned()
            .ok_or_else(|| SSFError::SignatureError(format!("Public key not found for validator {}", validator_index)))
    }

    /// Cache an aggregated signature
    pub fn cache_aggregate(&self, block_hash: H256, aggregate: AggregatedSignature) {
        let mut cache = self.signature_cache.write().unwrap();
        cache.aggregated.insert(block_hash, aggregate);

        // Clean old entries if cache is too large
        if cache.aggregated.len() > cache.max_size {
            let oldest = cache.aggregated.keys().next().cloned();
            if let Some(key) = oldest {
                cache.aggregated.remove(&key);
            }
        }
    }

    /// Get cached aggregate if available
    pub fn get_cached_aggregate(&self, block_hash: &H256) -> Option<AggregatedSignature> {
        self.signature_cache.read().unwrap().aggregated.get(block_hash).cloned()
    }
}

#[derive(Debug, Clone)]
pub struct SignatureData {
    pub validator_index: u64,
    pub signature: Vec<u8>,
    pub stake: U256,
}

struct PartialAggregate {
    signature: Option<AggregateSignature>,
    participants: Vec<u64>,
    total_stake: U256,
}