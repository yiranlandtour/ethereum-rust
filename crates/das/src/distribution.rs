use ethereum_types::H256;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};
use async_trait::async_trait;

use crate::{DASError, Result, DataColumn};

/// Distribution strategy for data columns
#[derive(Debug, Clone)]
pub enum DistributionStrategy {
    /// Distribute to all peers in custody subnet
    Broadcast,
    /// Distribute to minimum required peers
    Minimal,
    /// Distribute based on peer reliability
    ReliabilityBased,
    /// Distribute with geographic diversity
    GeoDiverse,
}

/// Network interface for distribution
#[async_trait]
pub trait DistributionNetwork: Send + Sync {
    async fn send_column(
        &self,
        peer_id: &[u8; 32],
        column: &DataColumn,
    ) -> Result<()>;
    
    async fn get_peer_info(&self, peer_id: &[u8; 32]) -> Result<PeerInfo>;
    
    async fn find_custody_peers(&self, column_index: u64) -> Result<Vec<[u8; 32]>>;
}

#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub peer_id: [u8; 32],
    pub latency_ms: u32,
    pub reliability_score: f64,
    pub location: Option<String>,
    pub custody_columns: Vec<u64>,
}

/// Data distributor for PeerDAS
pub struct DataDistributor {
    strategy: DistributionStrategy,
    network: Option<Arc<dyn DistributionNetwork>>,
    peer_tracker: Arc<RwLock<PeerTracker>>,
    distribution_queue: Arc<RwLock<DistributionQueue>>,
    metrics: Arc<DistributionMetrics>,
}

struct PeerTracker {
    peers: HashMap<[u8; 32], TrackedPeer>,
    column_custodians: HashMap<u64, HashSet<[u8; 32]>>,
}

struct TrackedPeer {
    info: PeerInfo,
    last_seen: Instant,
    successful_sends: u64,
    failed_sends: u64,
}

struct DistributionQueue {
    pending: Vec<PendingDistribution>,
    max_queue_size: usize,
}

struct PendingDistribution {
    column: DataColumn,
    target_peers: Vec<[u8; 32]>,
    created_at: Instant,
    attempts: u32,
}

struct DistributionMetrics {
    total_distributions: std::sync::atomic::AtomicU64,
    successful_distributions: std::sync::atomic::AtomicU64,
    failed_distributions: std::sync::atomic::AtomicU64,
    avg_distribution_time_ms: std::sync::atomic::AtomicU64,
}

impl DataDistributor {
    pub fn new(strategy: DistributionStrategy) -> Self {
        Self {
            strategy,
            network: None,
            peer_tracker: Arc::new(RwLock::new(PeerTracker::new())),
            distribution_queue: Arc::new(RwLock::new(DistributionQueue::new(1000))),
            metrics: Arc::new(DistributionMetrics::new()),
        }
    }
    
    pub fn with_network(mut self, network: Arc<dyn DistributionNetwork>) -> Self {
        self.network = Some(network);
        self
    }
    
    /// Distribute a data column to appropriate peers
    pub async fn distribute_column(&self, column: DataColumn) -> Result<DistributionResult> {
        let start = Instant::now();
        
        self.metrics.total_distributions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Find target peers based on strategy
        let target_peers = self.select_target_peers(&column).await?;
        
        if target_peers.is_empty() {
            self.metrics.failed_distributions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            return Err(DASError::NetworkError(
                "No suitable peers found for distribution".to_string()
            ));
        }
        
        info!(
            "Distributing column {} to {} peers",
            column.index,
            target_peers.len()
        );
        
        // Distribute to selected peers
        let results = self.send_to_peers(&column, &target_peers).await;
        
        // Count successes and failures
        let successful_peers: Vec<[u8; 32]> = results
            .iter()
            .filter_map(|(peer, success)| if *success { Some(*peer) } else { None })
            .collect();
        
        let failed_peers: Vec<[u8; 32]> = results
            .iter()
            .filter_map(|(peer, success)| if !*success { Some(*peer) } else { None })
            .collect();
        
        // Update metrics
        if !successful_peers.is_empty() {
            self.metrics.successful_distributions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            self.metrics.failed_distributions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        let elapsed = start.elapsed();
        self.metrics.avg_distribution_time_ms.store(
            elapsed.as_millis() as u64,
            std::sync::atomic::Ordering::Relaxed,
        );
        
        // Update peer tracker
        self.update_peer_stats(&successful_peers, &failed_peers).await;
        
        Ok(DistributionResult {
            column_index: column.index,
            successful_peers,
            failed_peers,
            distribution_time: elapsed,
        })
    }
    
    /// Distribute multiple columns in parallel
    pub async fn distribute_columns(&self, columns: Vec<DataColumn>) -> Vec<DistributionResult> {
        let mut handles = Vec::new();
        
        for column in columns {
            let distributor = self.clone();
            let handle = tokio::spawn(async move {
                distributor.distribute_column(column).await
            });
            handles.push(handle);
        }
        
        let mut results = Vec::new();
        for handle in handles {
            if let Ok(Ok(result)) = handle.await {
                results.push(result);
            }
        }
        
        results
    }
    
    async fn select_target_peers(&self, column: &DataColumn) -> Result<Vec<[u8; 32]>> {
        match &self.strategy {
            DistributionStrategy::Broadcast => {
                self.select_broadcast_peers(column.index).await
            }
            DistributionStrategy::Minimal => {
                self.select_minimal_peers(column.index).await
            }
            DistributionStrategy::ReliabilityBased => {
                self.select_reliable_peers(column.index).await
            }
            DistributionStrategy::GeoDiverse => {
                self.select_geo_diverse_peers(column.index).await
            }
        }
    }
    
    async fn select_broadcast_peers(&self, column_index: u64) -> Result<Vec<[u8; 32]>> {
        if let Some(network) = &self.network {
            network.find_custody_peers(column_index).await
        } else {
            // Mock peers for testing
            Ok(vec![
                [1u8; 32],
                [2u8; 32],
                [3u8; 32],
            ])
        }
    }
    
    async fn select_minimal_peers(&self, column_index: u64) -> Result<Vec<[u8; 32]>> {
        let all_peers = self.select_broadcast_peers(column_index).await?;
        
        // Select minimum required peers (e.g., 3)
        Ok(all_peers.into_iter().take(3).collect())
    }
    
    async fn select_reliable_peers(&self, column_index: u64) -> Result<Vec<[u8; 32]>> {
        let all_peers = self.select_broadcast_peers(column_index).await?;
        let tracker = self.peer_tracker.read().await;
        
        // Sort by reliability score
        let mut peer_scores: Vec<([u8; 32], f64)> = all_peers
            .into_iter()
            .map(|peer| {
                let score = tracker.peers
                    .get(&peer)
                    .map(|p| p.info.reliability_score)
                    .unwrap_or(0.5);
                (peer, score)
            })
            .collect();
        
        peer_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        
        // Select top reliable peers
        Ok(peer_scores.into_iter()
            .take(5)
            .map(|(peer, _)| peer)
            .collect())
    }
    
    async fn select_geo_diverse_peers(&self, column_index: u64) -> Result<Vec<[u8; 32]>> {
        let all_peers = self.select_broadcast_peers(column_index).await?;
        let tracker = self.peer_tracker.read().await;
        
        // Group peers by location
        let mut location_groups: HashMap<String, Vec<[u8; 32]>> = HashMap::new();
        
        for peer in all_peers {
            let location = tracker.peers
                .get(&peer)
                .and_then(|p| p.info.location.clone())
                .unwrap_or_else(|| "unknown".to_string());
            
            location_groups.entry(location).or_insert_with(Vec::new).push(peer);
        }
        
        // Select one peer from each location
        let mut selected = Vec::new();
        for (_, peers) in location_groups.iter() {
            if let Some(peer) = peers.first() {
                selected.push(*peer);
            }
        }
        
        Ok(selected)
    }
    
    async fn send_to_peers(
        &self,
        column: &DataColumn,
        peers: &[[u8; 32]],
    ) -> Vec<([u8; 32], bool)> {
        let mut results = Vec::new();
        
        if let Some(network) = &self.network {
            // Send to real network
            let mut handles = Vec::new();
            
            for peer in peers {
                let network = network.clone();
                let column = column.clone();
                let peer = *peer;
                
                let handle = tokio::spawn(async move {
                    let result = tokio::time::timeout(
                        Duration::from_secs(5),
                        network.send_column(&peer, &column),
                    )
                    .await
                    .unwrap_or_else(|_| Err(DASError::NetworkError("Timeout".to_string())))
                    .is_ok();
                    
                    (peer, result)
                });
                
                handles.push(handle);
            }
            
            for handle in handles {
                if let Ok(result) = handle.await {
                    results.push(result);
                }
            }
        } else {
            // Mock distribution for testing
            for peer in peers {
                // Simulate 90% success rate
                let success = rand::random::<f64>() < 0.9;
                results.push((*peer, success));
            }
        }
        
        results
    }
    
    async fn update_peer_stats(
        &self,
        successful_peers: &[[u8; 32]],
        failed_peers: &[[u8; 32]],
    ) {
        let mut tracker = self.peer_tracker.write().await;
        
        for peer in successful_peers {
            if let Some(tracked) = tracker.peers.get_mut(peer) {
                tracked.successful_sends += 1;
                tracked.last_seen = Instant::now();
                
                // Update reliability score
                let total = tracked.successful_sends + tracked.failed_sends;
                tracked.info.reliability_score = tracked.successful_sends as f64 / total as f64;
            }
        }
        
        for peer in failed_peers {
            if let Some(tracked) = tracker.peers.get_mut(peer) {
                tracked.failed_sends += 1;
                
                // Update reliability score
                let total = tracked.successful_sends + tracked.failed_sends;
                tracked.info.reliability_score = tracked.successful_sends as f64 / total as f64;
            }
        }
    }
    
    /// Add a peer to tracking
    pub async fn add_peer(&self, info: PeerInfo) {
        let mut tracker = self.peer_tracker.write().await;
        
        // Update column custodians
        for column in &info.custody_columns {
            tracker.column_custodians
                .entry(*column)
                .or_insert_with(HashSet::new)
                .insert(info.peer_id);
        }
        
        // Add or update peer
        tracker.peers.insert(info.peer_id, TrackedPeer {
            info,
            last_seen: Instant::now(),
            successful_sends: 0,
            failed_sends: 0,
        });
    }
    
    /// Remove a peer from tracking
    pub async fn remove_peer(&self, peer_id: &[u8; 32]) {
        let mut tracker = self.peer_tracker.write().await;
        
        if let Some(peer) = tracker.peers.remove(peer_id) {
            // Remove from column custodians
            for column in peer.info.custody_columns {
                if let Some(custodians) = tracker.column_custodians.get_mut(&column) {
                    custodians.remove(peer_id);
                }
            }
        }
    }
    
    /// Get distribution metrics
    pub fn get_metrics(&self) -> DistributionMetricsSnapshot {
        DistributionMetricsSnapshot {
            total_distributions: self.metrics.total_distributions
                .load(std::sync::atomic::Ordering::Relaxed),
            successful_distributions: self.metrics.successful_distributions
                .load(std::sync::atomic::Ordering::Relaxed),
            failed_distributions: self.metrics.failed_distributions
                .load(std::sync::atomic::Ordering::Relaxed),
            avg_distribution_time_ms: self.metrics.avg_distribution_time_ms
                .load(std::sync::atomic::Ordering::Relaxed),
        }
    }
}

impl Clone for DataDistributor {
    fn clone(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            network: self.network.clone(),
            peer_tracker: self.peer_tracker.clone(),
            distribution_queue: self.distribution_queue.clone(),
            metrics: self.metrics.clone(),
        }
    }
}

impl PeerTracker {
    fn new() -> Self {
        Self {
            peers: HashMap::new(),
            column_custodians: HashMap::new(),
        }
    }
}

impl DistributionQueue {
    fn new(max_queue_size: usize) -> Self {
        Self {
            pending: Vec::new(),
            max_queue_size,
        }
    }
    
    fn add(&mut self, distribution: PendingDistribution) -> Result<()> {
        if self.pending.len() >= self.max_queue_size {
            return Err(DASError::NetworkError("Distribution queue full".to_string()));
        }
        
        self.pending.push(distribution);
        Ok(())
    }
    
    fn get_next(&mut self) -> Option<PendingDistribution> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }
    
    fn cleanup_expired(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.pending.retain(|dist| {
            now.duration_since(dist.created_at) < max_age
        });
    }
}

impl DistributionMetrics {
    fn new() -> Self {
        Self {
            total_distributions: std::sync::atomic::AtomicU64::new(0),
            successful_distributions: std::sync::atomic::AtomicU64::new(0),
            failed_distributions: std::sync::atomic::AtomicU64::new(0),
            avg_distribution_time_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DistributionResult {
    pub column_index: u64,
    pub successful_peers: Vec<[u8; 32]>,
    pub failed_peers: Vec<[u8; 32]>,
    pub distribution_time: Duration,
}

#[derive(Debug, Clone)]
pub struct DistributionMetricsSnapshot {
    pub total_distributions: u64,
    pub successful_distributions: u64,
    pub failed_distributions: u64,
    pub avg_distribution_time_ms: u64,
}

/// Batch distributor for efficient bulk distribution
pub struct BatchDistributor {
    distributor: Arc<DataDistributor>,
    batch_size: usize,
    batch_timeout: Duration,
}

impl BatchDistributor {
    pub fn new(distributor: DataDistributor, batch_size: usize) -> Self {
        Self {
            distributor: Arc::new(distributor),
            batch_size,
            batch_timeout: Duration::from_secs(5),
        }
    }
    
    /// Distribute columns in batches
    pub async fn distribute_batch(&self, columns: Vec<DataColumn>) -> Vec<DistributionResult> {
        let mut all_results = Vec::new();
        
        for batch in columns.chunks(self.batch_size) {
            let batch_results = self.distributor.distribute_columns(batch.to_vec()).await;
            all_results.extend(batch_results);
            
            // Small delay between batches to avoid overwhelming the network
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        
        all_results
    }
}