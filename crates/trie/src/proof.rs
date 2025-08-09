use ethereum_types::H256;
use crate::{Node, NodeRef, Nibbles, Result, TrieError};

pub struct MerkleProof {
    pub nodes: Vec<Vec<u8>>,
}

impl MerkleProof {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
    
    pub fn add_node(&mut self, encoded: Vec<u8>) {
        self.nodes.push(encoded);
    }
    
    pub fn verify(&self, root_hash: &H256, key: &[u8], expected_value: Option<&[u8]>) -> Result<bool> {
        if self.nodes.is_empty() {
            return Ok(false);
        }
        
        let nibbles = Nibbles::from_bytes(key);
        let first_node = Node::decode_raw(&self.nodes[0])?;
        
        // Verify the root hash matches
        if first_node.hash() != *root_hash {
            return Ok(false);
        }
        
        // Traverse the proof path
        let value = self.verify_at_node(&first_node, &nibbles, 0, 1)?;
        
        match (value, expected_value) {
            (Some(v), Some(exp)) => Ok(v == exp),
            (None, None) => Ok(true),
            _ => Ok(false),
        }
    }
    
    fn verify_at_node(
        &self,
        node: &Node,
        key: &Nibbles,
        key_index: usize,
        proof_index: usize,
    ) -> Result<Option<Vec<u8>>> {
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
                    let child = self.resolve_proof_node(child_ref, proof_index)?;
                    self.verify_at_node(&child, key, key_index + common_len, proof_index + 1)
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
                            let child = self.resolve_proof_node(child_ref, proof_index)?;
                            self.verify_at_node(&child, key, key_index + 1, proof_index + 1)
                        }
                    }
                }
            }
        }
    }
    
    fn resolve_proof_node(&self, node_ref: &NodeRef, proof_index: usize) -> Result<Node> {
        match node_ref {
            NodeRef::Inline(node) => Ok((**node).clone()),
            NodeRef::Hash(_) => {
                if proof_index >= self.nodes.len() {
                    return Err(TrieError::InvalidProof);
                }
                Node::decode_raw(&self.nodes[proof_index])
            }
        }
    }
}

pub fn generate_proof<F>(
    root: &Node,
    key: &[u8],
    load_node: F,
) -> Result<MerkleProof>
where
    F: Fn(&H256) -> Result<Vec<u8>>,
{
    let mut proof = MerkleProof::new();
    let nibbles = Nibbles::from_bytes(key);
    
    collect_proof_nodes(root, &nibbles, 0, &mut proof, &load_node)?;
    
    Ok(proof)
}

fn collect_proof_nodes<F>(
    node: &Node,
    key: &Nibbles,
    key_index: usize,
    proof: &mut MerkleProof,
    load_node: &F,
) -> Result<()>
where
    F: Fn(&H256) -> Result<Vec<u8>>,
{
    proof.add_node(node.encode_raw());
    
    match node {
        Node::Empty | Node::Leaf { .. } => Ok(()),
        
        Node::Extension { key: ext_key, node: child_ref } => {
            let remaining_key = key.slice_from(key_index);
            let common_len = ext_key.common_prefix_len(&remaining_key);
            
            if common_len == ext_key.len() {
                match child_ref {
                    NodeRef::Inline(child) => {
                        collect_proof_nodes(child, key, key_index + common_len, proof, load_node)
                    }
                    NodeRef::Hash(hash) => {
                        let child_data = load_node(hash)?;
                        let child = Node::decode_raw(&child_data)?;
                        collect_proof_nodes(&child, key, key_index + common_len, proof, load_node)
                    }
                }
            } else {
                Ok(())
            }
        }
        
        Node::Branch { children, .. } => {
            if key_index < key.len() {
                let nibble = key.get(key_index).unwrap() as usize;
                if let Some(child_ref) = &children[nibble] {
                    match child_ref {
                        NodeRef::Inline(child) => {
                            collect_proof_nodes(child, key, key_index + 1, proof, load_node)
                        }
                        NodeRef::Hash(hash) => {
                            let child_data = load_node(hash)?;
                            let child = Node::decode_raw(&child_data)?;
                            collect_proof_nodes(&child, key, key_index + 1, proof, load_node)
                        }
                    }
                }
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PatriciaTrie;
    use ethereum_storage::MemoryDatabase;
    use std::sync::Arc;
    
    #[test]
    fn test_proof_generation_and_verification() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db.clone());
        
        // Insert test data
        trie.insert(b"test1", vec![1, 2, 3]).unwrap();
        trie.insert(b"test2", vec![4, 5, 6]).unwrap();
        trie.insert(b"test3", vec![7, 8, 9]).unwrap();
        
        let root_hash = trie.commit().unwrap();
        
        // Generate proof for existing key
        let proof = generate_proof(&trie.root, b"test2", |hash| {
            let key = vec![b't', hash.as_bytes().to_vec()].concat();
            db.get(&key).map(|opt| opt.unwrap_or_default())
        }).unwrap();
        
        // Verify proof
        assert!(proof.verify(&root_hash, b"test2", Some(&[4, 5, 6])).unwrap());
        assert!(!proof.verify(&root_hash, b"test2", Some(&[1, 2, 3])).unwrap());
        assert!(!proof.verify(&root_hash, b"test4", None).unwrap());
    }
    
    #[test]
    fn test_proof_for_non_existent_key() {
        let db = Arc::new(MemoryDatabase::new());
        let mut trie = PatriciaTrie::new(db.clone());
        
        trie.insert(b"test1", vec![1, 2, 3]).unwrap();
        trie.insert(b"test3", vec![7, 8, 9]).unwrap();
        
        let root_hash = trie.commit().unwrap();
        
        // Generate proof for non-existent key
        let proof = generate_proof(&trie.root, b"test2", |hash| {
            let key = vec![b't', hash.as_bytes().to_vec()].concat();
            db.get(&key).map(|opt| opt.unwrap_or_default())
        }).unwrap();
        
        // Verify proof shows non-existence
        assert!(proof.verify(&root_hash, b"test2", None).unwrap());
    }
}