use ethereum_types::{H256, U256};
use ethereum_storage::Database;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::VecDeque;

use crate::{Result, FilterError};

/// Block filter for tracking new blocks
pub struct BlockFilter<D: Database> {
    db: Arc<D>,
    pending_blocks: Arc<RwLock<VecDeque<H256>>>,
    last_poll_block: Arc<RwLock<U256>>,
    created_at: u64,
}

impl<D: Database> BlockFilter<D> {
    pub fn new(db: Arc<D>) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            db,
            pending_blocks: Arc::new(RwLock::new(VecDeque::new())),
            last_poll_block: Arc::new(RwLock::new(U256::zero())),
            created_at,
        }
    }
    
    /// Get filter creation time
    pub fn created_at(&self) -> u64 {
        self.created_at
    }
    
    /// Add a new block hash
    pub async fn add_block(&self, block_hash: H256) {
        self.pending_blocks.write().push_back(block_hash);
    }
    
    /// Get changes since last poll
    pub async fn get_changes(&self) -> Result<Vec<H256>> {
        let mut pending = self.pending_blocks.write();
        let hashes: Vec<H256> = pending.drain(..).collect();
        Ok(hashes)
    }
    
    /// Poll for new blocks
    pub async fn poll_for_changes(&self) -> Result<()> {
        let current_block = self.get_latest_block_number().await?;
        let mut last_poll = self.last_poll_block.write();
        
        if current_block <= *last_poll {
            return Ok(()); // No new blocks
        }
        
        // Get new block hashes
        for block_num in (last_poll.as_u64() + 1)..=current_block.as_u64() {
            let hash = self.get_block_hash(U256::from(block_num)).await?;
            self.pending_blocks.write().push_back(hash);
        }
        
        *last_poll = current_block;
        Ok(())
    }
    
    /// Get latest block number
    async fn get_latest_block_number(&self) -> Result<U256> {
        let key = b"latest_block";
        
        match self.db.get(key)? {
            Some(data) => {
                let bytes: [u8; 32] = data.try_into()
                    .map_err(|_| FilterError::InvalidCriteria)?;
                Ok(U256::from_big_endian(&bytes))
            }
            None => Ok(U256::zero()),
        }
    }
    
    /// Get block hash by number
    async fn get_block_hash(&self, block_number: U256) -> Result<H256> {
        let key = format!("block:number:{}", block_number);
        
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                let bytes: [u8; 32] = data.try_into()
                    .map_err(|_| FilterError::InvalidCriteria)?;
                Ok(H256::from(bytes))
            }
            None => Err(FilterError::InvalidCriteria),
        }
    }
}