use ethereum_types::{H256, U256, Address};
use ethereum_core::Transaction;
use parking_lot::RwLock;
use priority_queue::PriorityQueue;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::cmp::Ordering;
use thiserror::Error;
use tokio::sync::broadcast;
use tokio::time::{self, Duration};

#[derive(Debug, Error)]
pub enum TxPoolError {
    #[error("Transaction already exists")]
    AlreadyExists,
    
    #[error("Transaction nonce too low")]
    NonceTooLow,
    
    #[error("Transaction gas price too low")]
    GasPriceTooLow,
    
    #[error("Transaction pool is full")]
    PoolFull,
    
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(String),
    
    #[error("Account balance insufficient")]
    InsufficientBalance,
    
    #[error("Gas limit exceeded")]
    GasLimitExceeded,
}

pub type Result<T> = std::result::Result<T, TxPoolError>;

#[derive(Debug, Clone)]
pub struct TxPoolConfig {
    pub max_size: usize,
    pub max_account_slots: usize,
    pub price_limit: U256,
    pub price_bump: u64, // Percentage
    pub account_queue: usize,
    pub global_queue: usize,
    pub lifetime: Duration,
}

impl Default for TxPoolConfig {
    fn default() -> Self {
        Self {
            max_size: 4096,
            max_account_slots: 16,
            price_limit: U256::from(1_000_000_000), // 1 gwei
            price_bump: 10, // 10% price bump required for replacement
            account_queue: 64,
            global_queue: 1024,
            lifetime: Duration::from_secs(3 * 60 * 60), // 3 hours
        }
    }
}

#[derive(Debug, Clone)]
pub struct PooledTransaction {
    pub tx: Transaction,
    pub hash: H256,
    pub gas_price: U256,
    pub from: Address,
    pub timestamp: std::time::Instant,
}

impl PooledTransaction {
    pub fn new(tx: Transaction) -> Self {
        let hash = tx.hash();
        let gas_price = tx.gas_price();
        let from = tx.from();
        
        Self {
            tx,
            hash,
            gas_price,
            from,
            timestamp: std::time::Instant::now(),
        }
    }
    
    pub fn effective_gas_price(&self) -> U256 {
        self.gas_price
    }
}

#[derive(Clone)]
struct TxPriority(U256);

impl Ord for TxPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl PartialOrd for TxPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for TxPriority {}

impl PartialEq for TxPriority {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

pub struct TransactionPool {
    config: TxPoolConfig,
    pending: Arc<RwLock<HashMap<Address, VecDeque<PooledTransaction>>>>,
    queued: Arc<RwLock<HashMap<Address, VecDeque<PooledTransaction>>>>,
    all: Arc<RwLock<HashMap<H256, PooledTransaction>>>,
    price_heap: Arc<RwLock<PriorityQueue<H256, TxPriority>>>,
    events_tx: broadcast::Sender<TxPoolEvent>,
}

#[derive(Debug, Clone)]
pub enum TxPoolEvent {
    NewTransaction(H256),
    Removed(H256),
    Promoted(H256),
}

impl TransactionPool {
    pub fn new(config: TxPoolConfig) -> Self {
        let (events_tx, _) = broadcast::channel(1000); // Buffer size of 1000 events
        
        Self {
            config,
            pending: Arc::new(RwLock::new(HashMap::new())),
            queued: Arc::new(RwLock::new(HashMap::new())),
            all: Arc::new(RwLock::new(HashMap::new())),
            price_heap: Arc::new(RwLock::new(PriorityQueue::new())),
            events_tx,
        }
    }
    
    pub fn add_transaction(&self, tx: Transaction) -> Result<H256> {
        let pooled = PooledTransaction::new(tx);
        let hash = pooled.hash;
        
        // Check if transaction already exists
        if self.all.read().contains_key(&hash) {
            return Err(TxPoolError::AlreadyExists);
        }
        
        // Validate gas price
        if pooled.gas_price < self.config.price_limit {
            return Err(TxPoolError::GasPriceTooLow);
        }
        
        // Check pool size
        if self.all.read().len() >= self.config.max_size {
            // Try to evict lower priced transaction
            if !self.evict_transaction(&pooled)? {
                return Err(TxPoolError::PoolFull);
            }
        }
        
        // Add to pool
        self.add_to_pool(pooled)?;
        
        // Send event
        let _ = self.events_tx.send(TxPoolEvent::NewTransaction(hash));
        
        Ok(hash)
    }
    
    fn add_to_pool(&self, tx: PooledTransaction) -> Result<()> {
        let from = tx.from;
        let nonce = tx.tx.nonce();
        let hash = tx.hash;
        let gas_price = tx.gas_price;
        
        // Get expected nonce for account
        let expected_nonce = self.get_next_nonce(&from);
        
        // Add to all transactions
        self.all.write().insert(hash, tx.clone());
        
        // Add to price heap
        self.price_heap.write().push(hash, TxPriority(gas_price));
        
        if nonce == expected_nonce {
            // Add to pending
            self.pending.write()
                .entry(from)
                .or_insert_with(VecDeque::new)
                .push_back(tx);
            
            // Try to promote queued transactions
            self.promote_queued(&from);
        } else if nonce > expected_nonce {
            // Add to queued
            self.queued.write()
                .entry(from)
                .or_insert_with(VecDeque::new)
                .push_back(tx);
        } else {
            // Nonce too low
            self.all.write().remove(&hash);
            self.price_heap.write().remove(&hash);
            return Err(TxPoolError::NonceTooLow);
        }
        
        Ok(())
    }
    
    fn evict_transaction(&self, new_tx: &PooledTransaction) -> Result<bool> {
        let mut heap = self.price_heap.write();
        
        // Find transaction with lowest gas price
        if let Some((hash, priority)) = heap.peek() {
            if priority.0 < new_tx.gas_price {
                let hash = *hash;
                heap.remove(&hash);
                
                // Remove from pool
                if let Some(old_tx) = self.all.write().remove(&hash) {
                    self.remove_from_lists(&old_tx);
                    let _ = self.events_tx.send(TxPoolEvent::Removed(hash));
                    return Ok(true);
                }
            }
        }
        
        Ok(false)
    }
    
    fn remove_from_lists(&self, tx: &PooledTransaction) {
        let from = tx.from;
        let hash = tx.hash;
        
        // Remove from pending
        if let Some(txs) = self.pending.write().get_mut(&from) {
            txs.retain(|t| t.hash != hash);
        }
        
        // Remove from queued
        if let Some(txs) = self.queued.write().get_mut(&from) {
            txs.retain(|t| t.hash != hash);
        }
    }
    
    fn promote_queued(&self, address: &Address) {
        let expected_nonce = self.get_next_nonce(address);
        
        let mut queued = self.queued.write();
        if let Some(txs) = queued.get_mut(address) {
            let mut promoted = Vec::new();
            
            // Find transactions that can be promoted
            txs.retain(|tx| {
                if tx.tx.nonce() == expected_nonce + U256::from(promoted.len()) {
                    promoted.push(tx.clone());
                    false
                } else {
                    true
                }
            });
            
            // Add promoted transactions to pending
            if !promoted.is_empty() {
                let mut pending = self.pending.write();
                let pending_txs = pending.entry(*address).or_insert_with(VecDeque::new);
                for tx in promoted {
                    let hash = tx.hash;
                    pending_txs.push_back(tx);
                    let _ = self.events_tx.send(TxPoolEvent::Promoted(hash));
                }
            }
        }
    }
    
    fn get_next_nonce(&self, address: &Address) -> U256 {
        // This should query the blockchain state
        // For now, return the next nonce based on pending transactions
        if let Some(txs) = self.pending.read().get(address) {
            if let Some(last_tx) = txs.back() {
                return last_tx.tx.nonce() + U256::one();
            }
        }
        U256::zero()
    }
    
    pub fn get_transaction(&self, hash: &H256) -> Option<PooledTransaction> {
        self.all.read().get(hash).cloned()
    }
    
    pub fn remove_transaction(&self, hash: &H256) -> Option<PooledTransaction> {
        if let Some(tx) = self.all.write().remove(hash) {
            self.price_heap.write().remove(hash);
            self.remove_from_lists(&tx);
            let _ = self.events_tx.send(TxPoolEvent::Removed(*hash));
            Some(tx)
        } else {
            None
        }
    }
    
    pub fn get_pending(&self) -> Vec<PooledTransaction> {
        let mut result = Vec::new();
        for txs in self.pending.read().values() {
            result.extend(txs.iter().cloned());
        }
        result
    }
    
    pub fn get_queued(&self) -> Vec<PooledTransaction> {
        let mut result = Vec::new();
        for txs in self.queued.read().values() {
            result.extend(txs.iter().cloned());
        }
        result
    }
    
    pub fn get_pending_by_address(&self, address: &Address) -> Vec<PooledTransaction> {
        self.pending.read()
            .get(address)
            .map(|txs| txs.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    pub fn get_queued_by_address(&self, address: &Address) -> Vec<PooledTransaction> {
        self.queued.read()
            .get(address)
            .map(|txs| txs.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    pub fn pending_count(&self) -> usize {
        self.pending.read()
            .values()
            .map(|txs| txs.len())
            .sum()
    }
    
    pub fn queued_count(&self) -> usize {
        self.queued.read()
            .values()
            .map(|txs| txs.len())
            .sum()
    }
    
    pub fn total_count(&self) -> usize {
        self.all.read().len()
    }
    
    pub fn clear(&self) {
        self.pending.write().clear();
        self.queued.write().clear();
        self.all.write().clear();
        self.price_heap.write().clear();
    }
    
    pub async fn run_maintenance(&self) {
        let mut interval = time::interval(Duration::from_secs(60));
        
        loop {
            interval.tick().await;
            
            // Remove expired transactions
            let now = std::time::Instant::now();
            let mut expired = Vec::new();
            
            for (hash, tx) in self.all.read().iter() {
                if now.duration_since(tx.timestamp) > self.config.lifetime {
                    expired.push(*hash);
                }
            }
            
            for hash in expired {
                self.remove_transaction(&hash);
            }
            
            tracing::debug!(
                "Transaction pool maintenance: {} pending, {} queued, {} total",
                self.pending_count(),
                self.queued_count(),
                self.total_count()
            );
        }
    }
    
    pub fn subscribe(&self) -> broadcast::Receiver<TxPoolEvent> {
        self.events_tx.subscribe()
    }
    
    pub fn get_transactions_for_block(&self, gas_limit: U256) -> Vec<PooledTransaction> {
        let mut result = Vec::new();
        let mut total_gas = U256::zero();
        
        // Get transactions sorted by gas price
        let mut txs_by_price: Vec<_> = self.pending.read()
            .values()
            .flat_map(|txs| txs.iter().cloned())
            .collect();
        
        txs_by_price.sort_by(|a, b| b.gas_price.cmp(&a.gas_price));
        
        for tx in txs_by_price {
            let gas = tx.tx.gas_limit();
            if total_gas + gas <= gas_limit {
                total_gas += gas;
                result.push(tx);
            } else {
                break;
            }
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_transaction_pool_basic() {
        let pool = TransactionPool::new(TxPoolConfig::default());
        
        // Create a test transaction
        // This would need proper transaction creation
        
        assert_eq!(pool.total_count(), 0);
    }
    
    #[test]
    fn test_transaction_priority() {
        let p1 = TxPriority(U256::from(100));
        let p2 = TxPriority(U256::from(200));
        
        assert!(p1 < p2);
        assert!(p2 > p1);
    }
}