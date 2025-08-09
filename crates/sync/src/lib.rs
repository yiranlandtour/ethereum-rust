use ethereum_types::{H256, U256};
use ethereum_core::{Block, Header};
use ethereum_storage::Database;
use ethereum_network::peer::{Peer, PeerManager};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::mpsc;
use tokio::time;

pub mod fast_sync;
pub mod snap_sync;
pub mod state_sync;
pub mod block_downloader;

pub use fast_sync::FastSync;
pub use snap_sync::SnapSync;
pub use state_sync::StateSync;
pub use block_downloader::BlockDownloader;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("Sync timeout")]
    Timeout,
    
    #[error("Invalid block: {0}")]
    InvalidBlock(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Storage error: {0}")]
    StorageError(#[from] ethereum_storage::StorageError),
    
    #[error("Peer disconnected")]
    PeerDisconnected,
    
    #[error("No peers available")]
    NoPeers,
    
    #[error("Sync cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, SyncError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncMode {
    Fast,
    Full,
    Snap,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    Downloading,
    Processing,
    Synced,
    Error,
}

#[derive(Debug, Clone)]
pub struct SyncProgress {
    pub starting_block: U256,
    pub current_block: U256,
    pub highest_block: U256,
    pub pulled_states: u64,
    pub known_states: u64,
}

pub struct SyncConfig {
    pub mode: SyncMode,
    pub max_peers: usize,
    pub max_block_request: usize,
    pub max_header_request: usize,
    pub max_body_request: usize,
    pub max_receipt_request: usize,
    pub max_state_request: usize,
    pub timeout: Duration,
    pub retry_limit: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            mode: SyncMode::Fast,
            max_peers: 25,
            max_block_request: 128,
            max_header_request: 192,
            max_body_request: 128,
            max_receipt_request: 256,
            max_state_request: 384,
            timeout: Duration::from_secs(10),
            retry_limit: 3,
        }
    }
}

pub struct Synchronizer<D: Database> {
    config: SyncConfig,
    db: Arc<D>,
    peer_manager: Arc<PeerManager>,
    status: Arc<RwLock<SyncStatus>>,
    progress: Arc<RwLock<SyncProgress>>,
    events_tx: mpsc::UnboundedSender<SyncEvent>,
    cancel_tx: Option<mpsc::Sender<()>>,
}

#[derive(Debug, Clone)]
pub enum SyncEvent {
    Started,
    Progress(SyncProgress),
    BlockImported(H256),
    StateImported(H256),
    Completed,
    Error(String),
}

impl<D: Database + 'static> Synchronizer<D> {
    pub fn new(
        config: SyncConfig,
        db: Arc<D>,
        peer_manager: Arc<PeerManager>,
    ) -> Self {
        let (events_tx, _) = mpsc::unbounded_channel();
        
        Self {
            config,
            db,
            peer_manager,
            status: Arc::new(RwLock::new(SyncStatus::Idle)),
            progress: Arc::new(RwLock::new(SyncProgress {
                starting_block: U256::zero(),
                current_block: U256::zero(),
                highest_block: U256::zero(),
                pulled_states: 0,
                known_states: 0,
            })),
            events_tx,
            cancel_tx: None,
        }
    }
    
    pub async fn start(&mut self) -> Result<()> {
        *self.status.write() = SyncStatus::Downloading;
        self.events_tx.send(SyncEvent::Started).ok();
        
        let (cancel_tx, mut cancel_rx) = mpsc::channel(1);
        self.cancel_tx = Some(cancel_tx);
        
        match self.config.mode {
            SyncMode::Fast => {
                self.run_fast_sync(&mut cancel_rx).await?;
            }
            SyncMode::Full => {
                self.run_full_sync(&mut cancel_rx).await?;
            }
            SyncMode::Snap => {
                self.run_snap_sync(&mut cancel_rx).await?;
            }
            SyncMode::Light => {
                self.run_light_sync(&mut cancel_rx).await?;
            }
        }
        
        *self.status.write() = SyncStatus::Synced;
        self.events_tx.send(SyncEvent::Completed).ok();
        
        Ok(())
    }
    
    pub async fn stop(&mut self) {
        if let Some(tx) = self.cancel_tx.take() {
            tx.send(()).await.ok();
        }
        *self.status.write() = SyncStatus::Idle;
    }
    
    async fn run_fast_sync(&self, cancel_rx: &mut mpsc::Receiver<()>) -> Result<()> {
        let fast_sync = FastSync::new(
            self.db.clone(),
            self.peer_manager.clone(),
            self.config.clone(),
        );
        
        // Download headers first
        let pivot_header = fast_sync.download_headers(cancel_rx).await?;
        
        // Download state at pivot block
        fast_sync.download_state(&pivot_header, cancel_rx).await?;
        
        // Download block bodies and receipts
        fast_sync.download_blocks(cancel_rx).await?;
        
        // Switch to full sync for remaining blocks
        self.run_full_sync(cancel_rx).await?;
        
        Ok(())
    }
    
    async fn run_full_sync(&self, cancel_rx: &mut mpsc::Receiver<()>) -> Result<()> {
        let downloader = BlockDownloader::new(
            self.db.clone(),
            self.peer_manager.clone(),
            self.config.clone(),
        );
        
        loop {
            tokio::select! {
                _ = cancel_rx.recv() => {
                    return Err(SyncError::Cancelled);
                }
                result = downloader.download_next_batch() => {
                    match result {
                        Ok(blocks) if blocks.is_empty() => {
                            // No more blocks to download
                            break;
                        }
                        Ok(blocks) => {
                            self.process_blocks(blocks).await?;
                        }
                        Err(e) => {
                            tracing::error!("Failed to download blocks: {}", e);
                            return Err(e);
                        }
                    }
                }
            }
            
            // Update progress
            self.update_progress().await;
        }
        
        Ok(())
    }
    
    async fn run_snap_sync(&self, cancel_rx: &mut mpsc::Receiver<()>) -> Result<()> {
        let snap_sync = SnapSync::new(
            self.db.clone(),
            self.peer_manager.clone(),
            self.config.clone(),
        );
        
        // Download account ranges
        snap_sync.download_accounts(cancel_rx).await?;
        
        // Download storage ranges
        snap_sync.download_storage(cancel_rx).await?;
        
        // Download bytecodes
        snap_sync.download_bytecodes(cancel_rx).await?;
        
        // Heal trie nodes
        snap_sync.heal_trie(cancel_rx).await?;
        
        // Switch to full sync
        self.run_full_sync(cancel_rx).await?;
        
        Ok(())
    }
    
    async fn run_light_sync(&self, _cancel_rx: &mut mpsc::Receiver<()>) -> Result<()> {
        // Light sync only downloads headers and verifies using CHT (Canonical Hash Trie)
        // This is a simplified implementation
        tracing::info!("Light sync not yet fully implemented");
        Ok(())
    }
    
    async fn process_blocks(&self, blocks: Vec<Block>) -> Result<()> {
        for block in blocks {
            // Validate block
            self.validate_block(&block)?;
            
            // Import block to database
            self.import_block(block).await?;
        }
        
        Ok(())
    }
    
    fn validate_block(&self, block: &Block) -> Result<()> {
        // Basic validation
        // Full validation would include:
        // 1. Parent hash exists
        // 2. Timestamp is valid
        // 3. Gas limit is within bounds
        // 4. Extra data size is valid
        // 5. Transaction root matches
        // 6. Receipt root matches
        // 7. State root matches (after execution)
        
        if block.header.gas_used > block.header.gas_limit {
            return Err(SyncError::InvalidBlock("Gas used exceeds gas limit".to_string()));
        }
        
        Ok(())
    }
    
    async fn import_block(&self, block: Block) -> Result<()> {
        let hash = block.header.hash();
        
        // Store block header
        let header_key = format!("header:{}", hex::encode(hash));
        self.db.put(
            header_key.as_bytes(),
            &bincode::serialize(&block.header).unwrap(),
        )?;
        
        // Store block body
        let body_key = format!("body:{}", hex::encode(hash));
        self.db.put(
            body_key.as_bytes(),
            &bincode::serialize(&block.body).unwrap(),
        )?;
        
        // Update canonical chain
        let number_key = format!("number:{}", block.header.number);
        self.db.put(number_key.as_bytes(), hash.as_bytes())?;
        
        // Send event
        self.events_tx.send(SyncEvent::BlockImported(hash)).ok();
        
        Ok(())
    }
    
    async fn update_progress(&self) {
        let progress = self.progress.read().clone();
        self.events_tx.send(SyncEvent::Progress(progress)).ok();
    }
    
    pub fn status(&self) -> SyncStatus {
        *self.status.read()
    }
    
    pub fn progress(&self) -> SyncProgress {
        self.progress.read().clone()
    }
    
    pub fn subscribe(&self) -> mpsc::UnboundedReceiver<SyncEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        
        // Forward events to new subscriber
        let events_tx = self.events_tx.clone();
        tokio::spawn(async move {
            // Implementation would forward events
        });
        
        rx
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sync_config_default() {
        let config = SyncConfig::default();
        assert_eq!(config.mode, SyncMode::Fast);
        assert_eq!(config.max_peers, 25);
    }
}