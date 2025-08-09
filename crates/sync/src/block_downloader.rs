use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
use ethereum_storage::Database;
use ethereum_network::peer::PeerManager;
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};
use parking_lot::RwLock;

use crate::{Result, SyncError, SyncConfig};

pub struct BlockDownloader<D: Database> {
    db: Arc<D>,
    peer_manager: Arc<PeerManager>,
    config: SyncConfig,
    download_queue: Arc<RwLock<VecDeque<U256>>>,
    downloading: Arc<RwLock<HashMap<U256, DownloadTask>>>,
    downloaded: Arc<RwLock<HashMap<U256, Block>>>,
}

#[derive(Debug, Clone)]
struct DownloadTask {
    block_number: U256,
    peer_id: H256,
    attempts: usize,
    started_at: std::time::Instant,
}

impl<D: Database + 'static> BlockDownloader<D> {
    pub fn new(
        db: Arc<D>,
        peer_manager: Arc<PeerManager>,
        config: SyncConfig,
    ) -> Self {
        Self {
            db,
            peer_manager,
            config,
            download_queue: Arc::new(RwLock::new(VecDeque::new())),
            downloading: Arc::new(RwLock::new(HashMap::new())),
            downloaded: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    pub async fn download_next_batch(&self) -> Result<Vec<Block>> {
        // Get current chain head
        let local_head = self.get_local_head().await?;
        
        // Get best peer's head
        let peers = self.peer_manager.get_all_peers().await;
        if peers.is_empty() {
            return Err(SyncError::NoPeers);
        }
        
        // Find highest block among peers
        let remote_head = U256::from(1000); // Mock value, would get from peer
        
        if local_head >= remote_head {
            // Already synced
            return Ok(vec![]);
        }
        
        // Queue blocks for download
        self.queue_blocks(local_head + U256::one(), remote_head).await;
        
        // Download blocks in parallel
        let blocks = self.download_blocks().await?;
        
        // Sort blocks by number
        let mut sorted_blocks = blocks;
        sorted_blocks.sort_by_key(|b| b.header.number);
        
        Ok(sorted_blocks)
    }
    
    async fn queue_blocks(&self, start: U256, end: U256) {
        let mut queue = self.download_queue.write();
        
        let batch_size = std::cmp::min(
            self.config.max_block_request,
            (end - start).as_usize() + 1
        );
        
        for i in 0..batch_size {
            let block_num = start + U256::from(i);
            if block_num <= end {
                queue.push_back(block_num);
            }
        }
    }
    
    async fn download_blocks(&self) -> Result<Vec<Block>> {
        let mut blocks = Vec::new();
        let mut handles = Vec::new();
        
        // Start download tasks
        while let Some(block_num) = self.download_queue.write().pop_front() {
            let handle = self.download_block(block_num);
            handles.push(handle);
            
            // Limit concurrent downloads
            if handles.len() >= self.config.max_peers {
                break;
            }
        }
        
        // Wait for downloads to complete
        for handle in handles {
            match handle.await {
                Ok(block) => blocks.push(block),
                Err(e) => {
                    tracing::warn!("Failed to download block: {}", e);
                    // Re-queue failed block
                    // self.download_queue.write().push_back(block_num);
                }
            }
        }
        
        Ok(blocks)
    }
    
    async fn download_block(&self, block_number: U256) -> Result<Block> {
        // Select peer for download
        let peers = self.peer_manager.get_all_peers().await;
        if peers.is_empty() {
            return Err(SyncError::NoPeers);
        }
        
        let peer = &peers[0];
        let peer_id = H256::zero(); // Would get actual peer ID
        
        // Create download task
        let task = DownloadTask {
            block_number,
            peer_id,
            attempts: 1,
            started_at: std::time::Instant::now(),
        };
        
        self.downloading.write().insert(block_number, task);
        
        // Request block from peer
        // In real implementation, would send GetBlockBodies message
        
        // Simulate block download
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        // Create mock block
        let block = Block {
            header: Header {
                number: block_number,
                gas_limit: U256::from(8_000_000),
                gas_used: U256::zero(),
                timestamp: 0,
                ..Default::default()
            },
            body: Default::default(),
        };
        
        // Remove from downloading
        self.downloading.write().remove(&block_number);
        
        // Add to downloaded
        self.downloaded.write().insert(block_number, block.clone());
        
        Ok(block)
    }
    
    async fn get_local_head(&self) -> Result<U256> {
        // Get highest block number from database
        // For now, return 0
        Ok(U256::zero())
    }
    
    pub async fn cleanup_stale_downloads(&self) {
        let now = std::time::Instant::now();
        let timeout = self.config.timeout;
        
        let mut downloading = self.downloading.write();
        let mut to_retry = Vec::new();
        
        downloading.retain(|block_num, task| {
            if now.duration_since(task.started_at) > timeout {
                if task.attempts < self.config.retry_limit {
                    to_retry.push(*block_num);
                }
                false
            } else {
                true
            }
        });
        
        // Re-queue timed out blocks
        let mut queue = self.download_queue.write();
        for block_num in to_retry {
            queue.push_back(block_num);
        }
    }
    
    pub fn get_download_stats(&self) -> DownloadStats {
        DownloadStats {
            queued: self.download_queue.read().len(),
            downloading: self.downloading.read().len(),
            downloaded: self.downloaded.read().len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DownloadStats {
    pub queued: usize,
    pub downloading: usize,
    pub downloaded: usize,
}