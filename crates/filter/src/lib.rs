use ethereum_types::{H256, U256, Address, Bloom};
use ethereum_core::{Block, Transaction, Receipt, Log};
use ethereum_storage::Database;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use tokio::sync::{RwLock, mpsc, broadcast};
use thiserror::Error;
use serde::{Serialize, Deserialize};

pub mod log_filter;
pub mod block_filter;
pub mod pending_tx_filter;
pub mod subscription;

pub use log_filter::{LogFilter, LogFilterBuilder};
pub use block_filter::BlockFilter;
pub use pending_tx_filter::PendingTransactionFilter;
pub use subscription::{Subscription, SubscriptionManager, SubscriptionType};

#[derive(Debug, Error)]
pub enum FilterError {
    #[error("Filter not found")]
    FilterNotFound,
    
    #[error("Invalid filter criteria")]
    InvalidCriteria,
    
    #[error("Storage error: {0}")]
    StorageError(#[from] ethereum_storage::StorageError),
    
    #[error("Subscription error: {0}")]
    SubscriptionError(String),
    
    #[error("Filter expired")]
    FilterExpired,
}

pub type Result<T> = std::result::Result<T, FilterError>;

/// Filter ID type
pub type FilterId = U256;

/// Filter criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCriteria {
    pub from_block: Option<BlockNumber>,
    pub to_block: Option<BlockNumber>,
    pub address: Option<Vec<Address>>,
    pub topics: Vec<Option<Vec<H256>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BlockNumber {
    Number(U256),
    Latest,
    Earliest,
    Pending,
}

/// Main filter system
pub struct FilterSystem<D: Database> {
    db: Arc<D>,
    filters: Arc<RwLock<HashMap<FilterId, Filter>>>,
    subscriptions: Arc<SubscriptionManager>,
    next_filter_id: Arc<RwLock<U256>>,
    poll_interval: std::time::Duration,
}

/// Filter types
#[derive(Debug, Clone)]
enum Filter {
    Log(LogFilter),
    Block(BlockFilter),
    PendingTransaction(PendingTransactionFilter),
}

impl<D: Database + 'static> FilterSystem<D> {
    pub fn new(db: Arc<D>) -> Self {
        let subscriptions = Arc::new(SubscriptionManager::new());
        
        Self {
            db,
            filters: Arc::new(RwLock::new(HashMap::new())),
            subscriptions,
            next_filter_id: Arc::new(RwLock::new(U256::one())),
            poll_interval: std::time::Duration::from_secs(1),
        }
    }
    
    /// Start the filter system
    pub async fn start(&self) {
        // Start subscription manager
        self.subscriptions.start().await;
        
        // Start filter polling
        self.start_filter_polling().await;
    }
    
    /// Create a new log filter
    pub async fn new_log_filter(&self, criteria: FilterCriteria) -> Result<FilterId> {
        let filter = LogFilter::new(criteria, self.db.clone());
        let filter_id = self.next_filter_id().await;
        
        self.filters.write().await.insert(
            filter_id,
            Filter::Log(filter),
        );
        
        Ok(filter_id)
    }
    
    /// Create a new block filter
    pub async fn new_block_filter(&self) -> Result<FilterId> {
        let filter = BlockFilter::new(self.db.clone());
        let filter_id = self.next_filter_id().await;
        
        self.filters.write().await.insert(
            filter_id,
            Filter::Block(filter),
        );
        
        Ok(filter_id)
    }
    
    /// Create a new pending transaction filter
    pub async fn new_pending_transaction_filter(&self) -> Result<FilterId> {
        let filter = PendingTransactionFilter::new();
        let filter_id = self.next_filter_id().await;
        
        self.filters.write().await.insert(
            filter_id,
            Filter::PendingTransaction(filter),
        );
        
        Ok(filter_id)
    }
    
    /// Get filter changes since last poll
    pub async fn get_filter_changes(&self, filter_id: FilterId) -> Result<FilterChanges> {
        let mut filters = self.filters.write().await;
        
        let filter = filters.get_mut(&filter_id)
            .ok_or(FilterError::FilterNotFound)?;
        
        match filter {
            Filter::Log(log_filter) => {
                let logs = log_filter.get_changes().await?;
                Ok(FilterChanges::Logs(logs))
            }
            Filter::Block(block_filter) => {
                let hashes = block_filter.get_changes().await?;
                Ok(FilterChanges::Hashes(hashes))
            }
            Filter::PendingTransaction(tx_filter) => {
                let hashes = tx_filter.get_changes().await?;
                Ok(FilterChanges::Hashes(hashes))
            }
        }
    }
    
    /// Get all logs matching filter
    pub async fn get_filter_logs(&self, filter_id: FilterId) -> Result<Vec<Log>> {
        let filters = self.filters.read().await;
        
        let filter = filters.get(&filter_id)
            .ok_or(FilterError::FilterNotFound)?;
        
        match filter {
            Filter::Log(log_filter) => {
                log_filter.get_all_logs().await
            }
            _ => Err(FilterError::InvalidCriteria),
        }
    }
    
    /// Get logs matching criteria
    pub async fn get_logs(&self, criteria: FilterCriteria) -> Result<Vec<Log>> {
        let filter = LogFilter::new(criteria, self.db.clone());
        filter.get_all_logs().await
    }
    
    /// Uninstall a filter
    pub async fn uninstall_filter(&self, filter_id: FilterId) -> Result<bool> {
        Ok(self.filters.write().await.remove(&filter_id).is_some())
    }
    
    /// Subscribe to events
    pub async fn subscribe(&self, subscription_type: SubscriptionType) -> Result<Subscription> {
        self.subscriptions.subscribe(subscription_type).await
            .map_err(|e| FilterError::SubscriptionError(e.to_string()))
    }
    
    /// Unsubscribe from events
    pub async fn unsubscribe(&self, subscription_id: U256) -> Result<bool> {
        self.subscriptions.unsubscribe(subscription_id).await
            .map_err(|e| FilterError::SubscriptionError(e.to_string()))
    }
    
    /// Notify new block
    pub async fn notify_new_block(&self, block: Block) {
        // Update block filters
        let filters = self.filters.read().await;
        for filter in filters.values() {
            if let Filter::Block(block_filter) = filter {
                block_filter.add_block(block.header.hash()).await;
            }
        }
        
        // Notify subscriptions
        self.subscriptions.notify_new_block(block).await;
    }
    
    /// Notify new pending transaction
    pub async fn notify_new_pending_transaction(&self, tx: Transaction) {
        // Update pending transaction filters
        let filters = self.filters.read().await;
        for filter in filters.values() {
            if let Filter::PendingTransaction(tx_filter) = filter {
                tx_filter.add_transaction(tx.hash()).await;
            }
        }
        
        // Notify subscriptions
        self.subscriptions.notify_new_pending_transaction(tx).await;
    }
    
    /// Notify new logs
    pub async fn notify_new_logs(&self, logs: Vec<Log>) {
        // Update log filters
        let filters = self.filters.read().await;
        for filter in filters.values() {
            if let Filter::Log(log_filter) = filter {
                for log in &logs {
                    if log_filter.matches(log) {
                        log_filter.add_log(log.clone()).await;
                    }
                }
            }
        }
        
        // Notify subscriptions
        self.subscriptions.notify_new_logs(logs).await;
    }
    
    /// Start filter polling
    async fn start_filter_polling(&self) {
        let filters = self.filters.clone();
        let db = self.db.clone();
        let interval = self.poll_interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                // Poll for changes
                let filters_guard = filters.read().await;
                for (id, filter) in filters_guard.iter() {
                    match filter {
                        Filter::Log(log_filter) => {
                            if let Err(e) = log_filter.poll_for_changes().await {
                                tracing::warn!("Failed to poll log filter {}: {}", id, e);
                            }
                        }
                        Filter::Block(block_filter) => {
                            if let Err(e) = block_filter.poll_for_changes().await {
                                tracing::warn!("Failed to poll block filter {}: {}", id, e);
                            }
                        }
                        _ => {}
                    }
                }
            }
        });
    }
    
    /// Get next filter ID
    async fn next_filter_id(&self) -> FilterId {
        let mut id = self.next_filter_id.write().await;
        let filter_id = *id;
        *id = *id + U256::one();
        filter_id
    }
    
    /// Clean up expired filters
    pub async fn cleanup_expired_filters(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let mut filters = self.filters.write().await;
        
        // Remove filters older than 5 minutes
        const FILTER_TIMEOUT: u64 = 300;
        
        filters.retain(|_, filter| {
            match filter {
                Filter::Log(f) => now - f.created_at() < FILTER_TIMEOUT,
                Filter::Block(f) => now - f.created_at() < FILTER_TIMEOUT,
                Filter::PendingTransaction(f) => now - f.created_at() < FILTER_TIMEOUT,
            }
        });
    }
}

/// Filter changes result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterChanges {
    Logs(Vec<Log>),
    Hashes(Vec<H256>),
}

/// Bloom filter utilities
pub struct BloomFilter;

impl BloomFilter {
    /// Check if log matches bloom filter
    pub fn matches(bloom: &Bloom, log: &Log) -> bool {
        // Check address
        if !Self::contains_address(bloom, &log.address) {
            return false;
        }
        
        // Check topics
        for topic in &log.topics {
            if !Self::contains_topic(bloom, topic) {
                return false;
            }
        }
        
        true
    }
    
    /// Check if bloom contains address
    pub fn contains_address(bloom: &Bloom, address: &Address) -> bool {
        let hash = ethereum_crypto::keccak256(address.as_bytes());
        Self::contains_hash(bloom, &hash)
    }
    
    /// Check if bloom contains topic
    pub fn contains_topic(bloom: &Bloom, topic: &H256) -> bool {
        Self::contains_hash(bloom, &topic.0)
    }
    
    /// Check if bloom contains hash
    fn contains_hash(bloom: &Bloom, hash: &[u8; 32]) -> bool {
        for i in 0..3 {
            let bit_index = (hash[i * 2] as usize) | ((hash[i * 2 + 1] as usize) << 8);
            let byte_index = bit_index / 8;
            let bit_mask = 1u8 << (bit_index % 8);
            
            if byte_index < bloom.0.len() && (bloom.0[byte_index] & bit_mask) == 0 {
                return false;
            }
        }
        
        true
    }
    
    /// Add to bloom filter
    pub fn add_to_bloom(bloom: &mut Bloom, data: &[u8]) {
        let hash = ethereum_crypto::keccak256(data);
        
        for i in 0..3 {
            let bit_index = (hash[i * 2] as usize) | ((hash[i * 2 + 1] as usize) << 8);
            let byte_index = bit_index / 8;
            let bit_mask = 1u8 << (bit_index % 8);
            
            if byte_index < bloom.0.len() {
                bloom.0[byte_index] |= bit_mask;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_filter() {
        let mut bloom = Bloom::default();
        let address = Address::from([1u8; 20]);
        
        BloomFilter::add_to_bloom(&mut bloom, address.as_bytes());
        assert!(BloomFilter::contains_address(&bloom, &address));
        
        let other_address = Address::from([2u8; 20]);
        // May or may not contain due to false positives
    }
}