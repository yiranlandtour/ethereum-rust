use ethereum_types::H256;
use parking_lot::RwLock;
use std::collections::VecDeque;

use crate::Result;

/// Pending transaction filter for tracking new pending transactions
pub struct PendingTransactionFilter {
    pending_transactions: RwLock<VecDeque<H256>>,
    created_at: u64,
}

impl PendingTransactionFilter {
    pub fn new() -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            pending_transactions: RwLock::new(VecDeque::new()),
            created_at,
        }
    }
    
    /// Get filter creation time
    pub fn created_at(&self) -> u64 {
        self.created_at
    }
    
    /// Add a new pending transaction hash
    pub async fn add_transaction(&self, tx_hash: H256) {
        self.pending_transactions.write().push_back(tx_hash);
    }
    
    /// Get changes since last poll
    pub async fn get_changes(&self) -> Result<Vec<H256>> {
        let mut pending = self.pending_transactions.write();
        let hashes: Vec<H256> = pending.drain(..).collect();
        Ok(hashes)
    }
}