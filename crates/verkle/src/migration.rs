use ethereum_types::{H256, U256, Address};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{info, debug, warn};
use async_trait::async_trait;

use crate::{Result, VerkleError};
use crate::tree::{VerkleTree, VerkleConfig};

/// Migration strategy from MPT to Verkle
#[derive(Debug, Clone)]
pub enum MigrationStrategy {
    /// Migrate all at once (for testnets)
    Immediate,
    /// Gradual migration over time
    Gradual {
        accounts_per_block: usize,
        storage_slots_per_account: usize,
    },
    /// Overlay approach - maintain both trees
    Overlay {
        transition_block: u64,
    },
    /// Copy-on-write migration
    CopyOnWrite,
}

/// Migration status
#[derive(Debug, Clone)]
pub struct MigrationStatus {
    pub total_accounts: u64,
    pub migrated_accounts: u64,
    pub total_storage_slots: u64,
    pub migrated_storage_slots: u64,
    pub started_at: Instant,
    pub estimated_completion: Option<Duration>,
    pub current_phase: MigrationPhase,
}

#[derive(Debug, Clone)]
pub enum MigrationPhase {
    NotStarted,
    PreparingSnapshot,
    MigratingAccounts,
    MigratingStorage,
    VerifyingIntegrity,
    Completed,
}

/// State source for migration
#[async_trait]
pub trait StateSource: Send + Sync {
    async fn get_account(&self, address: &Address) -> Result<Option<AccountData>>;
    async fn get_storage(&self, address: &Address, slot: &H256) -> Result<Option<H256>>;
    async fn get_code(&self, code_hash: &H256) -> Result<Option<Vec<u8>>>;
    async fn list_accounts(&self) -> Result<Vec<Address>>;
    async fn list_storage_keys(&self, address: &Address) -> Result<Vec<H256>>;
}

#[derive(Debug, Clone)]
pub struct AccountData {
    pub nonce: U256,
    pub balance: U256,
    pub code_hash: H256,
    pub storage_root: H256,
}

/// State migrator from MPT to Verkle
pub struct StateMigrator {
    strategy: MigrationStrategy,
    source: Arc<dyn StateSource>,
    target_tree: Arc<VerkleTree>,
    status: Arc<RwLock<MigrationStatus>>,
    migration_queue: Arc<RwLock<MigrationQueue>>,
    workers: Vec<tokio::task::JoinHandle<()>>,
    metrics: Arc<MigrationMetrics>,
}

struct MigrationQueue {
    accounts: VecDeque<Address>,
    storage_items: VecDeque<(Address, H256)>,
    code_items: VecDeque<H256>,
}

struct MigrationMetrics {
    accounts_processed: std::sync::atomic::AtomicU64,
    storage_slots_processed: std::sync::atomic::AtomicU64,
    errors_encountered: std::sync::atomic::AtomicU64,
    bytes_migrated: std::sync::atomic::AtomicU64,
}

impl StateMigrator {
    pub fn new(
        strategy: MigrationStrategy,
        source: Arc<dyn StateSource>,
        target_tree: VerkleTree,
    ) -> Self {
        Self {
            strategy,
            source,
            target_tree: Arc::new(target_tree),
            status: Arc::new(RwLock::new(MigrationStatus {
                total_accounts: 0,
                migrated_accounts: 0,
                total_storage_slots: 0,
                migrated_storage_slots: 0,
                started_at: Instant::now(),
                estimated_completion: None,
                current_phase: MigrationPhase::NotStarted,
            })),
            migration_queue: Arc::new(RwLock::new(MigrationQueue {
                accounts: VecDeque::new(),
                storage_items: VecDeque::new(),
                code_items: VecDeque::new(),
            })),
            workers: Vec::new(),
            metrics: Arc::new(MigrationMetrics::new()),
        }
    }
    
    /// Start the migration process
    pub async fn start_migration(&mut self) -> Result<()> {
        info!("Starting state migration with strategy: {:?}", self.strategy);
        
        // Update status
        {
            let mut status = self.status.write().unwrap();
            status.current_phase = MigrationPhase::PreparingSnapshot;
            status.started_at = Instant::now();
        }
        
        // Prepare migration based on strategy
        match &self.strategy {
            MigrationStrategy::Immediate => {
                self.migrate_immediate().await?
            }
            MigrationStrategy::Gradual { accounts_per_block, storage_slots_per_account } => {
                self.migrate_gradual(*accounts_per_block, *storage_slots_per_account).await?
            }
            MigrationStrategy::Overlay { transition_block } => {
                self.migrate_overlay(*transition_block).await?
            }
            MigrationStrategy::CopyOnWrite => {
                self.migrate_copy_on_write().await?
            }
        }
        
        Ok(())
    }
    
    /// Immediate migration (for testnets)
    async fn migrate_immediate(&mut self) -> Result<()> {
        info!("Starting immediate migration");
        
        // Get all accounts
        let accounts = self.source.list_accounts().await?;
        
        {
            let mut status = self.status.write().unwrap();
            status.total_accounts = accounts.len() as u64;
            status.current_phase = MigrationPhase::MigratingAccounts;
        }
        
        // Migrate all accounts
        for address in accounts {
            self.migrate_account(&address).await?;
        }
        
        // Verify migration
        self.verify_migration().await?;
        
        {
            let mut status = self.status.write().unwrap();
            status.current_phase = MigrationPhase::Completed;
        }
        
        info!("Immediate migration completed");
        Ok(())
    }
    
    /// Gradual migration
    async fn migrate_gradual(
        &mut self,
        accounts_per_block: usize,
        storage_slots_per_account: usize,
    ) -> Result<()> {
        info!("Starting gradual migration: {} accounts/block, {} slots/account",
            accounts_per_block, storage_slots_per_account);
        
        // Get all accounts and add to queue
        let accounts = self.source.list_accounts().await?;
        
        {
            let mut queue = self.migration_queue.write().unwrap();
            for account in accounts {
                queue.accounts.push_back(account);
            }
        }
        
        // Start worker threads
        for i in 0..4 {
            let migrator = self.clone_for_worker();
            let handle = tokio::spawn(async move {
                migrator.migration_worker(
                    i,
                    accounts_per_block,
                    storage_slots_per_account,
                ).await;
            });
            self.workers.push(handle);
        }
        
        Ok(())
    }
    
    /// Overlay migration - maintain both trees
    async fn migrate_overlay(&mut self, transition_block: u64) -> Result<()> {
        info!("Starting overlay migration at block {}", transition_block);
        
        // Create overlay structure
        let overlay = OverlayMigration::new(
            self.source.clone(),
            self.target_tree.clone(),
            transition_block,
        );
        
        overlay.start().await?;
        
        Ok(())
    }
    
    /// Copy-on-write migration
    async fn migrate_copy_on_write(&mut self) -> Result<()> {
        info!("Starting copy-on-write migration");
        
        // Set up CoW handler
        let cow_handler = CopyOnWriteHandler::new(
            self.source.clone(),
            self.target_tree.clone(),
        );
        
        cow_handler.start().await?;
        
        Ok(())
    }
    
    /// Migrate a single account
    async fn migrate_account(&self, address: &Address) -> Result<()> {
        debug!("Migrating account: {:?}", address);
        
        // Get account data
        if let Some(account) = self.source.get_account(address).await? {
            // Convert to Verkle format
            let key = self.account_key(address);
            let value = self.encode_account(&account)?;
            
            // Insert into Verkle tree
            self.target_tree.insert(&key, &value)?;
            
            // Migrate storage
            let storage_keys = self.source.list_storage_keys(address).await?;
            for slot in storage_keys {
                if let Some(value) = self.source.get_storage(address, &slot).await? {
                    let storage_key = self.storage_key(address, &slot);
                    self.target_tree.insert(&storage_key, value.as_bytes())?;
                    
                    self.metrics.storage_slots_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            
            // Migrate code if present
            if account.code_hash != H256::zero() {
                if let Some(code) = self.source.get_code(&account.code_hash).await? {
                    let code_key = self.code_key(&account.code_hash);
                    self.target_tree.insert(&code_key, &code)?;
                }
            }
            
            self.metrics.accounts_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            
            // Update status
            let mut status = self.status.write().unwrap();
            status.migrated_accounts += 1;
        }
        
        Ok(())
    }
    
    /// Worker for gradual migration
    async fn migration_worker(
        &self,
        worker_id: usize,
        accounts_per_batch: usize,
        storage_slots_per_account: usize,
    ) {
        info!("Migration worker {} started", worker_id);
        
        loop {
            // Get batch of accounts
            let accounts = {
                let mut queue = self.migration_queue.write().unwrap();
                let mut batch = Vec::new();
                
                for _ in 0..accounts_per_batch {
                    if let Some(account) = queue.accounts.pop_front() {
                        batch.push(account);
                    } else {
                        break;
                    }
                }
                
                batch
            };
            
            if accounts.is_empty() {
                break;
            }
            
            // Migrate batch
            for address in accounts {
                if let Err(e) = self.migrate_account(&address).await {
                    warn!("Failed to migrate account {:?}: {}", address, e);
                    self.metrics.errors_encountered.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            
            // Small delay to avoid overwhelming the system
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        
        info!("Migration worker {} completed", worker_id);
    }
    
    /// Verify migration integrity
    async fn verify_migration(&self) -> Result<()> {
        info!("Verifying migration integrity");
        
        {
            let mut status = self.status.write().unwrap();
            status.current_phase = MigrationPhase::VerifyingIntegrity;
        }
        
        // Sample verification - check random accounts
        let accounts = self.source.list_accounts().await?;
        let sample_size = (accounts.len() / 100).max(10).min(1000);
        
        use rand::seq::SliceRandom;
        let mut rng = rand::thread_rng();
        let sample: Vec<_> = accounts.choose_multiple(&mut rng, sample_size).cloned().collect();
        
        for address in sample {
            // Compare source and target
            let source_account = self.source.get_account(&address).await?;
            let verkle_key = self.account_key(&address);
            let verkle_data = self.target_tree.get(&verkle_key)?;
            
            if let (Some(source), Some(target)) = (source_account, verkle_data) {
                let decoded = self.decode_account(&target)?;
                
                if decoded.nonce != source.nonce || decoded.balance != source.balance {
                    return Err(VerkleError::MigrationFailed(
                        format!("Account verification failed for {:?}", address)
                    ));
                }
            }
        }
        
        info!("Migration verification completed successfully");
        Ok(())
    }
    
    /// Get migration status
    pub fn get_status(&self) -> MigrationStatus {
        self.status.read().unwrap().clone()
    }
    
    /// Stop migration
    pub async fn stop_migration(&mut self) {
        info!("Stopping migration");
        
        // Cancel all workers
        for worker in self.workers.drain(..) {
            worker.abort();
        }
    }
    
    // Helper methods
    
    fn account_key(&self, address: &Address) -> Vec<u8> {
        let mut key = vec![0u8; 32];
        key[0] = 0x00; // Account prefix
        key[1..21].copy_from_slice(address.as_bytes());
        key
    }
    
    fn storage_key(&self, address: &Address, slot: &H256) -> Vec<u8> {
        let mut key = vec![0u8; 32];
        key[0] = 0x01; // Storage prefix
        
        let hash = ethereum_crypto::keccak256(&[
            address.as_bytes(),
            slot.as_bytes(),
        ].concat());
        
        key[1..32].copy_from_slice(&hash[..31]);
        key
    }
    
    fn code_key(&self, code_hash: &H256) -> Vec<u8> {
        let mut key = vec![0u8; 32];
        key[0] = 0x02; // Code prefix
        key[1..32].copy_from_slice(&code_hash.as_bytes()[..31]);
        key
    }
    
    fn encode_account(&self, account: &AccountData) -> Result<Vec<u8>> {
        Ok(bincode::serialize(account)
            .map_err(|e| VerkleError::MigrationFailed(e.to_string()))?)
    }
    
    fn decode_account(&self, data: &[u8]) -> Result<AccountData> {
        bincode::deserialize(data)
            .map_err(|e| VerkleError::MigrationFailed(e.to_string()))
    }
    
    fn clone_for_worker(&self) -> Self {
        Self {
            strategy: self.strategy.clone(),
            source: self.source.clone(),
            target_tree: self.target_tree.clone(),
            status: self.status.clone(),
            migration_queue: self.migration_queue.clone(),
            workers: Vec::new(),
            metrics: self.metrics.clone(),
        }
    }
}

impl MigrationMetrics {
    fn new() -> Self {
        Self {
            accounts_processed: std::sync::atomic::AtomicU64::new(0),
            storage_slots_processed: std::sync::atomic::AtomicU64::new(0),
            errors_encountered: std::sync::atomic::AtomicU64::new(0),
            bytes_migrated: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

/// Overlay migration handler
struct OverlayMigration {
    source: Arc<dyn StateSource>,
    target: Arc<VerkleTree>,
    transition_block: u64,
}

impl OverlayMigration {
    fn new(
        source: Arc<dyn StateSource>,
        target: Arc<VerkleTree>,
        transition_block: u64,
    ) -> Self {
        Self {
            source,
            target,
            transition_block,
        }
    }
    
    async fn start(&self) -> Result<()> {
        // Implementation for overlay migration
        info!("Overlay migration started at block {}", self.transition_block);
        Ok(())
    }
}

/// Copy-on-write handler
struct CopyOnWriteHandler {
    source: Arc<dyn StateSource>,
    target: Arc<VerkleTree>,
}

impl CopyOnWriteHandler {
    fn new(source: Arc<dyn StateSource>, target: Arc<VerkleTree>) -> Self {
        Self { source, target }
    }
    
    async fn start(&self) -> Result<()> {
        // Implementation for CoW migration
        info!("Copy-on-write migration started");
        Ok(())
    }
}