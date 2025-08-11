use ethereum_types::{H256, U256};
use ethereum_storage::Storage;
use std::sync::{Arc, RwLock};
use std::collections::HashSet;
use tokio::sync::mpsc;
use tracing::{info, debug, warn};
use async_trait::async_trait;

use crate::{Result, HistoryExpiryError};

/// Pruning engine for removing expired history
pub struct PruningEngine {
    storage: Arc<dyn Storage>,
    policy: PruningPolicy,
    state: Arc<RwLock<PruningState>>,
    metrics: Arc<PruningMetrics>,
}

#[derive(Debug, Clone)]
pub enum PruningPolicy {
    /// Conservative pruning - verify before deletion
    Conservative {
        verify_backup: bool,
        batch_size: usize,
    },
    /// Aggressive pruning - fast deletion
    Aggressive {
        parallel_workers: usize,
        skip_verification: bool,
    },
    /// Incremental pruning over time
    Incremental {
        blocks_per_cycle: usize,
        cycle_delay_ms: u64,
    },
    /// Smart pruning based on access patterns
    Smart {
        keep_frequently_accessed: bool,
        access_threshold: u64,
    },
}

impl Default for PruningPolicy {
    fn default() -> Self {
        Self::Conservative {
            verify_backup: true,
            batch_size: 100,
        }
    }
}

struct PruningState {
    is_pruning: bool,
    last_pruned_block: u64,
    total_pruned: u64,
    total_freed_bytes: u64,
    current_operation: Option<PruningOperation>,
    access_patterns: AccessPatterns,
}

#[derive(Debug, Clone)]
struct PruningOperation {
    started_at: std::time::Instant,
    blocks_to_prune: Vec<u64>,
    blocks_pruned: usize,
    estimated_size: u64,
}

struct AccessPatterns {
    block_access_count: std::collections::HashMap<u64, u64>,
    last_access_time: std::collections::HashMap<u64, std::time::Instant>,
}

struct PruningMetrics {
    blocks_pruned: std::sync::atomic::AtomicU64,
    bytes_freed: std::sync::atomic::AtomicU64,
    pruning_operations: std::sync::atomic::AtomicU64,
    pruning_failures: std::sync::atomic::AtomicU64,
    average_pruning_time_ms: std::sync::atomic::AtomicU64,
}

impl PruningEngine {
    pub fn new(storage: Arc<dyn Storage>, policy: PruningPolicy) -> Result<Self> {
        Ok(Self {
            storage,
            policy,
            state: Arc::new(RwLock::new(PruningState {
                is_pruning: false,
                last_pruned_block: 0,
                total_pruned: 0,
                total_freed_bytes: 0,
                current_operation: None,
                access_patterns: AccessPatterns {
                    block_access_count: std::collections::HashMap::new(),
                    last_access_time: std::collections::HashMap::new(),
                },
            })),
            metrics: Arc::new(PruningMetrics::new()),
        })
    }

    /// Prune a list of blocks
    pub async fn prune_blocks(&self, block_numbers: Vec<u64>) -> Result<PruneResult> {
        // Check if already pruning
        {
            let mut state = self.state.write().unwrap();
            if state.is_pruning {
                return Err(HistoryExpiryError::PruningError("Pruning already in progress".into()));
            }
            state.is_pruning = true;
            state.current_operation = Some(PruningOperation {
                started_at: std::time::Instant::now(),
                blocks_to_prune: block_numbers.clone(),
                blocks_pruned: 0,
                estimated_size: 0,
            });
        }

        let start = std::time::Instant::now();
        let result = match &self.policy {
            PruningPolicy::Conservative { verify_backup, batch_size } => {
                self.prune_conservative(block_numbers, *verify_backup, *batch_size).await
            }
            PruningPolicy::Aggressive { parallel_workers, skip_verification } => {
                self.prune_aggressive(block_numbers, *parallel_workers, *skip_verification).await
            }
            PruningPolicy::Incremental { blocks_per_cycle, cycle_delay_ms } => {
                self.prune_incremental(block_numbers, *blocks_per_cycle, *cycle_delay_ms).await
            }
            PruningPolicy::Smart { keep_frequently_accessed, access_threshold } => {
                self.prune_smart(block_numbers, *keep_frequently_accessed, *access_threshold).await
            }
        };

        // Update state and metrics
        {
            let mut state = self.state.write().unwrap();
            state.is_pruning = false;
            state.current_operation = None;
            
            if let Ok(ref res) = result {
                state.total_pruned += res.blocks_pruned;
                state.total_freed_bytes += res.bytes_freed;
                if res.blocks_pruned > 0 {
                    state.last_pruned_block = block_numbers.iter().max().copied().unwrap_or(0);
                }
            }
        }

        let elapsed = start.elapsed();
        self.metrics.average_pruning_time_ms.store(
            elapsed.as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        if let Ok(ref res) = result {
            self.metrics.blocks_pruned.fetch_add(res.blocks_pruned, std::sync::atomic::Ordering::Relaxed);
            self.metrics.bytes_freed.fetch_add(res.bytes_freed, std::sync::atomic::Ordering::Relaxed);
            self.metrics.pruning_operations.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            
            info!("Pruned {} blocks, freed {} bytes in {:?}", 
                  res.blocks_pruned, res.bytes_freed, elapsed);
        } else {
            self.metrics.pruning_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }

        result
    }

    /// Conservative pruning with verification
    async fn prune_conservative(
        &self,
        block_numbers: Vec<u64>,
        verify_backup: bool,
        batch_size: usize,
    ) -> Result<PruneResult> {
        let mut total_pruned = 0u64;
        let mut total_freed = 0u64;

        for batch in block_numbers.chunks(batch_size) {
            // Verify blocks are backed up if required
            if verify_backup {
                for &block_num in batch {
                    if !self.verify_block_backed_up(block_num).await? {
                        warn!("Block {} not backed up, skipping", block_num);
                        continue;
                    }
                }
            }

            // Get sizes before pruning
            let mut batch_size = 0u64;
            for &block_num in batch {
                if let Ok(Some(size)) = self.storage.get_block_size(block_num) {
                    batch_size += size;
                }
            }

            // Prune the batch
            match self.storage.prune_blocks(batch.to_vec()) {
                Ok(pruned) => {
                    total_pruned += pruned as u64;
                    total_freed += batch_size;
                    
                    // Update operation progress
                    {
                        let mut state = self.state.write().unwrap();
                        if let Some(ref mut op) = state.current_operation {
                            op.blocks_pruned += pruned;
                            op.estimated_size += batch_size;
                        }
                    }
                    
                    debug!("Pruned batch of {} blocks", pruned);
                }
                Err(e) => {
                    warn!("Failed to prune batch: {}", e);
                }
            }

            // Small delay between batches
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }

        Ok(PruneResult {
            blocks_pruned: total_pruned,
            bytes_freed: total_freed,
        })
    }

    /// Aggressive parallel pruning
    async fn prune_aggressive(
        &self,
        block_numbers: Vec<u64>,
        parallel_workers: usize,
        skip_verification: bool,
    ) -> Result<PruneResult> {
        use futures::future::join_all;

        let chunk_size = (block_numbers.len() + parallel_workers - 1) / parallel_workers;
        let mut handles = Vec::new();

        for chunk in block_numbers.chunks(chunk_size) {
            let storage = self.storage.clone();
            let blocks = chunk.to_vec();
            let skip_ver = skip_verification;

            let handle = tokio::spawn(async move {
                if !skip_ver {
                    // Quick verification
                    for &block_num in &blocks {
                        if storage.get_block_by_number(block_num).is_err() {
                            warn!("Block {} not found, skipping", block_num);
                            return Ok::<(u64, u64), HistoryExpiryError>((0, 0));
                        }
                    }
                }

                // Get total size
                let mut total_size = 0u64;
                for &block_num in &blocks {
                    if let Ok(Some(size)) = storage.get_block_size(block_num) {
                        total_size += size;
                    }
                }

                // Prune blocks
                match storage.prune_blocks(blocks.clone()) {
                    Ok(pruned) => Ok((pruned as u64, total_size)),
                    Err(e) => {
                        warn!("Worker failed to prune: {}", e);
                        Ok((0, 0))
                    }
                }
            });

            handles.push(handle);
        }

        // Wait for all workers
        let results = join_all(handles).await;
        
        let mut total_pruned = 0u64;
        let mut total_freed = 0u64;

        for result in results {
            match result {
                Ok(Ok((pruned, freed))) => {
                    total_pruned += pruned;
                    total_freed += freed;
                }
                Ok(Err(e)) => warn!("Worker error: {}", e),
                Err(e) => warn!("Task join error: {}", e),
            }
        }

        Ok(PruneResult {
            blocks_pruned: total_pruned,
            bytes_freed: total_freed,
        })
    }

    /// Incremental pruning over time
    async fn prune_incremental(
        &self,
        block_numbers: Vec<u64>,
        blocks_per_cycle: usize,
        cycle_delay_ms: u64,
    ) -> Result<PruneResult> {
        let mut total_pruned = 0u64;
        let mut total_freed = 0u64;

        for chunk in block_numbers.chunks(blocks_per_cycle) {
            // Get sizes
            let mut chunk_size = 0u64;
            for &block_num in chunk {
                if let Ok(Some(size)) = self.storage.get_block_size(block_num) {
                    chunk_size += size;
                }
            }

            // Prune chunk
            match self.storage.prune_blocks(chunk.to_vec()) {
                Ok(pruned) => {
                    total_pruned += pruned as u64;
                    total_freed += chunk_size;
                    
                    info!("Incrementally pruned {} blocks", pruned);
                }
                Err(e) => {
                    warn!("Incremental pruning failed: {}", e);
                }
            }

            // Delay between cycles
            tokio::time::sleep(tokio::time::Duration::from_millis(cycle_delay_ms)).await;
        }

        Ok(PruneResult {
            blocks_pruned: total_pruned,
            bytes_freed: total_freed,
        })
    }

    /// Smart pruning based on access patterns
    async fn prune_smart(
        &self,
        mut block_numbers: Vec<u64>,
        keep_frequently_accessed: bool,
        access_threshold: u64,
    ) -> Result<PruneResult> {
        if keep_frequently_accessed {
            let state = self.state.read().unwrap();
            
            // Filter out frequently accessed blocks
            block_numbers.retain(|&block_num| {
                let access_count = state.access_patterns.block_access_count
                    .get(&block_num)
                    .copied()
                    .unwrap_or(0);
                
                if access_count >= access_threshold {
                    debug!("Keeping frequently accessed block {} (accessed {} times)", 
                           block_num, access_count);
                    false
                } else {
                    true
                }
            });
        }

        // Prune remaining blocks conservatively
        self.prune_conservative(block_numbers, false, 100).await
    }

    /// Verify a block is backed up
    async fn verify_block_backed_up(&self, block_number: u64) -> Result<bool> {
        // In production, would check with archival backend
        // For now, simulate verification
        Ok(true)
    }

    /// Record block access for smart pruning
    pub fn record_access(&self, block_number: u64) {
        let mut state = self.state.write().unwrap();
        
        *state.access_patterns.block_access_count
            .entry(block_number)
            .or_insert(0) += 1;
        
        state.access_patterns.last_access_time
            .insert(block_number, std::time::Instant::now());
    }

    /// Get pruning status
    pub fn get_status(&self) -> PruningStatus {
        let state = self.state.read().unwrap();
        
        PruningStatus {
            is_pruning: state.is_pruning,
            last_pruned_block: state.last_pruned_block,
            total_pruned: state.total_pruned,
            total_freed_bytes: state.total_freed_bytes,
            current_operation: state.current_operation.as_ref().map(|op| {
                OperationStatus {
                    started_at: op.started_at,
                    total_blocks: op.blocks_to_prune.len(),
                    blocks_pruned: op.blocks_pruned,
                    estimated_size: op.estimated_size,
                    elapsed: op.started_at.elapsed(),
                }
            }),
        }
    }

    /// Get pruning statistics
    pub fn get_stats(&self) -> PruningStats {
        PruningStats {
            blocks_pruned: self.metrics.blocks_pruned.load(std::sync::atomic::Ordering::Relaxed),
            bytes_freed: self.metrics.bytes_freed.load(std::sync::atomic::Ordering::Relaxed),
            pruning_operations: self.metrics.pruning_operations.load(std::sync::atomic::Ordering::Relaxed),
            pruning_failures: self.metrics.pruning_failures.load(std::sync::atomic::Ordering::Relaxed),
            average_pruning_time_ms: self.metrics.average_pruning_time_ms.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Clean up access patterns for pruned blocks
    pub fn cleanup_access_patterns(&self, pruned_blocks: &[u64]) {
        let mut state = self.state.write().unwrap();
        
        for block_num in pruned_blocks {
            state.access_patterns.block_access_count.remove(block_num);
            state.access_patterns.last_access_time.remove(block_num);
        }
    }
}

impl PruningMetrics {
    fn new() -> Self {
        Self {
            blocks_pruned: std::sync::atomic::AtomicU64::new(0),
            bytes_freed: std::sync::atomic::AtomicU64::new(0),
            pruning_operations: std::sync::atomic::AtomicU64::new(0),
            pruning_failures: std::sync::atomic::AtomicU64::new(0),
            average_pruning_time_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PruneResult {
    pub blocks_pruned: u64,
    pub bytes_freed: u64,
}

#[derive(Debug, Clone)]
pub struct PruningStatus {
    pub is_pruning: bool,
    pub last_pruned_block: u64,
    pub total_pruned: u64,
    pub total_freed_bytes: u64,
    pub current_operation: Option<OperationStatus>,
}

#[derive(Debug, Clone)]
pub struct OperationStatus {
    pub started_at: std::time::Instant,
    pub total_blocks: usize,
    pub blocks_pruned: usize,
    pub estimated_size: u64,
    pub elapsed: std::time::Duration,
}

#[derive(Debug, Clone)]
pub struct PruningStats {
    pub blocks_pruned: u64,
    pub bytes_freed: u64,
    pub pruning_operations: u64,
    pub pruning_failures: u64,
    pub average_pruning_time_ms: u64,
}

// Extension trait for Storage to add pruning capabilities
#[async_trait]
pub trait StoragePruning: Storage {
    /// Prune multiple blocks
    fn prune_blocks(&self, block_numbers: Vec<u64>) -> std::result::Result<usize, Box<dyn std::error::Error>>;
    
    /// Get block size
    fn get_block_size(&self, block_number: u64) -> std::result::Result<Option<u64>, Box<dyn std::error::Error>>;
    
    /// Get total storage size
    fn get_total_size(&self) -> std::result::Result<u64, Box<dyn std::error::Error>>;
}

// Mock implementation for Storage trait
#[async_trait]
impl<T: Storage + ?Sized> StoragePruning for T {
    fn prune_blocks(&self, block_numbers: Vec<u64>) -> std::result::Result<usize, Box<dyn std::error::Error>> {
        // In production, would actually delete blocks from storage
        Ok(block_numbers.len())
    }
    
    fn get_block_size(&self, _block_number: u64) -> std::result::Result<Option<u64>, Box<dyn std::error::Error>> {
        // In production, would get actual block size
        Ok(Some(2_000_000)) // ~2MB average
    }
    
    fn get_total_size(&self) -> std::result::Result<u64, Box<dyn std::error::Error>> {
        // In production, would get actual storage size
        Ok(100_000_000_000) // 100GB
    }
}