use ethereum_types::H256;
use ethereum_storage::{Database, WriteBatch};
use std::sync::Arc;
use crate::{Node, NodeRef, Nibbles, Result, TrieError};

pub struct PatriciaTrie<D: Database> {
    db: Arc<D>,
    root: Node,
    root_hash: Option<H256>,
}

impl<D: Database> PatriciaTrie<D> {
    pub fn new(db: Arc<D>) -> Self {
        Self {
            db,
            root: Node::Empty,
            root_hash: None,
        }
    }
    
    pub fn new_with_root(db: Arc<D>, root_hash: H256) -> Result<Self> {
        let root = Self::load_node(&*db, &root_hash)?;
        Ok(Self {
            db,
            root,
            root_hash: Some(root_hash),
        })
    }
    
    pub fn root_hash(&mut self) -> H256 {
        if self.root_hash.is_none() {
            self.root_hash = Some(self.root.hash());
        }
        self.root_hash.unwrap()
    }
    
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let nibbles = Nibbles::from_bytes(key);
        self.get_at_node(&self.root, &nibbles, 0)
    }
    
    fn get_at_node(&self, node: &Node, key: &Nibbles, key_index: usize) -> Result<Option<Vec<u8>>> {
        match node {
            Node::Empty => Ok(None),
            
            Node::Leaf { key: leaf_key, value } => {
                let remaining_key = key.slice_from(key_index);
                if remaining_key == *leaf_key {
                    Ok(Some(value.clone()))
                } else {
                    Ok(None)
                }
            }
            
            Node::Extension { key: ext_key, node: child_ref } => {
                let remaining_key = key.slice_from(key_index);
                let common_len = ext_key.common_prefix_len(&remaining_key);
                
                if common_len == ext_key.len() {
                    let child = self.resolve_node_ref(child_ref)?;
                    self.get_at_node(&child, key, key_index + common_len)
                } else {
                    Ok(None)
                }
            }
            
            Node::Branch { children, value } => {
                if key_index == key.len() {
                    Ok(value.clone())
                } else {
                    let nibble = key.get(key_index).unwrap() as usize;
                    match &children[nibble] {
                        None => Ok(None),
                        Some(child_ref) => {
                            let child = self.resolve_node_ref(child_ref)?;
                            self.get_at_node(&child, key, key_index + 1)
                        }
                    }
                }
            }
        }
    }
    
    pub fn insert(&mut self, key: &[u8], value: Vec<u8>) -> Result<()> {
        let nibbles = Nibbles::from_bytes(key);
        self.root = self.insert_at_node(self.root.clone(), &nibbles, 0, value)?;
        self.root_hash = None; // Invalidate cached hash
        Ok(())
    }
    
    fn insert_at_node(&mut self, node: Node, key: &Nibbles, key_index: usize, value: Vec<u8>) -> Result<Node> {
        match node {
            Node::Empty => {
                Ok(Node::Leaf {
                    key: key.slice_from(key_index),
                    value,
                })
            }
            
            Node::Leaf { key: leaf_key, value: leaf_value } => {
                let remaining_key = key.slice_from(key_index);
                let common_len = leaf_key.common_prefix_len(&remaining_key);
                
                if common_len == leaf_key.len() && common_len == remaining_key.len() {
                    // Same key, update value
                    Ok(Node::Leaf {
                        key: leaf_key,
                        value,
                    })
                } else if common_len == 0 {
                    // No common prefix, create branch
                    let mut branch = Node::new_branch();
                    if let Node::Branch { ref mut children, ref mut value: branch_value } = branch {
                        // Insert existing leaf
                        if leaf_key.is_empty() {
                            *branch_value = Some(leaf_value);
                        } else {
                            let nibble = leaf_key.get(0).unwrap() as usize;
                            let new_leaf = Node::Leaf {
                                key: leaf_key.slice_from(1),
                                value: leaf_value,
                            };
                            children[nibble] = Some(NodeRef::from_node(new_leaf));
                        }
                        
                        // Insert new value
                        if remaining_key.is_empty() {
                            *branch_value = Some(value);
                        } else {
                            let nibble = remaining_key.get(0).unwrap() as usize;
                            let new_leaf = Node::Leaf {
                                key: remaining_key.slice_from(1),
                                value,
                            };
                            children[nibble] = Some(NodeRef::from_node(new_leaf));
                        }
                    }
                    Ok(branch)
                } else {
                    // Some common prefix, create extension + branch
                    let common_prefix = leaf_key.slice(0, common_len);
                    let mut branch = Node::new_branch();
                    
                    if let Node::Branch { ref mut children, ref mut value: branch_value } = branch {
                        // Insert existing leaf remainder
                        if common_len < leaf_key.len() {
                            let nibble = leaf_key.get(common_len).unwrap() as usize;
                            let new_leaf = Node::Leaf {
                                key: leaf_key.slice_from(common_len + 1),
                                value: leaf_value,
                            };
                            children[nibble] = Some(NodeRef::from_node(new_leaf));
                        } else {
                            *branch_value = Some(leaf_value);
                        }
                        
                        // Insert new value remainder
                        if common_len < remaining_key.len() {
                            let nibble = remaining_key.get(common_len).unwrap() as usize;
                            let new_leaf = Node::Leaf {
                                key: remaining_key.slice_from(common_len + 1),
                                value,
                            };
                            children[nibble] = Some(NodeRef::from_node(new_leaf));
                        } else {
                            *branch_value = Some(value);
                        }
                    }
                    
                    if common_len > 0 {
                        Ok(Node::Extension {
                            key: common_prefix,
                            node: NodeRef::from_node(branch),
                        })
                    } else {
                        Ok(branch)
                    }
                }
            }
            
            Node::Extension { key: ext_key, node: child_ref } => {
                let remaining_key = key.slice_from(key_index);
                let common_len = ext_key.common_prefix_len(&remaining_key);
                
                if common_len == ext_key.len() {
                    // Full match, recurse into child
                    let child = self.resolve_node_ref(&child_ref)?;
                    let new_child = self.insert_at_node(child, key, key_index + common_len, value)?;
                    Ok(Node::Extension {
                        key: ext_key,
                        node: NodeRef::from_node(new_child),
                    })
                } else {
                    // Partial match, split extension
                    let common_prefix = ext_key.slice(0, common_len);
                    let ext_remainder = ext_key.slice_from(common_len);
                    let key_remainder = remaining_key.slice_from(common_len);
                    
                    let mut branch = Node::new_branch();
                    if let Node::Branch { ref mut children, ref mut value: branch_value } = branch {
                        // Insert existing extension remainder
                        if !ext_remainder.is_empty() {
                            let nibble = ext_remainder.get(0).unwrap() as usize;
                            if ext_remainder.len() == 1 {
                                children[nibble] = Some(child_ref);
                            } else {
                                let new_ext = Node::Extension {
                                    key: ext_remainder.slice_from(1),
                                    node: child_ref,
                                };
                                children[nibble] = Some(NodeRef::from_node(new_ext));
                            }
                        }
                        
                        // Insert new value
                        if key_remainder.is_empty() {
                            *branch_value = Some(value);
                        } else {
                            let nibble = key_remainder.get(0).unwrap() as usize;
                            let new_leaf = Node::Leaf {
                                key: key_remainder.slice_from(1),
                                value,
                            };
                            children[nibble] = Some(NodeRef::from_node(new_leaf));
                        }
                    }
                    
                    if common_len > 0 {
                        Ok(Node::Extension {
                            key: common_prefix,
                            node: NodeRef::from_node(branch),
                        })
                    } else {
                        Ok(branch)
                    }
                }
            }
            
            Node::Branch { mut children, mut value: branch_value } => {
                if key_index == key.len() {
                    // Insert at branch value
                    branch_value = Some(value);
                } else {
                    let nibble = key.get(key_index).unwrap() as usize;
                    let child = match children[nibble].take() {
                        None => Node::Empty,
                        Some(ref child_ref) => self.resolve_node_ref(child_ref)?,
                    };
                    let new_child = self.insert_at_node(child, key, key_index + 1, value)?;
                    children[nibble] = Some(NodeRef::from_node(new_child));
                }
                Ok(Node::Branch { children, value: branch_value })
            }
        }
    }
    
    pub fn delete(&mut self, key: &[u8]) -> Result<bool> {
        let nibbles = Nibbles::from_bytes(key);
        let (new_root, deleted) = self.delete_at_node(self.root.clone(), &nibbles, 0)?;
        self.root = new_root;
        self.root_hash = None; // Invalidate cached hash
        Ok(deleted)
    }
    
    fn delete_at_node(&mut self, node: Node, key: &Nibbles, key_index: usize) -> Result<(Node, bool)> {
        match node {
            Node::Empty => Ok((Node::Empty, false)),
            
            Node::Leaf { key: leaf_key, .. } => {
                let remaining_key = key.slice_from(key_index);
                if remaining_key == leaf_key {
                    Ok((Node::Empty, true))
                } else {
                    Ok((node, false))
                }
            }
            
            Node::Extension { key: ext_key, node: child_ref } => {
                let remaining_key = key.slice_from(key_index);
                let common_len = ext_key.common_prefix_len(&remaining_key);
                
                if common_len == ext_key.len() {
                    let child = self.resolve_node_ref(&child_ref)?;
                    let (new_child, deleted) = self.delete_at_node(child, key, key_index + common_len)?;
                    
                    if deleted {
                        // Try to collapse extension
                        match new_child {
                            Node::Empty => Ok((Node::Empty, true)),
                            Node::Leaf { key: child_key, value } => {
                                let mut combined_key = ext_key.clone();
                                combined_key.extend(&child_key);
                                Ok((Node::Leaf { key: combined_key, value }, true))
                            }
                            Node::Extension { key: child_key, node } => {
                                let mut combined_key = ext_key.clone();
                                combined_key.extend(&child_key);
                                Ok((Node::Extension { key: combined_key, node }, true))
                            }
                            _ => Ok((Node::Extension {
                                key: ext_key,
                                node: NodeRef::from_node(new_child),
                            }, true))
                        }
                    } else {
                        Ok((node, false))
                    }
                } else {
                    Ok((node, false))
                }
            }
            
            Node::Branch { mut children, mut value } => {
                if key_index == key.len() {
                    if value.is_some() {
                        value = None;
                        let compacted = self.try_compact_branch(children, value)?;
                        Ok((compacted, true))
                    } else {
                        Ok((node, false))
                    }
                } else {
                    let nibble = key.get(key_index).unwrap() as usize;
                    if let Some(child_ref) = &children[nibble] {
                        let child = self.resolve_node_ref(child_ref)?;
                        let (new_child, deleted) = self.delete_at_node(child, key, key_index + 1)?;
                        
                        if deleted {
                            if matches!(new_child, Node::Empty) {
                                children[nibble] = None;
                            } else {
                                children[nibble] = Some(NodeRef::from_node(new_child));
                            }
                            let compacted = self.try_compact_branch(children, value)?;
                            Ok((compacted, true))
                        } else {
                            Ok((node, false))
                        }
                    } else {
                        Ok((node, false))
                    }
                }
            }
        }
    }
    
    fn try_compact_branch(&self, children: [Option<NodeRef>; 16], value: Option<Vec<u8>>) -> Result<Node> {
        let non_empty_children: Vec<(usize, &NodeRef)> = children
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|r| (i, r)))
            .collect();
        
        match (non_empty_children.len(), value) {
            (0, None) => Ok(Node::Empty),
            (0, Some(v)) => Ok(Node::Leaf {
                key: Nibbles::new(vec![]),
                value: v,
            }),
            (1, None) => {
                let (nibble, child_ref) = non_empty_children[0];
                let child = self.resolve_node_ref(child_ref)?;
                match child {
                    Node::Leaf { key: child_key, value } => {
                        let mut new_key = Nibbles::new(vec![nibble as u8]);
                        new_key.extend(&child_key);
                        Ok(Node::Leaf { key: new_key, value })
                    }
                    Node::Extension { key: child_key, node } => {
                        let mut new_key = Nibbles::new(vec![nibble as u8]);
                        new_key.extend(&child_key);
                        Ok(Node::Extension { key: new_key, node })
                    }
                    _ => Ok(Node::Extension {
                        key: Nibbles::new(vec![nibble as u8]),
                        node: child_ref.clone(),
                    })
                }
            }
            _ => Ok(Node::Branch { children, value })
        }
    }
    
    fn resolve_node_ref(&self, node_ref: &NodeRef) -> Result<Node> {
        match node_ref {
            NodeRef::Inline(node) => Ok((**node).clone()),
            NodeRef::Hash(hash) => Self::load_node(&*self.db, hash),
        }
    }
    
    fn load_node(db: &D, hash: &H256) -> Result<Node> {
        let key = Self::node_key(hash);
        let data = db.get(&key)?
            .ok_or(TrieError::KeyNotFound)?;
        Node::decode_raw(&data)
    }
    
    fn node_key(hash: &H256) -> Vec<u8> {
        let mut key = vec![b't']; // 't' for trie node
        key.extend_from_slice(hash.as_bytes());
        key
    }
    
    pub fn commit(&mut self) -> Result<H256> {
        let mut batch = self.db.batch();
        self.commit_node(&self.root, &mut *batch)?;
        self.db.write_batch(batch)?;
        Ok(self.root_hash())
    }
    
    fn commit_node(&self, node: &Node, batch: &mut dyn WriteBatch) -> Result<()> {
        let encoded = node.encode_raw();
        if encoded.len() >= 32 {
            let hash = node.hash();
            let key = Self::node_key(&hash);
            batch.put(&key, &encoded);
        }
        
        match node {
            Node::Extension { node: child_ref, .. } => {
                if let NodeRef::Inline(child) = child_ref {
                    self.commit_node(child, batch)?;
                }
            }
            Node::Branch { children, .. } => {
                for child_ref in children.iter().flatten() {
                    if let NodeRef::Inline(child) = child_ref {
                        self.commit_node(child, batch)?;
                    }
                }
            }
            _ => {}
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethereum_storage::MemoryDatabase;
    
    #[test]
    fn test_empty_trie() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db);
        
        assert_eq!(trie.get(b"test").unwrap(), None);
        assert_eq!(
            trie.root_hash(),
            H256::from_slice(&ethereum_crypto::keccak256(&[]).as_bytes())
        );
    }
    
    #[test]
    fn test_single_insert() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db);
        
        trie.insert(b"test", vec![1, 2, 3]).unwrap();
        assert_eq!(trie.get(b"test").unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(trie.get(b"test2").unwrap(), None);
    }
    
    #[test]
    fn test_multiple_inserts() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db);
        
        trie.insert(b"test", vec![1, 2, 3]).unwrap();
        trie.insert(b"test2", vec![4, 5, 6]).unwrap();
        trie.insert(b"test3", vec![7, 8, 9]).unwrap();
        
        assert_eq!(trie.get(b"test").unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(trie.get(b"test2").unwrap(), Some(vec![4, 5, 6]));
        assert_eq!(trie.get(b"test3").unwrap(), Some(vec![7, 8, 9]));
    }
    
    #[test]
    fn test_update() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db);
        
        trie.insert(b"test", vec![1, 2, 3]).unwrap();
        assert_eq!(trie.get(b"test").unwrap(), Some(vec![1, 2, 3]));
        
        trie.insert(b"test", vec![4, 5, 6]).unwrap();
        assert_eq!(trie.get(b"test").unwrap(), Some(vec![4, 5, 6]));
    }
    
    #[test]
    fn test_delete() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db);
        
        trie.insert(b"test", vec![1, 2, 3]).unwrap();
        trie.insert(b"test2", vec![4, 5, 6]).unwrap();
        
        assert!(trie.delete(b"test").unwrap());
        assert_eq!(trie.get(b"test").unwrap(), None);
        assert_eq!(trie.get(b"test2").unwrap(), Some(vec![4, 5, 6]));
        
        assert!(!trie.delete(b"test").unwrap());
    }
    
    #[test]
    fn test_commit_and_reload() {
        let db = Arc::new(MemoryDatabase::new());
        let root_hash = {
            let mut trie = PatriciaTrie::new(db.clone());
            
            trie.insert(b"test", vec![1, 2, 3]).unwrap();
            trie.insert(b"test2", vec![4, 5, 6]).unwrap();
            trie.insert(b"test3", vec![7, 8, 9]).unwrap();
            
            trie.commit().unwrap()
        };
        
        let trie2 = PatriciaTrie::new_with_root(db, root_hash).unwrap();
        assert_eq!(trie2.get(b"test").unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(trie2.get(b"test2").unwrap(), Some(vec![4, 5, 6]));
        assert_eq!(trie2.get(b"test3").unwrap(), Some(vec![7, 8, 9]));
    }
}