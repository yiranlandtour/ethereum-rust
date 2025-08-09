use ethereum_types::{H256, U256, Address, Bloom};
use ethereum_core::{Log, Receipt, Block};
use ethereum_storage::Database;
use std::sync::Arc;
use parking_lot::RwLock;
use std::collections::VecDeque;

use crate::{Result, FilterError, FilterCriteria, BlockNumber, BloomFilter};

/// Log filter for filtering event logs
pub struct LogFilter<D: Database> {
    criteria: FilterCriteria,
    db: Arc<D>,
    pending_logs: Arc<RwLock<VecDeque<Log>>>,
    last_poll_block: Arc<RwLock<U256>>,
    created_at: u64,
}

impl<D: Database> LogFilter<D> {
    pub fn new(criteria: FilterCriteria, db: Arc<D>) -> Self {
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        Self {
            criteria,
            db,
            pending_logs: Arc::new(RwLock::new(VecDeque::new())),
            last_poll_block: Arc::new(RwLock::new(U256::zero())),
            created_at,
        }
    }
    
    /// Get filter creation time
    pub fn created_at(&self) -> u64 {
        self.created_at
    }
    
    /// Check if a log matches the filter criteria
    pub fn matches(&self, log: &Log) -> bool {
        // Check address filter
        if let Some(ref addresses) = self.criteria.address {
            if !addresses.is_empty() && !addresses.contains(&log.address) {
                return false;
            }
        }
        
        // Check topics filter
        for (i, topic_filter) in self.criteria.topics.iter().enumerate() {
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
    
    /// Add a log to pending queue
    pub async fn add_log(&self, log: Log) {
        self.pending_logs.write().push_back(log);
    }
    
    /// Get changes since last poll
    pub async fn get_changes(&self) -> Result<Vec<Log>> {
        let mut pending = self.pending_logs.write();
        let logs: Vec<Log> = pending.drain(..).collect();
        Ok(logs)
    }
    
    /// Get all logs matching the filter
    pub async fn get_all_logs(&self) -> Result<Vec<Log>> {
        let from_block = self.resolve_block_number(&self.criteria.from_block).await?;
        let to_block = self.resolve_block_number(&self.criteria.to_block).await?;
        
        let mut all_logs = Vec::new();
        
        // Iterate through blocks
        for block_num in from_block.as_u64()..=to_block.as_u64() {
            let block = self.get_block(U256::from(block_num)).await?;
            
            // Quick bloom filter check
            if let Some(ref addresses) = self.criteria.address {
                let mut matches_bloom = false;
                for addr in addresses {
                    if BloomFilter::contains_address(&block.header.bloom, addr) {
                        matches_bloom = true;
                        break;
                    }
                }
                
                if !matches_bloom {
                    continue; // Skip this block
                }
            }
            
            // Get receipts for block
            let receipts = self.get_receipts(&block.header.hash()).await?;
            
            // Extract logs from receipts
            for (tx_index, receipt) in receipts.iter().enumerate() {
                for (log_index, log) in receipt.logs.iter().enumerate() {
                    if self.matches(log) {
                        let mut log_with_position = log.clone();
                        log_with_position.block_hash = Some(block.header.hash());
                        log_with_position.block_number = Some(block.header.number);
                        log_with_position.transaction_hash = Some(
                            block.body.transactions[tx_index].hash()
                        );
                        log_with_position.transaction_index = Some(U256::from(tx_index));
                        log_with_position.log_index = Some(U256::from(log_index));
                        
                        all_logs.push(log_with_position);
                    }
                }
            }
        }
        
        Ok(all_logs)
    }
    
    /// Poll for changes in new blocks
    pub async fn poll_for_changes(&self) -> Result<()> {
        let current_block = self.get_latest_block_number().await?;
        let mut last_poll = self.last_poll_block.write();
        
        if current_block <= *last_poll {
            return Ok(()); // No new blocks
        }
        
        // Process new blocks
        for block_num in (last_poll.as_u64() + 1)..=current_block.as_u64() {
            let block = self.get_block(U256::from(block_num)).await?;
            let receipts = self.get_receipts(&block.header.hash()).await?;
            
            for (tx_index, receipt) in receipts.iter().enumerate() {
                for (log_index, log) in receipt.logs.iter().enumerate() {
                    if self.matches(log) {
                        let mut log_with_position = log.clone();
                        log_with_position.block_hash = Some(block.header.hash());
                        log_with_position.block_number = Some(block.header.number);
                        log_with_position.transaction_hash = Some(
                            block.body.transactions[tx_index].hash()
                        );
                        log_with_position.transaction_index = Some(U256::from(tx_index));
                        log_with_position.log_index = Some(U256::from(log_index));
                        
                        self.pending_logs.write().push_back(log_with_position);
                    }
                }
            }
        }
        
        *last_poll = current_block;
        Ok(())
    }
    
    /// Resolve block number
    async fn resolve_block_number(&self, block_num: &Option<BlockNumber>) -> Result<U256> {
        match block_num {
            Some(BlockNumber::Number(n)) => Ok(*n),
            Some(BlockNumber::Latest) | None => self.get_latest_block_number().await,
            Some(BlockNumber::Earliest) => Ok(U256::zero()),
            Some(BlockNumber::Pending) => self.get_latest_block_number().await,
        }
    }
    
    /// Get block by number
    async fn get_block(&self, block_number: U256) -> Result<Block> {
        let key = format!("block:number:{}", block_number);
        let block_hash = self.db.get(key.as_bytes())?
            .ok_or(FilterError::InvalidCriteria)?;
        
        let block_key = format!("block:{}", hex::encode(block_hash));
        let block_data = self.db.get(block_key.as_bytes())?
            .ok_or(FilterError::InvalidCriteria)?;
        
        bincode::deserialize(&block_data)
            .map_err(|_| FilterError::InvalidCriteria)
    }
    
    /// Get receipts for a block
    async fn get_receipts(&self, block_hash: &H256) -> Result<Vec<Receipt>> {
        let key = format!("receipts:{}", hex::encode(block_hash));
        
        match self.db.get(key.as_bytes())? {
            Some(data) => {
                bincode::deserialize(&data)
                    .map_err(|_| FilterError::InvalidCriteria)
            }
            None => Ok(Vec::new()),
        }
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
}

/// Log filter builder
pub struct LogFilterBuilder {
    from_block: Option<BlockNumber>,
    to_block: Option<BlockNumber>,
    addresses: Vec<Address>,
    topics: Vec<Option<Vec<H256>>>,
}

impl LogFilterBuilder {
    pub fn new() -> Self {
        Self {
            from_block: None,
            to_block: None,
            addresses: Vec::new(),
            topics: vec![None, None, None, None],
        }
    }
    
    pub fn from_block(mut self, block: BlockNumber) -> Self {
        self.from_block = Some(block);
        self
    }
    
    pub fn to_block(mut self, block: BlockNumber) -> Self {
        self.to_block = Some(block);
        self
    }
    
    pub fn address(mut self, address: Address) -> Self {
        self.addresses.push(address);
        self
    }
    
    pub fn addresses(mut self, addresses: Vec<Address>) -> Self {
        self.addresses = addresses;
        self
    }
    
    pub fn topic(mut self, index: usize, topic: H256) -> Self {
        if index < 4 {
            if self.topics[index].is_none() {
                self.topics[index] = Some(Vec::new());
            }
            self.topics[index].as_mut().unwrap().push(topic);
        }
        self
    }
    
    pub fn topics(mut self, index: usize, topics: Vec<H256>) -> Self {
        if index < 4 {
            self.topics[index] = Some(topics);
        }
        self
    }
    
    pub fn build(self) -> FilterCriteria {
        FilterCriteria {
            from_block: self.from_block,
            to_block: self.to_block,
            address: if self.addresses.is_empty() {
                None
            } else {
                Some(self.addresses)
            },
            topics: self.topics,
        }
    }
}