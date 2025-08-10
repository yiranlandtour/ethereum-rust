use ethereum_types::{H256, Address};
use ethereum_core::Transaction;
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use crate::Result;

/// Access set for tracking state access
#[derive(Debug, Clone, Default)]
pub struct AccessSet {
    pub reads: HashMap<Address, HashSet<H256>>,
    pub writes: HashMap<Address, HashMap<H256, H256>>,
    pub creates: HashSet<Address>,
    pub deletes: HashSet<Address>,
}

impl AccessSet {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_read(&mut self, address: Address, slot: H256) {
        self.reads.entry(address).or_insert_with(HashSet::new).insert(slot);
    }
    
    pub fn add_write(&mut self, address: Address, slot: H256, value: H256) {
        self.writes.entry(address).or_insert_with(HashMap::new).insert(slot, value);
    }
    
    pub fn add_create(&mut self, address: Address) {
        self.creates.insert(address);
    }
    
    pub fn add_delete(&mut self, address: Address) {
        self.deletes.insert(address);
    }
    
    pub fn merge(&mut self, other: &AccessSet) {
        for (addr, slots) in &other.reads {
            self.reads.entry(*addr).or_insert_with(HashSet::new).extend(slots);
        }
        
        for (addr, writes) in &other.writes {
            self.writes.entry(*addr).or_insert_with(HashMap::new).extend(writes);
        }
        
        self.creates.extend(&other.creates);
        self.deletes.extend(&other.deletes);
    }
}

/// Conflict detector for parallel execution
pub struct ConflictDetector {
    conflict_cache: RwLock<HashMap<(H256, H256), bool>>,
}

impl ConflictDetector {
    pub fn new() -> Self {
        Self {
            conflict_cache: RwLock::new(HashMap::new()),
        }
    }
    
    /// Check if two transactions conflict
    pub fn check_conflict(&self, tx1: &Transaction, tx2: &Transaction) -> Result<bool> {
        let key = (tx1.hash(), tx2.hash());
        
        // Check cache
        if let Some(&result) = self.conflict_cache.read().unwrap().get(&key) {
            return Ok(result);
        }
        
        // Analyze for conflicts
        let has_conflict = self.analyze_conflict(tx1, tx2)?;
        
        // Cache result
        self.conflict_cache.write().unwrap().insert(key, has_conflict);
        
        Ok(has_conflict)
    }
    
    /// Check if two access sets conflict
    pub fn conflicts(&self, set1: &AccessSet, set2: &AccessSet) -> bool {
        // Read-write conflicts
        for (addr, slots1) in &set1.reads {
            if let Some(writes2) = set2.writes.get(addr) {
                for slot in slots1 {
                    if writes2.contains_key(slot) {
                        return true;
                    }
                }
            }
        }
        
        // Write-read conflicts
        for (addr, writes1) in &set1.writes {
            if let Some(slots2) = set2.reads.get(addr) {
                for slot in writes1.keys() {
                    if slots2.contains(slot) {
                        return true;
                    }
                }
            }
        }
        
        // Write-write conflicts
        for (addr, writes1) in &set1.writes {
            if let Some(writes2) = set2.writes.get(addr) {
                for slot in writes1.keys() {
                    if writes2.contains_key(slot) {
                        return true;
                    }
                }
            }
        }
        
        // Create-delete conflicts
        if !set1.creates.is_disjoint(&set2.deletes) ||
           !set1.deletes.is_disjoint(&set2.creates) {
            return true;
        }
        
        // Create-create conflicts
        if !set1.creates.is_disjoint(&set2.creates) {
            return true;
        }
        
        false
    }
    
    fn analyze_conflict(&self, tx1: &Transaction, tx2: &Transaction) -> Result<bool> {
        // Same sender always conflicts (nonce ordering)
        if tx1.from == tx2.from {
            return Ok(true);
        }
        
        // Check contract interactions
        if let (Some(to1), Some(to2)) = (tx1.to, tx2.to) {
            // Same contract
            if to1 == to2 {
                // Could analyze call data for more precise conflict detection
                return Ok(true);
            }
            
            // Cross-contract calls (simplified)
            if self.may_interact(to1, to2) {
                return Ok(true);
            }
        }
        
        Ok(false)
    }
    
    fn may_interact(&self, addr1: Address, addr2: Address) -> bool {
        // Simplified: assume no cross-contract interactions
        // In production, would analyze contract code and storage
        false
    }
    
    pub fn clear_cache(&self) {
        self.conflict_cache.write().unwrap().clear();
    }
}