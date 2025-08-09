use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Transaction, Log};
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::HashMap;
use tokio::sync::{mpsc, broadcast};
use serde::{Serialize, Deserialize};
use serde_json::Value;

use crate::{FilterCriteria, FilterError, Result};

/// Subscription types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubscriptionType {
    NewHeads,
    NewPendingTransactions,
    Logs(FilterCriteria),
    Syncing,
}

/// Subscription
pub struct Subscription {
    pub id: U256,
    pub subscription_type: SubscriptionType,
    pub sender: mpsc::UnboundedSender<SubscriptionNotification>,
}

/// Subscription notification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SubscriptionNotification {
    NewHead(BlockHeader),
    NewPendingTransaction(H256),
    Log(Log),
    Syncing(SyncStatus),
}

/// Block header for subscription
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockHeader {
    pub hash: H256,
    pub parent_hash: H256,
    pub uncles_hash: H256,
    pub author: Address,
    pub state_root: H256,
    pub transactions_root: H256,
    pub receipts_root: H256,
    pub number: U256,
    pub gas_used: U256,
    pub gas_limit: U256,
    pub extra_data: Vec<u8>,
    pub logs_bloom: ethereum_types::Bloom,
    pub timestamp: u64,
    pub difficulty: U256,
    pub mix_hash: H256,
    pub nonce: u64,
}

impl From<&ethereum_core::Header> for BlockHeader {
    fn from(header: &ethereum_core::Header) -> Self {
        Self {
            hash: header.hash(),
            parent_hash: header.parent_hash,
            uncles_hash: header.uncles_hash,
            author: header.author,
            state_root: header.state_root,
            transactions_root: header.transactions_root,
            receipts_root: header.receipts_root,
            number: header.number,
            gas_used: header.gas_used,
            gas_limit: header.gas_limit,
            extra_data: header.extra_data.clone(),
            logs_bloom: header.bloom,
            timestamp: header.timestamp,
            difficulty: header.difficulty,
            mix_hash: header.mix_hash,
            nonce: header.nonce,
        }
    }
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    pub syncing: bool,
    pub starting_block: Option<U256>,
    pub current_block: Option<U256>,
    pub highest_block: Option<U256>,
}

/// Subscription manager
pub struct SubscriptionManager {
    subscriptions: Arc<RwLock<HashMap<U256, Subscription>>>,
    next_id: Arc<RwLock<U256>>,
    new_heads_broadcast: broadcast::Sender<Block>,
    new_pending_tx_broadcast: broadcast::Sender<Transaction>,
    new_logs_broadcast: broadcast::Sender<Vec<Log>>,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        let (new_heads_tx, _) = broadcast::channel(100);
        let (new_pending_tx_tx, _) = broadcast::channel(100);
        let (new_logs_tx, _) = broadcast::channel(100);
        
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(RwLock::new(U256::one())),
            new_heads_broadcast: new_heads_tx,
            new_pending_tx_broadcast: new_pending_tx_tx,
            new_logs_broadcast: new_logs_tx,
        }
    }
    
    /// Start the subscription manager
    pub async fn start(&self) {
        // Start notification handlers
        self.start_new_heads_handler();
        self.start_new_pending_tx_handler();
        self.start_new_logs_handler();
    }
    
    /// Subscribe to events
    pub async fn subscribe(&self, subscription_type: SubscriptionType) -> Result<Subscription> {
        let id = self.next_subscription_id().await;
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        let subscription = Subscription {
            id,
            subscription_type: subscription_type.clone(),
            sender: tx,
        };
        
        self.subscriptions.write().insert(id, subscription);
        
        // Return subscription with receiver
        Ok(Subscription {
            id,
            subscription_type,
            sender: rx.into(),
        })
    }
    
    /// Unsubscribe
    pub async fn unsubscribe(&self, subscription_id: U256) -> Result<bool> {
        Ok(self.subscriptions.write().remove(&subscription_id).is_some())
    }
    
    /// Notify new block
    pub async fn notify_new_block(&self, block: Block) {
        let _ = self.new_heads_broadcast.send(block);
    }
    
    /// Notify new pending transaction
    pub async fn notify_new_pending_transaction(&self, tx: Transaction) {
        let _ = self.new_pending_tx_broadcast.send(tx);
    }
    
    /// Notify new logs
    pub async fn notify_new_logs(&self, logs: Vec<Log>) {
        let _ = self.new_logs_broadcast.send(logs);
    }
    
    /// Start new heads notification handler
    fn start_new_heads_handler(&self) {
        let subscriptions = self.subscriptions.clone();
        let mut receiver = self.new_heads_broadcast.subscribe();
        
        tokio::spawn(async move {
            while let Ok(block) = receiver.recv().await {
                let subs = subscriptions.read();
                
                for sub in subs.values() {
                    if matches!(sub.subscription_type, SubscriptionType::NewHeads) {
                        let header = BlockHeader::from(&block.header);
                        let notification = SubscriptionNotification::NewHead(header);
                        
                        if let Err(e) = sub.sender.send(notification) {
                            tracing::warn!("Failed to send new head notification: {}", e);
                        }
                    }
                }
            }
        });
    }
    
    /// Start new pending transactions handler
    fn start_new_pending_tx_handler(&self) {
        let subscriptions = self.subscriptions.clone();
        let mut receiver = self.new_pending_tx_broadcast.subscribe();
        
        tokio::spawn(async move {
            while let Ok(tx) = receiver.recv().await {
                let subs = subscriptions.read();
                
                for sub in subs.values() {
                    if matches!(sub.subscription_type, SubscriptionType::NewPendingTransactions) {
                        let notification = SubscriptionNotification::NewPendingTransaction(tx.hash());
                        
                        if let Err(e) = sub.sender.send(notification) {
                            tracing::warn!("Failed to send pending tx notification: {}", e);
                        }
                    }
                }
            }
        });
    }
    
    /// Start new logs handler
    fn start_new_logs_handler(&self) {
        let subscriptions = self.subscriptions.clone();
        let mut receiver = self.new_logs_broadcast.subscribe();
        
        tokio::spawn(async move {
            while let Ok(logs) = receiver.recv().await {
                let subs = subscriptions.read();
                
                for sub in subs.values() {
                    if let SubscriptionType::Logs(ref criteria) = sub.subscription_type {
                        for log in &logs {
                            if Self::log_matches_criteria(log, criteria) {
                                let notification = SubscriptionNotification::Log(log.clone());
                                
                                if let Err(e) = sub.sender.send(notification) {
                                    tracing::warn!("Failed to send log notification: {}", e);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
    
    /// Check if log matches criteria
    fn log_matches_criteria(log: &Log, criteria: &FilterCriteria) -> bool {
        // Check address filter
        if let Some(ref addresses) = criteria.address {
            if !addresses.is_empty() && !addresses.contains(&log.address) {
                return false;
            }
        }
        
        // Check topics filter
        for (i, topic_filter) in criteria.topics.iter().enumerate() {
            if let Some(ref topics) = topic_filter {
                if !topics.is_empty() {
                    if i >= log.topics.len() || !topics.contains(&log.topics[i]) {
                        return false;
                    }
                }
            }
        }
        
        true
    }
    
    /// Get next subscription ID
    async fn next_subscription_id(&self) -> U256 {
        let mut id = self.next_id.write();
        let subscription_id = *id;
        *id = *id + U256::one();
        subscription_id
    }
    
    /// Get active subscriptions count
    pub fn active_subscriptions(&self) -> usize {
        self.subscriptions.read().len()
    }
    
    /// Clean up closed subscriptions
    pub async fn cleanup_closed_subscriptions(&self) {
        let mut subs = self.subscriptions.write();
        
        subs.retain(|id, sub| {
            if sub.sender.is_closed() {
                tracing::debug!("Removing closed subscription {}", id);
                false
            } else {
                true
            }
        });
    }
}

/// WebSocket subscription message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMessage {
    pub jsonrpc: String,
    pub method: String,
    pub params: SubscriptionParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionParams {
    pub subscription: U256,
    pub result: Value,
}

impl SubscriptionMessage {
    pub fn new(subscription_id: U256, notification: SubscriptionNotification) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: "eth_subscription".to_string(),
            params: SubscriptionParams {
                subscription: subscription_id,
                result: serde_json::to_value(notification).unwrap(),
            },
        }
    }
}