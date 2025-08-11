use ethereum_types::{H256, U256};
use ethereum_core::Block;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, debug, warn};
use async_trait::async_trait;

use crate::{Result, HistoryExpiryError};
use crate::archival::ArchivalBackend;
use crate::portal_integration::PortalNetworkClient;

/// History retriever for accessing expired data
pub struct HistoryRetriever {
    strategy: RetrievalStrategy,
    archival_backend: Arc<dyn ArchivalBackend>,
    portal_client: Arc<PortalNetworkClient>,
    cache: Arc<RwLock<RetrievalCache>>,
    metrics: Arc<RetrievalMetrics>,
}

#[derive(Debug, Clone)]
pub enum RetrievalStrategy {
    /// Try local archive first, then network
    LocalFirst,
    /// Try Portal Network first, then archive
    NetworkFirst,
    /// Try both in parallel, return fastest
    Parallel,
    /// Try sources based on block age
    Adaptive {
        recent_threshold: u64, // blocks
        archive_after: u64,    // blocks
    },
}

impl Default for RetrievalStrategy {
    fn default() -> Self {
        Self::Adaptive {
            recent_threshold: 10000,
            archive_after: 100000,
        }
    }
}

struct RetrievalCache {
    blocks: HashMap<H256, CachedBlock>,
    max_size: usize,
    hit_count: u64,
    miss_count: u64,
}

#[derive(Clone)]
struct CachedBlock {
    block: Block,
    retrieved_at: std::time::Instant,
    source: RetrievalSource,
}

#[derive(Debug, Clone)]
enum RetrievalSource {
    LocalArchive,
    PortalNetwork,
    Cache,
}

struct RetrievalMetrics {
    blocks_retrieved: std::sync::atomic::AtomicU64,
    archive_retrievals: std::sync::atomic::AtomicU64,
    network_retrievals: std::sync::atomic::AtomicU64,
    cache_hits: std::sync::atomic::AtomicU64,
    retrieval_failures: std::sync::atomic::AtomicU64,
    average_retrieval_time_ms: std::sync::atomic::AtomicU64,
}

impl HistoryRetriever {
    pub fn new(
        strategy: RetrievalStrategy,
        archival_backend: Arc<dyn ArchivalBackend>,
        portal_client: Arc<PortalNetworkClient>,
    ) -> Result<Self> {
        Ok(Self {
            strategy,
            archival_backend,
            portal_client,
            cache: Arc::new(RwLock::new(RetrievalCache {
                blocks: HashMap::new(),
                max_size: 1000,
                hit_count: 0,
                miss_count: 0,
            })),
            metrics: Arc::new(RetrievalMetrics::new()),
        })
    }

    /// Retrieve a block by hash
    pub async fn retrieve_block(&self, block_hash: H256) -> Result<Option<Block>> {
        let start = std::time::Instant::now();

        // Check cache first
        if let Some(block) = self.get_from_cache(&block_hash) {
            self.metrics.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(Some(block));
        }

        // Retrieve based on strategy
        let result = match &self.strategy {
            RetrievalStrategy::LocalFirst => {
                self.retrieve_local_first(block_hash).await
            }
            RetrievalStrategy::NetworkFirst => {
                self.retrieve_network_first(block_hash).await
            }
            RetrievalStrategy::Parallel => {
                self.retrieve_parallel(block_hash).await
            }
            RetrievalStrategy::Adaptive { recent_threshold, archive_after } => {
                self.retrieve_adaptive(block_hash, *recent_threshold, *archive_after).await
            }
        };

        // Update metrics
        let elapsed = start.elapsed();
        self.metrics.average_retrieval_time_ms.store(
            elapsed.as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );

        match &result {
            Ok(Some(block)) => {
                self.metrics.blocks_retrieved.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.cache_block(block_hash, block.clone(), RetrievalSource::LocalArchive);
                info!("Retrieved block {} in {:?}", block_hash, elapsed);
            }
            Ok(None) => {
                debug!("Block {} not found", block_hash);
            }
            Err(e) => {
                self.metrics.retrieval_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                warn!("Failed to retrieve block {}: {}", block_hash, e);
            }
        }

        result
    }

    /// Retrieve multiple blocks
    pub async fn retrieve_blocks(&self, block_hashes: Vec<H256>) -> Result<Vec<Block>> {
        let mut blocks = Vec::new();
        
        // Process in batches for efficiency
        for chunk in block_hashes.chunks(10) {
            let mut handles = Vec::new();
            
            for &hash in chunk {
                let retriever = self.clone();
                let handle = tokio::spawn(async move {
                    retriever.retrieve_block(hash).await
                });
                handles.push(handle);
            }
            
            for handle in handles {
                if let Ok(Ok(Some(block))) = handle.await {
                    blocks.push(block);
                }
            }
        }
        
        Ok(blocks)
    }

    /// Retrieve blocks by range
    pub async fn retrieve_block_range(&self, start: u64, end: u64) -> Result<Vec<Block>> {
        info!("Retrieving block range {}..{}", start, end);
        
        // Try to get from archive first (more efficient for ranges)
        match self.retrieve_range_from_archive(start, end).await {
            Ok(blocks) if !blocks.is_empty() => {
                info!("Retrieved {} blocks from archive", blocks.len());
                return Ok(blocks);
            }
            _ => {}
        }
        
        // Fall back to individual retrieval
        let mut blocks = Vec::new();
        for block_num in start..end {
            // Would need block hash to retrieve individual blocks
            // This is a simplified implementation
            warn!("Individual block retrieval by number not implemented");
        }
        
        Ok(blocks)
    }

    /// Local-first retrieval strategy
    async fn retrieve_local_first(&self, block_hash: H256) -> Result<Option<Block>> {
        // Try local archive
        match self.retrieve_from_archive(block_hash).await {
            Ok(Some(block)) => {
                self.metrics.archive_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(Some(block));
            }
            Ok(None) => debug!("Block not in archive"),
            Err(e) => warn!("Archive retrieval failed: {}", e),
        }
        
        // Try Portal Network
        match self.retrieve_from_network(block_hash).await {
            Ok(Some(block)) => {
                self.metrics.network_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(Some(block))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Network-first retrieval strategy
    async fn retrieve_network_first(&self, block_hash: H256) -> Result<Option<Block>> {
        // Try Portal Network
        match self.retrieve_from_network(block_hash).await {
            Ok(Some(block)) => {
                self.metrics.network_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(Some(block));
            }
            Ok(None) => debug!("Block not in network"),
            Err(e) => warn!("Network retrieval failed: {}", e),
        }
        
        // Try local archive
        match self.retrieve_from_archive(block_hash).await {
            Ok(Some(block)) => {
                self.metrics.archive_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(Some(block))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Parallel retrieval strategy
    async fn retrieve_parallel(&self, block_hash: H256) -> Result<Option<Block>> {
        let archive_handle = {
            let backend = self.archival_backend.clone();
            let hash = block_hash;
            tokio::spawn(async move {
                // Simplified - would need proper archive ID lookup
                backend.retrieve_blocks("archive_id", 0..1).await
            })
        };
        
        let network_handle = {
            let client = self.portal_client.clone();
            let hash = block_hash;
            tokio::spawn(async move {
                client.retrieve_block(hash).await
            })
        };
        
        // Race both sources
        tokio::select! {
            archive_result = archive_handle => {
                match archive_result {
                    Ok(Ok(blocks)) if !blocks.is_empty() => {
                        self.metrics.archive_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(Some(blocks.into_iter().next().unwrap()))
                    }
                    _ => {
                        // Wait for network result
                        match network_handle.await {
                            Ok(Ok(block)) => {
                                self.metrics.network_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                Ok(block)
                            }
                            _ => Ok(None)
                        }
                    }
                }
            }
            network_result = network_handle => {
                match network_result {
                    Ok(Ok(Some(block))) => {
                        self.metrics.network_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        Ok(Some(block))
                    }
                    _ => {
                        // Wait for archive result
                        match archive_handle.await {
                            Ok(Ok(blocks)) if !blocks.is_empty() => {
                                self.metrics.archive_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                Ok(Some(blocks.into_iter().next().unwrap()))
                            }
                            _ => Ok(None)
                        }
                    }
                }
            }
        }
    }

    /// Adaptive retrieval strategy
    async fn retrieve_adaptive(
        &self,
        block_hash: H256,
        recent_threshold: u64,
        archive_after: u64,
    ) -> Result<Option<Block>> {
        // In production, would determine block age
        // For now, use a mixed approach
        
        // Try network for recent blocks
        if let Ok(Some(block)) = self.retrieve_from_network(block_hash).await {
            self.metrics.network_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(Some(block));
        }
        
        // Try archive for older blocks
        if let Ok(Some(block)) = self.retrieve_from_archive(block_hash).await {
            self.metrics.archive_retrievals.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Ok(Some(block));
        }
        
        Ok(None)
    }

    /// Retrieve from local archive
    async fn retrieve_from_archive(&self, block_hash: H256) -> Result<Option<Block>> {
        // Simplified - would need proper archive ID and range lookup
        match self.archival_backend.retrieve_blocks("archive_id", 0..1).await {
            Ok(blocks) => Ok(blocks.into_iter().next()),
            Err(e) => Err(e),
        }
    }

    /// Retrieve from Portal Network
    async fn retrieve_from_network(&self, block_hash: H256) -> Result<Option<Block>> {
        self.portal_client.retrieve_block(block_hash).await
    }

    /// Retrieve range from archive
    async fn retrieve_range_from_archive(&self, start: u64, end: u64) -> Result<Vec<Block>> {
        // Simplified - would need proper archive ID lookup
        self.archival_backend.retrieve_blocks("archive_id", start..end).await
    }

    /// Get block from cache
    fn get_from_cache(&self, block_hash: &H256) -> Option<Block> {
        let mut cache = self.cache.write().unwrap();
        
        if let Some(cached) = cache.blocks.get(block_hash) {
            cache.hit_count += 1;
            
            // Check if not too old (5 minutes)
            if cached.retrieved_at.elapsed() < std::time::Duration::from_secs(300) {
                return Some(cached.block.clone());
            }
            
            // Remove stale entry
            cache.blocks.remove(block_hash);
        }
        
        cache.miss_count += 1;
        None
    }

    /// Cache a retrieved block
    fn cache_block(&self, block_hash: H256, block: Block, source: RetrievalSource) {
        let mut cache = self.cache.write().unwrap();
        
        // Evict oldest if at capacity
        if cache.blocks.len() >= cache.max_size {
            let oldest = cache.blocks
                .iter()
                .min_by_key(|(_, v)| v.retrieved_at)
                .map(|(k, _)| k.clone());
            
            if let Some(key) = oldest {
                cache.blocks.remove(&key);
            }
        }
        
        cache.blocks.insert(block_hash, CachedBlock {
            block,
            retrieved_at: std::time::Instant::now(),
            source,
        });
    }

    /// Get retrieval statistics
    pub fn get_stats(&self) -> RetrievalStats {
        let cache = self.cache.read().unwrap();
        
        RetrievalStats {
            blocks_retrieved: self.metrics.blocks_retrieved.load(std::sync::atomic::Ordering::Relaxed),
            archive_retrievals: self.metrics.archive_retrievals.load(std::sync::atomic::Ordering::Relaxed),
            network_retrievals: self.metrics.network_retrievals.load(std::sync::atomic::Ordering::Relaxed),
            cache_hits: self.metrics.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
            cache_size: cache.blocks.len(),
            cache_hit_rate: if cache.hit_count + cache.miss_count > 0 {
                cache.hit_count as f64 / (cache.hit_count + cache.miss_count) as f64
            } else {
                0.0
            },
            retrieval_failures: self.metrics.retrieval_failures.load(std::sync::atomic::Ordering::Relaxed),
            average_retrieval_time_ms: self.metrics.average_retrieval_time_ms.load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl Clone for HistoryRetriever {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            archival_backend: self.archival_backend.clone(),
            portal_client: self.portal_client.clone(),
            cache: self.cache.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl RetrievalMetrics {
    fn new() -> Self {
        Self {
            blocks_retrieved: std::sync::atomic::AtomicU64::new(0),
            archive_retrievals: std::sync::atomic::AtomicU64::new(0),
            network_retrievals: std::sync::atomic::AtomicU64::new(0),
            cache_hits: std::sync::atomic::AtomicU64::new(0),
            retrieval_failures: std::sync::atomic::AtomicU64::new(0),
            average_retrieval_time_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RetrievalStats {
    pub blocks_retrieved: u64,
    pub archive_retrievals: u64,
    pub network_retrievals: u64,
    pub cache_hits: u64,
    pub cache_size: usize,
    pub cache_hit_rate: f64,
    pub retrieval_failures: u64,
    pub average_retrieval_time_ms: u64,
}