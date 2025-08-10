use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::{DASError, Result, DataColumn};
use crate::erasure::{ErasureCoding, CodedData};

/// Result of data reconstruction
#[derive(Debug, Clone)]
pub struct ReconstructionResult {
    pub data: Vec<u8>,
    pub columns_used: Vec<u64>,
    pub missing_columns: Vec<u64>,
    pub reconstruction_time_ms: u64,
}

/// Data reconstructor for PeerDAS
pub struct DataReconstructor {
    data_columns: usize,
    erasure_coder: Arc<ErasureCoding>,
    cache: Arc<RwLock<ReconstructionCache>>,
    metrics: Arc<ReconstructionMetrics>,
}

struct ReconstructionCache {
    entries: HashMap<ethereum_types::H256, CachedReconstruction>,
    max_entries: usize,
}

struct CachedReconstruction {
    data: Vec<u8>,
    reconstructed_at: std::time::Instant,
    columns_used: Vec<u64>,
}

struct ReconstructionMetrics {
    total_reconstructions: std::sync::atomic::AtomicU64,
    successful_reconstructions: std::sync::atomic::AtomicU64,
    failed_reconstructions: std::sync::atomic::AtomicU64,
    avg_columns_used: std::sync::atomic::AtomicU64,
}

impl DataReconstructor {
    pub fn new(data_columns: usize) -> Self {
        let erasure_coder = Arc::new(
            ErasureCoding::new(data_columns, data_columns).expect("Failed to create erasure coder")
        );
        
        Self {
            data_columns,
            erasure_coder,
            cache: Arc::new(RwLock::new(ReconstructionCache::new(100))),
            metrics: Arc::new(ReconstructionMetrics::new()),
        }
    }
    
    /// Reconstruct data from available columns
    pub async fn reconstruct(&self, columns: Vec<DataColumn>) -> Result<Vec<u8>> {
        let start = std::time::Instant::now();
        
        self.metrics.total_reconstructions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Check if we have enough columns
        if columns.len() < self.data_columns {
            self.metrics.failed_reconstructions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(DASError::InsufficientSamples(
                columns.len(),
                self.data_columns,
            ));
        }
        
        // Sort columns by index
        let mut sorted_columns = columns;
        sorted_columns.sort_by_key(|c| c.index);
        
        // Identify available and missing columns
        let available_indices: HashSet<u64> = sorted_columns.iter()
            .map(|c| c.index)
            .collect();
        
        let missing_indices: Vec<u64> = (0..self.data_columns as u64 * 2)
            .filter(|i| !available_indices.contains(i))
            .collect();
        
        info!(
            "Reconstructing data from {} columns, {} missing",
            sorted_columns.len(),
            missing_indices.len()
        );
        
        // Prepare column data for reconstruction
        let mut column_data = HashMap::new();
        for column in sorted_columns.iter() {
            column_data.insert(column.index as usize, column.column.clone());
        }
        
        // Perform reconstruction
        let reconstructed = self.erasure_coder.reconstruct(column_data)?;
        
        self.metrics.successful_reconstructions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.metrics.avg_columns_used.store(
            sorted_columns.len() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        
        let elapsed = start.elapsed().as_millis() as u64;
        info!("Data reconstruction completed in {}ms", elapsed);
        
        Ok(reconstructed)
    }
    
    /// Reconstruct specific columns from available data
    pub async fn reconstruct_columns(
        &self,
        available: Vec<DataColumn>,
        target_indices: Vec<u64>,
    ) -> Result<Vec<DataColumn>> {
        debug!(
            "Reconstructing {} columns from {} available columns",
            target_indices.len(),
            available.len()
        );
        
        // First reconstruct the full data
        let data = self.reconstruct(available.clone()).await?;
        
        // Then re-encode to get the missing columns
        let coded = self.erasure_coder.encode(&data)?;
        
        // Extract requested columns
        let mut reconstructed_columns = Vec::new();
        for index in target_indices {
            if let Some(column_data) = coded.get_column(index as usize) {
                let mut column = DataColumn::new(index, column_data);
                
                // Copy KZG commitments and proofs from available columns if possible
                if let Some(available_col) = available.iter().find(|c| c.index == index) {
                    column.kzg_commitments = available_col.kzg_commitments.clone();
                    column.kzg_proofs = available_col.kzg_proofs.clone();
                }
                
                reconstructed_columns.push(column);
            } else {
                return Err(DASError::ReconstructionFailed(
                    format!("Failed to reconstruct column {}", index)
                ));
            }
        }
        
        Ok(reconstructed_columns)
    }
    
    /// Verify reconstruction by checking against known columns
    pub async fn verify_reconstruction(
        &self,
        reconstructed: &[u8],
        known_columns: &[DataColumn],
    ) -> Result<bool> {
        // Re-encode the reconstructed data
        let coded = self.erasure_coder.encode(reconstructed)?;
        
        // Check that known columns match
        for column in known_columns {
            if let Some(reconstructed_column) = coded.get_column(column.index as usize) {
                if reconstructed_column != column.column {
                    warn!(
                        "Reconstruction verification failed for column {}",
                        column.index
                    );
                    return Ok(false);
                }
            }
        }
        
        debug!("Reconstruction verification successful");
        Ok(true)
    }
    
    /// Get reconstruction metrics
    pub fn get_metrics(&self) -> ReconstructionMetricsSnapshot {
        ReconstructionMetricsSnapshot {
            total_reconstructions: self.metrics.total_reconstructions
                .load(std::sync::atomic::Ordering::Relaxed),
            successful_reconstructions: self.metrics.successful_reconstructions
                .load(std::sync::atomic::Ordering::Relaxed),
            failed_reconstructions: self.metrics.failed_reconstructions
                .load(std::sync::atomic::Ordering::Relaxed),
            avg_columns_used: self.metrics.avg_columns_used
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl ReconstructionCache {
    fn new(max_entries: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
        }
    }
    
    fn get(&self, key: &ethereum_types::H256) -> Option<&CachedReconstruction> {
        self.entries.get(key)
    }
    
    fn insert(&mut self, key: ethereum_types::H256, reconstruction: CachedReconstruction) {
        if self.entries.len() >= self.max_entries {
            self.evict_oldest();
        }
        self.entries.insert(key, reconstruction);
    }
    
    fn evict_oldest(&mut self) {
        if let Some(oldest_key) = self.entries
            .iter()
            .min_by_key(|(_, r)| r.reconstructed_at)
            .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest_key);
        }
    }
}

impl ReconstructionMetrics {
    fn new() -> Self {
        Self {
            total_reconstructions: std::sync::atomic::AtomicU64::new(0),
            successful_reconstructions: std::sync::atomic::AtomicU64::new(0),
            failed_reconstructions: std::sync::atomic::AtomicU64::new(0),
            avg_columns_used: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReconstructionMetricsSnapshot {
    pub total_reconstructions: u64,
    pub successful_reconstructions: u64,
    pub failed_reconstructions: u64,
    pub avg_columns_used: u64,
}

/// Strategy for selecting columns for reconstruction
#[derive(Debug, Clone)]
pub enum ReconstructionStrategy {
    /// Use the first K available columns
    FirstK,
    /// Prefer systematic (data) columns over parity columns
    PreferSystematic,
    /// Select columns with lowest latency
    LowestLatency,
    /// Select columns with highest reliability
    HighestReliability,
}

impl ReconstructionStrategy {
    pub fn select_columns(
        &self,
        available: Vec<DataColumn>,
        required: usize,
    ) -> Vec<DataColumn> {
        match self {
            Self::FirstK => {
                available.into_iter().take(required).collect()
            }
            Self::PreferSystematic => {
                let mut systematic = Vec::new();
                let mut parity = Vec::new();
                
                for column in available {
                    if column.index < required as u64 {
                        systematic.push(column);
                    } else {
                        parity.push(column);
                    }
                }
                
                // Take systematic columns first, then parity if needed
                systematic.into_iter()
                    .chain(parity.into_iter())
                    .take(required)
                    .collect()
            }
            Self::LowestLatency | Self::HighestReliability => {
                // In production, these would consider network metrics
                // For now, just use FirstK strategy
                Self::FirstK.select_columns(available, required)
            }
        }
    }
}

/// Parallel reconstruction coordinator
pub struct ParallelReconstructor {
    reconstructors: Vec<Arc<DataReconstructor>>,
    shard_size: usize,
}

impl ParallelReconstructor {
    pub fn new(num_shards: usize, data_columns: usize) -> Self {
        let mut reconstructors = Vec::new();
        for _ in 0..num_shards {
            reconstructors.push(Arc::new(DataReconstructor::new(data_columns)));
        }
        
        Self {
            reconstructors,
            shard_size: data_columns / num_shards,
        }
    }
    
    /// Reconstruct data in parallel across multiple shards
    pub async fn reconstruct_parallel(
        &self,
        columns: Vec<DataColumn>,
    ) -> Result<Vec<u8>> {
        // Divide columns into shards
        let mut shards = vec![Vec::new(); self.reconstructors.len()];
        
        for column in columns {
            let shard_idx = (column.index as usize / self.shard_size)
                .min(self.reconstructors.len() - 1);
            shards[shard_idx].push(column);
        }
        
        // Reconstruct each shard in parallel
        let mut handles = Vec::new();
        for (idx, shard_columns) in shards.into_iter().enumerate() {
            if !shard_columns.is_empty() {
                let reconstructor = self.reconstructors[idx].clone();
                let handle = tokio::spawn(async move {
                    reconstructor.reconstruct(shard_columns).await
                });
                handles.push(handle);
            }
        }
        
        // Collect results
        let mut reconstructed_data = Vec::new();
        for handle in handles {
            let shard_data = handle.await
                .map_err(|e| DASError::ReconstructionFailed(e.to_string()))??;
            reconstructed_data.extend(shard_data);
        }
        
        Ok(reconstructed_data)
    }
}