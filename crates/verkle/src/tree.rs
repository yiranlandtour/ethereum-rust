use ethereum_types::{H256, U256, Address};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tracing::{debug, info};

use crate::{Result, VerkleError};
use crate::node::{VerkleNode, NodeType, Extension, Branch};
use crate::commitment::{VerkleCommitment, Commitment};
use crate::proof::VerkleProof;

/// Verkle tree configuration
#[derive(Debug, Clone)]
pub struct VerkleConfig {
    /// Width of the tree (number of children per branch)
    pub width: usize,
    /// Key length in bytes
    pub key_length: usize,
    /// Commitment scheme (IPA or KZG)
    pub commitment_scheme: CommitmentScheme,
    /// Enable caching
    pub enable_cache: bool,
    /// Cache size
    pub cache_size: usize,
}

#[derive(Debug, Clone)]
pub enum CommitmentScheme {
    IPA,
    KZG,
}

impl Default for VerkleConfig {
    fn default() -> Self {
        Self {
            width: 256,  // 2^8 children per branch
            key_length: 32,
            commitment_scheme: CommitmentScheme::IPA,
            enable_cache: true,
            cache_size: 10000,
        }
    }
}

/// Main Verkle tree implementation
pub struct VerkleTree {
    config: VerkleConfig,
    root: Arc<RwLock<Option<VerkleNode>>>,
    commitment_engine: Arc<VerkleCommitment>,
    cache: Arc<RwLock<HashMap<H256, Vec<u8>>>>,
    metrics: Arc<TreeMetrics>,
}

struct TreeMetrics {
    reads: std::sync::atomic::AtomicU64,
    writes: std::sync::atomic::AtomicU64,
    cache_hits: std::sync::atomic::AtomicU64,
    cache_misses: std::sync::atomic::AtomicU64,
}

impl VerkleTree {
    pub fn new(config: VerkleConfig) -> Result<Self> {
        let commitment_engine = Arc::new(VerkleCommitment::new(&config)?);
        
        Ok(Self {
            config,
            root: Arc::new(RwLock::new(None)),
            commitment_engine,
            cache: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(TreeMetrics::new()),
        })
    }
    
    /// Insert a key-value pair into the tree
    pub fn insert(&self, key: &[u8], value: &[u8]) -> Result<()> {
        if key.len() != self.config.key_length {
            return Err(VerkleError::InvalidKey(
                format!("Key length must be {}", self.config.key_length)
            ));
        }
        
        self.metrics.writes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        let mut root_guard = self.root.write().unwrap();
        
        if root_guard.is_none() {
            // Create root node
            *root_guard = Some(VerkleNode::new_extension(vec![0u8; 31]));
        }
        
        let root = root_guard.as_mut().unwrap();
        self.insert_recursive(root, key, value, 0)?;
        
        // Update cache
        if self.config.enable_cache {
            let mut cache = self.cache.write().unwrap();
            cache.insert(H256::from_slice(key), value.to_vec());
            
            // Evict if cache is full
            if cache.len() > self.config.cache_size {
                self.evict_cache_entries(&mut cache);
            }
        }
        
        Ok(())
    }
    
    fn insert_recursive(
        &self,
        node: &mut VerkleNode,
        key: &[u8],
        value: &[u8],
        depth: usize,
    ) -> Result<()> {
        match &mut node.node_type {
            NodeType::Extension(ext) => {
                // Check if we need to split the extension
                let common_prefix = self.common_prefix(&ext.stem, &key[depth..]);
                
                if common_prefix == ext.stem.len() {
                    // Continue to suffix tree
                    if ext.suffix_tree.is_none() {
                        ext.suffix_tree = Some(Box::new(VerkleNode::new_branch()));
                    }
                    
                    self.insert_recursive(
                        ext.suffix_tree.as_mut().unwrap(),
                        key,
                        value,
                        depth + common_prefix,
                    )?;
                } else {
                    // Split the extension
                    self.split_extension(node, key, value, depth, common_prefix)?;
                }
            }
            NodeType::Branch(branch) => {
                if depth >= key.len() {
                    // Store value at branch
                    branch.value = Some(value.to_vec());
                } else {
                    let index = key[depth] as usize;
                    
                    if branch.children[index].is_none() {
                        // Create new extension for this path
                        let remaining_key = &key[depth + 1..];
                        let mut new_ext = VerkleNode::new_extension(remaining_key.to_vec());
                        
                        // Add value as leaf
                        if let NodeType::Extension(ext) = &mut new_ext.node_type {
                            ext.suffix_tree = Some(Box::new(VerkleNode::new_leaf(value.to_vec())));
                        }
                        
                        branch.children[index] = Some(Box::new(new_ext));
                    } else {
                        self.insert_recursive(
                            branch.children[index].as_mut().unwrap(),
                            key,
                            value,
                            depth + 1,
                        )?;
                    }
                }
            }
            NodeType::Leaf(leaf_value) => {
                // Replace leaf value
                *leaf_value = value.to_vec();
            }
        }
        
        // Update commitment
        node.commitment = self.commitment_engine.compute_node_commitment(node)?;
        
        Ok(())
    }
    
    /// Get a value from the tree
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        if key.len() != self.config.key_length {
            return Err(VerkleError::InvalidKey(
                format!("Key length must be {}", self.config.key_length)
            ));
        }
        
        self.metrics.reads.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // Check cache first
        if self.config.enable_cache {
            let cache = self.cache.read().unwrap();
            if let Some(value) = cache.get(&H256::from_slice(key)) {
                self.metrics.cache_hits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return Ok(Some(value.clone()));
            }
            self.metrics.cache_misses.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        
        let root_guard = self.root.read().unwrap();
        
        if let Some(root) = root_guard.as_ref() {
            self.get_recursive(root, key, 0)
        } else {
            Ok(None)
        }
    }
    
    fn get_recursive(
        &self,
        node: &VerkleNode,
        key: &[u8],
        depth: usize,
    ) -> Result<Option<Vec<u8>>> {
        match &node.node_type {
            NodeType::Extension(ext) => {
                let key_part = &key[depth..depth + ext.stem.len().min(key.len() - depth)];
                
                if key_part == ext.stem.as_slice() {
                    if let Some(suffix) = &ext.suffix_tree {
                        self.get_recursive(suffix, key, depth + ext.stem.len())
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            NodeType::Branch(branch) => {
                if depth >= key.len() {
                    Ok(branch.value.clone())
                } else {
                    let index = key[depth] as usize;
                    
                    if let Some(child) = &branch.children[index] {
                        self.get_recursive(child, key, depth + 1)
                    } else {
                        Ok(None)
                    }
                }
            }
            NodeType::Leaf(value) => {
                Ok(Some(value.clone()))
            }
        }
    }
    
    /// Delete a key from the tree
    pub fn delete(&self, key: &[u8]) -> Result<bool> {
        if key.len() != self.config.key_length {
            return Err(VerkleError::InvalidKey(
                format!("Key length must be {}", self.config.key_length)
            ));
        }
        
        let mut root_guard = self.root.write().unwrap();
        
        if let Some(root) = root_guard.as_mut() {
            let deleted = self.delete_recursive(root, key, 0)?;
            
            // Remove from cache
            if deleted && self.config.enable_cache {
                let mut cache = self.cache.write().unwrap();
                cache.remove(&H256::from_slice(key));
            }
            
            Ok(deleted)
        } else {
            Ok(false)
        }
    }
    
    fn delete_recursive(
        &self,
        node: &mut VerkleNode,
        key: &[u8],
        depth: usize,
    ) -> Result<bool> {
        let deleted = match &mut node.node_type {
            NodeType::Extension(ext) => {
                let key_part = &key[depth..depth + ext.stem.len().min(key.len() - depth)];
                
                if key_part == ext.stem.as_slice() {
                    if let Some(suffix) = &mut ext.suffix_tree {
                        self.delete_recursive(suffix, key, depth + ext.stem.len())?
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            NodeType::Branch(branch) => {
                if depth >= key.len() {
                    if branch.value.is_some() {
                        branch.value = None;
                        true
                    } else {
                        false
                    }
                } else {
                    let index = key[depth] as usize;
                    
                    if let Some(child) = &mut branch.children[index] {
                        let deleted = self.delete_recursive(child, key, depth + 1)?;
                        
                        // Remove empty child
                        if deleted && self.is_node_empty(child) {
                            branch.children[index] = None;
                        }
                        
                        deleted
                    } else {
                        false
                    }
                }
            }
            NodeType::Leaf(_) => {
                true // Leaf will be removed by parent
            }
        };
        
        if deleted {
            // Update commitment
            node.commitment = self.commitment_engine.compute_node_commitment(node)?;
        }
        
        Ok(deleted)
    }
    
    /// Generate a proof for a key
    pub fn generate_proof(&self, key: &[u8]) -> Result<VerkleProof> {
        if key.len() != self.config.key_length {
            return Err(VerkleError::InvalidKey(
                format!("Key length must be {}", self.config.key_length)
            ));
        }
        
        let root_guard = self.root.read().unwrap();
        
        if let Some(root) = root_guard.as_ref() {
            let mut proof_nodes = Vec::new();
            let value = self.collect_proof_nodes(root, key, 0, &mut proof_nodes)?;
            
            Ok(VerkleProof::new(
                key.to_vec(),
                value,
                proof_nodes,
                root.commitment.clone(),
            ))
        } else {
            Err(VerkleError::NodeNotFound("Root node not found".to_string()))
        }
    }
    
    fn collect_proof_nodes(
        &self,
        node: &VerkleNode,
        key: &[u8],
        depth: usize,
        proof_nodes: &mut Vec<(Vec<u8>, Commitment)>,
    ) -> Result<Option<Vec<u8>>> {
        proof_nodes.push((key[..depth].to_vec(), node.commitment.clone()));
        
        match &node.node_type {
            NodeType::Extension(ext) => {
                let key_part = &key[depth..depth + ext.stem.len().min(key.len() - depth)];
                
                if key_part == ext.stem.as_slice() {
                    if let Some(suffix) = &ext.suffix_tree {
                        self.collect_proof_nodes(suffix, key, depth + ext.stem.len(), proof_nodes)
                    } else {
                        Ok(None)
                    }
                } else {
                    Ok(None)
                }
            }
            NodeType::Branch(branch) => {
                if depth >= key.len() {
                    Ok(branch.value.clone())
                } else {
                    let index = key[depth] as usize;
                    
                    // Add sibling commitments to proof
                    for (i, child) in branch.children.iter().enumerate() {
                        if i != index {
                            if let Some(sibling) = child {
                                proof_nodes.push((
                                    vec![i as u8],
                                    sibling.commitment.clone(),
                                ));
                            }
                        }
                    }
                    
                    if let Some(child) = &branch.children[index] {
                        self.collect_proof_nodes(child, key, depth + 1, proof_nodes)
                    } else {
                        Ok(None)
                    }
                }
            }
            NodeType::Leaf(value) => {
                Ok(Some(value.clone()))
            }
        }
    }
    
    /// Verify a proof
    pub fn verify_proof(&self, proof: &VerkleProof) -> Result<bool> {
        self.commitment_engine.verify_proof(proof)
    }
    
    /// Get the root commitment
    pub fn root_commitment(&self) -> Option<Commitment> {
        let root_guard = self.root.read().unwrap();
        root_guard.as_ref().map(|r| r.commitment.clone())
    }
    
    /// Get tree statistics
    pub fn get_stats(&self) -> TreeStats {
        TreeStats {
            reads: self.metrics.reads.load(std::sync::atomic::Ordering::Relaxed),
            writes: self.metrics.writes.load(std::sync::atomic::Ordering::Relaxed),
            cache_hits: self.metrics.cache_hits.load(std::sync::atomic::Ordering::Relaxed),
            cache_misses: self.metrics.cache_misses.load(std::sync::atomic::Ordering::Relaxed),
            cache_size: self.cache.read().unwrap().len(),
        }
    }
    
    // Helper methods
    
    fn common_prefix(&self, a: &[u8], b: &[u8]) -> usize {
        a.iter()
            .zip(b.iter())
            .take_while(|(x, y)| x == y)
            .count()
    }
    
    fn split_extension(
        &self,
        node: &mut VerkleNode,
        key: &[u8],
        value: &[u8],
        depth: usize,
        common_prefix: usize,
    ) -> Result<()> {
        if let NodeType::Extension(ext) = &mut node.node_type {
            let old_stem = ext.stem.clone();
            let old_suffix = ext.suffix_tree.take();
            
            // Update current extension with common prefix
            ext.stem = old_stem[..common_prefix].to_vec();
            
            // Create branch for divergence
            let mut branch = VerkleNode::new_branch();
            
            if let NodeType::Branch(branch_data) = &mut branch.node_type {
                // Add old path
                if common_prefix < old_stem.len() {
                    let old_index = old_stem[common_prefix] as usize;
                    let mut old_ext = VerkleNode::new_extension(
                        old_stem[common_prefix + 1..].to_vec()
                    );
                    
                    if let NodeType::Extension(old_ext_data) = &mut old_ext.node_type {
                        old_ext_data.suffix_tree = old_suffix;
                    }
                    
                    branch_data.children[old_index] = Some(Box::new(old_ext));
                }
                
                // Add new path
                let new_index = key[depth + common_prefix] as usize;
                let mut new_ext = VerkleNode::new_extension(
                    key[depth + common_prefix + 1..].to_vec()
                );
                
                if let NodeType::Extension(new_ext_data) = &mut new_ext.node_type {
                    new_ext_data.suffix_tree = Some(Box::new(VerkleNode::new_leaf(value.to_vec())));
                }
                
                branch_data.children[new_index] = Some(Box::new(new_ext));
            }
            
            ext.suffix_tree = Some(Box::new(branch));
        }
        
        Ok(())
    }
    
    fn is_node_empty(&self, node: &VerkleNode) -> bool {
        match &node.node_type {
            NodeType::Extension(ext) => ext.suffix_tree.is_none(),
            NodeType::Branch(branch) => {
                branch.value.is_none() && branch.children.iter().all(|c| c.is_none())
            }
            NodeType::Leaf(_) => false,
        }
    }
    
    fn evict_cache_entries(&self, cache: &mut HashMap<H256, Vec<u8>>) {
        // Simple FIFO eviction (in production, use LRU)
        let to_remove = cache.len() / 10; // Remove 10% of entries
        let keys: Vec<H256> = cache.keys().take(to_remove).cloned().collect();
        
        for key in keys {
            cache.remove(&key);
        }
    }
}

impl TreeMetrics {
    fn new() -> Self {
        Self {
            reads: std::sync::atomic::AtomicU64::new(0),
            writes: std::sync::atomic::AtomicU64::new(0),
            cache_hits: std::sync::atomic::AtomicU64::new(0),
            cache_misses: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TreeStats {
    pub reads: u64,
    pub writes: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_size: usize,
}

/// Account storage in Verkle tree
pub struct VerkleAccountStorage {
    tree: Arc<VerkleTree>,
}

impl VerkleAccountStorage {
    pub fn new(tree: VerkleTree) -> Self {
        Self {
            tree: Arc::new(tree),
        }
    }
    
    /// Get account state
    pub fn get_account(&self, address: &Address) -> Result<Option<AccountState>> {
        let key = self.account_key(address);
        
        if let Some(data) = self.tree.get(&key)? {
            Ok(Some(AccountState::decode(&data)?))
        } else {
            Ok(None)
        }
    }
    
    /// Set account state
    pub fn set_account(&self, address: &Address, state: &AccountState) -> Result<()> {
        let key = self.account_key(address);
        let data = state.encode()?;
        
        self.tree.insert(&key, &data)
    }
    
    /// Get storage value
    pub fn get_storage(&self, address: &Address, slot: &H256) -> Result<Option<H256>> {
        let key = self.storage_key(address, slot);
        
        if let Some(data) = self.tree.get(&key)? {
            Ok(Some(H256::from_slice(&data)))
        } else {
            Ok(None)
        }
    }
    
    /// Set storage value
    pub fn set_storage(&self, address: &Address, slot: &H256, value: &H256) -> Result<()> {
        let key = self.storage_key(address, slot);
        
        if value == &H256::zero() {
            self.tree.delete(&key)?;
        } else {
            self.tree.insert(&key, value.as_bytes())?;
        }
        
        Ok(())
    }
    
    fn account_key(&self, address: &Address) -> Vec<u8> {
        let mut key = vec![0u8; 32];
        key[0] = 0x00; // Account prefix
        key[1..21].copy_from_slice(address.as_bytes());
        key
    }
    
    fn storage_key(&self, address: &Address, slot: &H256) -> Vec<u8> {
        let mut key = vec![0u8; 32];
        key[0] = 0x01; // Storage prefix
        
        // Hash address and slot together
        let mut data = Vec::new();
        data.extend_from_slice(address.as_bytes());
        data.extend_from_slice(slot.as_bytes());
        
        let hash = ethereum_crypto::keccak256(&data);
        key[1..32].copy_from_slice(&hash[..31]);
        
        key
    }
}

#[derive(Debug, Clone)]
pub struct AccountState {
    pub nonce: U256,
    pub balance: U256,
    pub code_hash: H256,
}

impl AccountState {
    fn encode(&self) -> Result<Vec<u8>> {
        Ok(bincode::serialize(self)
            .map_err(|e| VerkleError::DatabaseError(e.to_string()))?)
    }
    
    fn decode(data: &[u8]) -> Result<Self> {
        bincode::deserialize(data)
            .map_err(|e| VerkleError::DatabaseError(e.to_string()))
    }
}