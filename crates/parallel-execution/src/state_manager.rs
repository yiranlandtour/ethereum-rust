use ethereum_types::{H256, U256, Address};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use dashmap::DashMap;

use crate::{Result, ParallelExecutionError};

/// State manager for parallel execution
pub struct StateManager {
    state: Arc<DashMap<Address, AccountState>>,
    snapshots: RwLock<Vec<StateSnapshot>>,
}

#[derive(Debug, Clone)]
pub struct AccountState {
    pub nonce: U256,
    pub balance: U256,
    pub code_hash: H256,
    pub storage: HashMap<H256, H256>,
}

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub id: u64,
    pub state: HashMap<Address, AccountState>,
    pub timestamp: std::time::Instant,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(DashMap::new()),
            snapshots: RwLock::new(Vec::new()),
        }
    }
    
    pub fn from_snapshot(snapshot: StateSnapshot) -> Self {
        let state = Arc::new(DashMap::new());
        for (addr, account) in snapshot.state {
            state.insert(addr, account);
        }
        
        Self {
            state,
            snapshots: RwLock::new(vec![snapshot]),
        }
    }
    
    /// Create a snapshot of current state
    pub fn snapshot(&self) -> Result<StateSnapshot> {
        let mut state_copy = HashMap::new();
        
        for entry in self.state.iter() {
            state_copy.insert(*entry.key(), entry.value().clone());
        }
        
        let snapshot = StateSnapshot {
            id: self.snapshots.read().unwrap().len() as u64,
            state: state_copy,
            timestamp: std::time::Instant::now(),
        };
        
        self.snapshots.write().unwrap().push(snapshot.clone());
        
        Ok(snapshot)
    }
    
    /// Restore from a snapshot
    pub fn restore(&self, snapshot_id: u64) -> Result<()> {
        let snapshots = self.snapshots.read().unwrap();
        let snapshot = snapshots
            .iter()
            .find(|s| s.id == snapshot_id)
            .ok_or_else(|| ParallelExecutionError::StateError(
                format!("Snapshot {} not found", snapshot_id)
            ))?;
        
        self.state.clear();
        for (addr, account) in &snapshot.state {
            self.state.insert(*addr, account.clone());
        }
        
        Ok(())
    }
    
    /// Get account state
    pub fn get_account(&self, address: &Address) -> Option<AccountState> {
        self.state.get(address).map(|entry| entry.clone())
    }
    
    /// Set account state
    pub fn set_account(&self, address: Address, account: AccountState) {
        self.state.insert(address, account);
    }
    
    /// Get storage value
    pub fn get_storage(&self, address: &Address, slot: &H256) -> Option<H256> {
        self.state.get(address).and_then(|account| {
            account.storage.get(slot).copied()
        })
    }
    
    /// Set storage value
    pub fn set_storage(&self, address: Address, slot: H256, value: H256) {
        self.state.entry(address)
            .or_insert_with(|| AccountState::default())
            .storage
            .insert(slot, value);
    }
    
    /// Check if changes can be applied
    pub fn can_apply(&self, address: &Address, changes: &StateChanges) -> Result<bool> {
        if let Some(account) = self.get_account(address) {
            // Check nonce ordering
            if let Some(new_nonce) = changes.nonce {
                if new_nonce < account.nonce {
                    return Ok(false);
                }
            }
            
            // Check balance
            if let Some(balance_change) = &changes.balance_change {
                if balance_change.is_decrease && balance_change.amount > account.balance {
                    return Ok(false);
                }
            }
        }
        
        Ok(true)
    }
    
    /// Apply state changes
    pub fn apply_changes(&self, changes: &HashMap<Address, StateChanges>) -> Result<()> {
        for (address, change) in changes {
            let mut account = self.get_account(address)
                .unwrap_or_else(AccountState::default);
            
            if let Some(nonce) = change.nonce {
                account.nonce = nonce;
            }
            
            if let Some(balance_change) = &change.balance_change {
                if balance_change.is_decrease {
                    account.balance = account.balance.saturating_sub(balance_change.amount);
                } else {
                    account.balance = account.balance.saturating_add(balance_change.amount);
                }
            }
            
            for (slot, value) in &change.storage {
                account.storage.insert(*slot, *value);
            }
            
            self.set_account(*address, account);
        }
        
        Ok(())
    }
    
    /// Compute state root
    pub fn compute_state_root(&self) -> Result<H256> {
        // Simplified: hash all account states
        let mut data = Vec::new();
        
        let mut accounts: Vec<_> = self.state.iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();
        
        accounts.sort_by_key(|(addr, _)| *addr);
        
        for (addr, account) in accounts {
            data.extend_from_slice(addr.as_bytes());
            data.extend_from_slice(&account.nonce.to_be_bytes::<32>());
            data.extend_from_slice(&account.balance.to_be_bytes::<32>());
            data.extend_from_slice(account.code_hash.as_bytes());
        }
        
        Ok(H256::from_slice(&ethereum_crypto::keccak256(&data)))
    }
}

impl Default for AccountState {
    fn default() -> Self {
        Self {
            nonce: U256::zero(),
            balance: U256::zero(),
            code_hash: H256::zero(),
            storage: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StateChanges {
    pub nonce: Option<U256>,
    pub balance_change: Option<BalanceChange>,
    pub storage: HashMap<H256, H256>,
}

#[derive(Debug, Clone)]
pub struct BalanceChange {
    pub amount: U256,
    pub is_decrease: bool,
}