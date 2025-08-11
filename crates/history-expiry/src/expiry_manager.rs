use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
use ethereum_storage::Storage;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tracing::{info, debug, warn};
use cron::Schedule;
use std::str::FromStr;

use crate::{Result, HistoryExpiryError};
use crate::archival::{ArchivalBackend, ArchivalStrategy};
use crate::pruning::{PruningEngine, PruningPolicy};
use crate::portal_integration::PortalNetworkClient;

/// History expiry manager implementing EIP-4444
pub struct HistoryExpiryManager {
    config: ExpiryConfig,
    storage: Arc<dyn Storage>,
    archival_backend: Arc<dyn ArchivalBackend>,
    portal_client: Arc<PortalNetworkClient>,
    pruning_engine: Arc<PruningEngine>,
    expiry_state: Arc<RwLock<ExpiryState>>,
    metrics: Arc<ExpiryMetrics>,
}

#[derive(Debug, Clone)]
pub struct ExpiryConfig {
    /// Block age before expiry (default: 1 year)
    pub expiry_period: Duration,
    /// Minimum blocks to keep (default: 128)
    pub min_blocks_retained: u64,
    /// Enable automatic expiry
    pub auto_expiry: bool,
    /// Expiry check interval
    pub check_interval: Duration,
    /// Archive before expiry
    pub archive_before_expiry: bool,
    /// Distribute to Portal Network
    pub portal_distribution: bool,
    /// Expiry policy
    pub policy: ExpiryPolicy,
}

impl Default for ExpiryConfig {
    fn default() -> Self {
        Self {
            expiry_period: Duration::from_secs(365 * 24 * 60 * 60), // 1 year
            min_blocks_retained: 128,
            auto_expiry: true,
            check_interval: Duration::from_secs(3600), // 1 hour
            archive_before_expiry: true,
            portal_distribution: true,
            policy: ExpiryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExpiryPolicy {
    /// Expire based on age only
    AgeBased { max_age: Duration },
    /// Expire based on block number
    BlockBased { blocks_to_keep: u64 },
    /// Expire based on storage size
    SizeBased { max_size_gb: u64 },
    /// Hybrid policy combining multiple factors
    Hybrid {
        max_age: Duration,
        max_blocks: u64,
        max_size_gb: u64,
    },
    /// Custom policy with schedule
    Scheduled { cron_expression: String },
}

impl Default for ExpiryPolicy {
    fn default() -> Self {
        Self::Hybrid {
            max_age: Duration::from_secs(365 * 24 * 60 * 60),
            max_blocks: 1_000_000,
            max_size_gb: 100,
        }
    }
}

struct ExpiryState {
    last_expired_block: u64,
    last_expiry_time: SystemTime,
    total_expired_blocks: u64,
    total_expired_size: u64,
    is_expiring: bool,
}

struct ExpiryMetrics {
    blocks_expired: std::sync::atomic::AtomicU64,
    bytes_freed: std::sync::atomic::AtomicU64,
    expiry_duration_ms: std::sync::atomic::AtomicU64,
    archival_success: std::sync::atomic::AtomicU64,
    archival_failures: std::sync::atomic::AtomicU64,
}

impl HistoryExpiryManager {
    pub fn new(
        config: ExpiryConfig,
        storage: Arc<dyn Storage>,
        archival_backend: Arc<dyn ArchivalBackend>,
        portal_client: Arc<PortalNetworkClient>,
    ) -> Result<Self> {
        let pruning_engine = Arc::new(PruningEngine::new(
            storage.clone(),
            Default::default(),
        )?);

        Ok(Self {
            config,
            storage,
            archival_backend,
            portal_client,
            pruning_engine,
            expiry_state: Arc::new(RwLock::new(ExpiryState {
                last_expired_block: 0,
                last_expiry_time: SystemTime::now(),
                total_expired_blocks: 0,
                total_expired_size: 0,
                is_expiring: false,
            })),
            metrics: Arc::new(ExpiryMetrics::new()),
        })
    }

    /// Start automatic history expiry
    pub async fn start(&self) -> Result<()> {
        if !self.config.auto_expiry {
            info!("Automatic history expiry is disabled");
            return Ok(());
        }

        let manager = self.clone();
        tokio::spawn(async move {
            manager.expiry_loop().await;
        });

        info!("History expiry manager started with {:?} policy", self.config.policy);
        Ok(())
    }

    /// Main expiry loop
    async fn expiry_loop(&self) {
        let mut interval = tokio::time::interval(self.config.check_interval);

        loop {
            interval.tick().await;

            if let Err(e) = self.check_and_expire().await {
                warn!("Expiry check failed: {}", e);
            }
        }
    }

    /// Check and expire old blocks
    pub async fn check_and_expire(&self) -> Result<()> {
        // Check if already expiring
        {
            let mut state = self.expiry_state.write().unwrap();
            if state.is_expiring {
                debug!("Expiry already in progress, skipping");
                return Ok(());
            }
            state.is_expiring = true;
        }

        let start = std::time::Instant::now();
        let result = self.perform_expiry().await;

        // Update state
        {
            let mut state = self.expiry_state.write().unwrap();
            state.is_expiring = false;
            state.last_expiry_time = SystemTime::now();
        }

        let duration = start.elapsed();
        self.metrics.expiry_duration_ms.store(
            duration.as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        result
    }

    /// Perform the actual expiry
    async fn perform_expiry(&self) -> Result<()> {
        let blocks_to_expire = self.identify_blocks_to_expire().await?;

        if blocks_to_expire.is_empty() {
            debug!("No blocks to expire");
            return Ok(());
        }

        info!("Found {} blocks to expire", blocks_to_expire.len());

        // Archive blocks if configured
        if self.config.archive_before_expiry {
            self.archive_blocks(&blocks_to_expire).await?;
        }

        // Distribute to Portal Network if configured
        if self.config.portal_distribution {
            self.distribute_to_portal(&blocks_to_expire).await?;
        }

        // Prune the blocks
        let pruned = self.prune_blocks(&blocks_to_expire).await?;

        // Update metrics
        self.metrics.blocks_expired.fetch_add(
            pruned.blocks_pruned,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.metrics.bytes_freed.fetch_add(
            pruned.bytes_freed,
            std::sync::atomic::Ordering::Relaxed,
        );

        // Update state
        {
            let mut state = self.expiry_state.write().unwrap();
            if let Some(last) = blocks_to_expire.last() {
                state.last_expired_block = *last;
            }
            state.total_expired_blocks += pruned.blocks_pruned;
            state.total_expired_size += pruned.bytes_freed;
        }

        info!(
            "Expired {} blocks, freed {} bytes",
            pruned.blocks_pruned, pruned.bytes_freed
        );

        Ok(())
    }

    /// Identify blocks that should be expired
    async fn identify_blocks_to_expire(&self) -> Result<Vec<u64>> {
        match &self.config.policy {
            ExpiryPolicy::AgeBased { max_age } => {
                self.identify_by_age(*max_age).await
            }
            ExpiryPolicy::BlockBased { blocks_to_keep } => {
                self.identify_by_block_count(*blocks_to_keep).await
            }
            ExpiryPolicy::SizeBased { max_size_gb } => {
                self.identify_by_size(*max_size_gb).await
            }
            ExpiryPolicy::Hybrid { max_age, max_blocks, max_size_gb } => {
                self.identify_hybrid(*max_age, *max_blocks, *max_size_gb).await
            }
            ExpiryPolicy::Scheduled { cron_expression } => {
                self.identify_by_schedule(cron_expression).await
            }
        }
    }

    /// Identify blocks by age
    async fn identify_by_age(&self, max_age: Duration) -> Result<Vec<u64>> {
        let current_time = SystemTime::now();
        let cutoff_time = current_time - max_age;
        
        let mut blocks_to_expire = Vec::new();
        let latest_block = self.storage.get_latest_block_number()
            .map_err(|e| HistoryExpiryError::ExpiryFailed(e.to_string()))?;

        // Keep minimum blocks
        let earliest_to_check = if latest_block > self.config.min_blocks_retained {
            latest_block - self.config.min_blocks_retained
        } else {
            return Ok(vec![]);
        };

        // Check blocks from oldest to newest
        for block_num in 0..earliest_to_check {
            if let Ok(Some(block)) = self.storage.get_block_by_number(block_num) {
                if let Ok(block_time) = SystemTime::UNIX_EPOCH.checked_add(
                    Duration::from_secs(block.header.timestamp)
                ) {
                    if block_time < cutoff_time {
                        blocks_to_expire.push(block_num);
                    } else {
                        break; // Blocks are ordered, so we can stop here
                    }
                }
            }
        }

        Ok(blocks_to_expire)
    }

    /// Identify blocks by count
    async fn identify_by_block_count(&self, blocks_to_keep: u64) -> Result<Vec<u64>> {
        let latest_block = self.storage.get_latest_block_number()
            .map_err(|e| HistoryExpiryError::ExpiryFailed(e.to_string()))?;

        let blocks_to_keep = blocks_to_keep.max(self.config.min_blocks_retained);

        if latest_block <= blocks_to_keep {
            return Ok(vec![]);
        }

        let expire_before = latest_block - blocks_to_keep;
        Ok((0..expire_before).collect())
    }

    /// Identify blocks by storage size
    async fn identify_by_size(&self, max_size_gb: u64) -> Result<Vec<u64>> {
        let max_size_bytes = max_size_gb * 1_073_741_824; // Convert GB to bytes
        let current_size = self.storage.get_total_size()
            .map_err(|e| HistoryExpiryError::ExpiryFailed(e.to_string()))?;

        if current_size <= max_size_bytes {
            return Ok(vec![]);
        }

        let size_to_free = current_size - max_size_bytes;
        let mut blocks_to_expire = Vec::new();
        let mut freed_size = 0u64;

        // Start from oldest blocks
        let mut block_num = 0u64;
        while freed_size < size_to_free {
            if let Ok(Some(size)) = self.storage.get_block_size(block_num) {
                blocks_to_expire.push(block_num);
                freed_size += size;
            }
            block_num += 1;

            // Ensure we keep minimum blocks
            let latest = self.storage.get_latest_block_number()
                .map_err(|e| HistoryExpiryError::ExpiryFailed(e.to_string()))?;
            if latest - block_num < self.config.min_blocks_retained {
                break;
            }
        }

        Ok(blocks_to_expire)
    }

    /// Hybrid identification strategy
    async fn identify_hybrid(
        &self,
        max_age: Duration,
        max_blocks: u64,
        max_size_gb: u64,
    ) -> Result<Vec<u64>> {
        // Get candidates from each strategy
        let by_age = self.identify_by_age(max_age).await?;
        let by_count = self.identify_by_block_count(max_blocks).await?;
        let by_size = self.identify_by_size(max_size_gb).await?;

        // Union of all strategies (most aggressive)
        let mut all_blocks: Vec<u64> = by_age.iter()
            .chain(by_count.iter())
            .chain(by_size.iter())
            .cloned()
            .collect();
        
        all_blocks.sort_unstable();
        all_blocks.dedup();

        Ok(all_blocks)
    }

    /// Schedule-based identification
    async fn identify_by_schedule(&self, cron_expression: &str) -> Result<Vec<u64>> {
        let schedule = Schedule::from_str(cron_expression)
            .map_err(|e| HistoryExpiryError::ExpiryFailed(format!("Invalid cron: {}", e)))?;

        let now = chrono::Utc::now();
        
        // Check if it's time to expire according to schedule
        if let Some(next) = schedule.upcoming(chrono::Utc).next() {
            if next <= now {
                // Use default age-based strategy when scheduled
                return self.identify_by_age(self.config.expiry_period).await;
            }
        }

        Ok(vec![])
    }

    /// Archive blocks before expiry
    async fn archive_blocks(&self, block_numbers: &[u64]) -> Result<()> {
        info!("Archiving {} blocks before expiry", block_numbers.len());

        for chunk in block_numbers.chunks(100) {
            let mut blocks = Vec::new();
            
            for &block_num in chunk {
                if let Ok(Some(block)) = self.storage.get_block_by_number(block_num) {
                    blocks.push(block);
                }
            }

            match self.archival_backend.archive_blocks(blocks).await {
                Ok(_) => {
                    self.metrics.archival_success.fetch_add(
                        chunk.len() as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
                Err(e) => {
                    warn!("Failed to archive chunk: {}", e);
                    self.metrics.archival_failures.fetch_add(
                        chunk.len() as u64,
                        std::sync::atomic::Ordering::Relaxed,
                    );
                }
            }
        }

        Ok(())
    }

    /// Distribute blocks to Portal Network
    async fn distribute_to_portal(&self, block_numbers: &[u64]) -> Result<()> {
        info!("Distributing {} blocks to Portal Network", block_numbers.len());

        for &block_num in block_numbers {
            if let Ok(Some(block)) = self.storage.get_block_by_number(block_num) {
                if let Err(e) = self.portal_client.distribute_block(block).await {
                    warn!("Failed to distribute block {} to Portal: {}", block_num, e);
                }
            }
        }

        Ok(())
    }

    /// Prune blocks from storage
    async fn prune_blocks(&self, block_numbers: &[u64]) -> Result<PruneResult> {
        self.pruning_engine.prune_blocks(block_numbers.to_vec()).await
    }

    /// Get expiry statistics
    pub fn get_stats(&self) -> ExpiryStats {
        let state = self.expiry_state.read().unwrap();
        
        ExpiryStats {
            last_expired_block: state.last_expired_block,
            last_expiry_time: state.last_expiry_time,
            total_expired_blocks: state.total_expired_blocks,
            total_expired_size: state.total_expired_size,
            blocks_expired: self.metrics.blocks_expired.load(std::sync::atomic::Ordering::Relaxed),
            bytes_freed: self.metrics.bytes_freed.load(std::sync::atomic::Ordering::Relaxed),
            archival_success: self.metrics.archival_success.load(std::sync::atomic::Ordering::Relaxed),
            archival_failures: self.metrics.archival_failures.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl Clone for HistoryExpiryManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            storage: self.storage.clone(),
            archival_backend: self.archival_backend.clone(),
            portal_client: self.portal_client.clone(),
            pruning_engine: self.pruning_engine.clone(),
            expiry_state: self.expiry_state.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl ExpiryMetrics {
    fn new() -> Self {
        Self {
            blocks_expired: std::sync::atomic::AtomicU64::new(0),
            bytes_freed: std::sync::atomic::AtomicU64::new(0),
            expiry_duration_ms: std::sync::atomic::AtomicU64::new(0),
            archival_success: std::sync::atomic::AtomicU64::new(0),
            archival_failures: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExpiryStats {
    pub last_expired_block: u64,
    pub last_expiry_time: SystemTime,
    pub total_expired_blocks: u64,
    pub total_expired_size: u64,
    pub blocks_expired: u64,
    pub bytes_freed: u64,
    pub archival_success: u64,
    pub archival_failures: u64,
}

#[derive(Debug, Clone)]
pub struct PruneResult {
    pub blocks_pruned: u64,
    pub bytes_freed: u64,
}