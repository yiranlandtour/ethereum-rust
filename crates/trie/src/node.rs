use ethereum_types::H256;
use ethereum_rlp::{Encode, Decode, Encoder, Decoder, RlpError};
use crate::{Nibbles, Result, TrieError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Empty,
    Leaf {
        key: Nibbles,
        value: Vec<u8>,
    },
    Extension {
        key: Nibbles,
        node: NodeRef,
    },
    Branch {
        children: [Option<NodeRef>; 16],
        value: Option<Vec<u8>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeRef {
    Hash(H256),
    Inline(Box<Node>),
}

impl Node {
    pub fn new_leaf(key: Nibbles, value: Vec<u8>) -> Self {
        Node::Leaf { key, value }
    }
    
    pub fn new_extension(key: Nibbles, node: NodeRef) -> Self {
        Node::Extension { key, node }
    }
    
    pub fn new_branch() -> Self {
        Node::Branch {
            children: Default::default(),
            value: None,
        }
    }
    
    pub fn is_empty(&self) -> bool {
        matches!(self, Node::Empty)
    }
    
    pub fn encode_raw(&self) -> Vec<u8> {
        let mut encoder = Encoder::new();
        match self {
            Node::Empty => {
                encoder.encode_bytes(&[]);
            }
            Node::Leaf { key, value } => {
                encoder.encode_list(2, |e| {
                    e.encode_bytes(&key.encode_compact(true));
                    e.encode_bytes(value);
                });
            }
            Node::Extension { key, node } => {
                encoder.encode_list(2, |e| {
                    e.encode_bytes(&key.encode_compact(false));
                    match node {
                        NodeRef::Hash(hash) => e.encode_bytes(hash.as_bytes()),
                        NodeRef::Inline(n) => {
                            let encoded = n.encode_raw();
                            e.encode_bytes(&encoded);
                        }
                    }
                });
            }
            Node::Branch { children, value } => {
                encoder.encode_list(17, |e| {
                    for child in children {
                        match child {
                            None => e.encode_bytes(&[]),
                            Some(NodeRef::Hash(hash)) => e.encode_bytes(hash.as_bytes()),
                            Some(NodeRef::Inline(n)) => {
                                let encoded = n.encode_raw();
                                e.encode_bytes(&encoded);
                            }
                        }
                    }
                    match value {
                        None => e.encode_bytes(&[]),
                        Some(v) => e.encode_bytes(v),
                    }
                });
            }
        }
        encoder.finish()
    }
    
    pub fn decode_raw(data: &[u8]) -> Result<Self> {
        if data.is_empty() {
            return Ok(Node::Empty);
        }
        
        let mut decoder = Decoder::new(data)?;
        let list_size = decoder.decode_list_header()?;
        
        match list_size {
            2 => {
                let key_data = decoder.decode_bytes()?;
                let (key, is_leaf) = Nibbles::decode_compact(&key_data)?;
                
                if is_leaf {
                    let value = decoder.decode_bytes()?;
                    Ok(Node::Leaf { key, value })
                } else {
                    let node_data = decoder.decode_bytes()?;
                    let node = if node_data.len() == 32 {
                        NodeRef::Hash(H256::from_slice(&node_data))
                    } else {
                        NodeRef::Inline(Box::new(Self::decode_raw(&node_data)?))
                    };
                    Ok(Node::Extension { key, node })
                }
            }
            17 => {
                let mut children: [Option<NodeRef>; 16] = Default::default();
                
                for i in 0..16 {
                    let child_data = decoder.decode_bytes()?;
                    if !child_data.is_empty() {
                        children[i] = Some(if child_data.len() == 32 {
                            NodeRef::Hash(H256::from_slice(&child_data))
                        } else {
                            NodeRef::Inline(Box::new(Self::decode_raw(&child_data)?))
                        });
                    }
                }
                
                let value_data = decoder.decode_bytes()?;
                let value = if value_data.is_empty() {
                    None
                } else {
                    Some(value_data)
                };
                
                Ok(Node::Branch { children, value })
            }
            _ => Err(TrieError::InvalidNode),
        }
    }
    
    pub fn hash(&self) -> H256 {
        let encoded = self.encode_raw();
        if encoded.len() < 32 {
            // Small nodes are stored inline
            let mut hash_data = [0u8; 32];
            hash_data[..encoded.len()].copy_from_slice(&encoded);
            H256::from(hash_data)
        } else {
            ethereum_crypto::keccak256(&encoded)
        }
    }
}

impl NodeRef {
    pub fn from_node(node: Node) -> Self {
        let encoded = node.encode_raw();
        if encoded.len() < 32 {
            NodeRef::Inline(Box::new(node))
        } else {
            NodeRef::Hash(node.hash())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_leaf_node_encoding() {
        let key = Nibbles::new(vec![1, 2, 3, 4]);
        let value = vec![5, 6, 7, 8];
        let node = Node::new_leaf(key.clone(), value.clone());
        
        let encoded = node.encode_raw();
        let decoded = Node::decode_raw(&encoded).unwrap();
        
        match decoded {
            Node::Leaf { key: k, value: v } => {
                assert_eq!(k, key);
                assert_eq!(v, value);
            }
            _ => panic!("Expected leaf node"),
        }
    }
    
    #[test]
    fn test_branch_node_encoding() {
        let mut node = Node::new_branch();
        if let Node::Branch { ref mut children, ref mut value } = node {
            children[0] = Some(NodeRef::Hash(H256::from_low_u64_be(123)));
            children[5] = Some(NodeRef::Inline(Box::new(Node::new_leaf(
                Nibbles::new(vec![1, 2]),
                vec![3, 4],
            ))));
            *value = Some(vec![9, 10]);
        }
        
        let encoded = node.encode_raw();
        let decoded = Node::decode_raw(&encoded).unwrap();
        
        assert_eq!(node, decoded);
    }
    
    #[test]
    fn test_extension_node_encoding() {
        let key = Nibbles::new(vec![1, 2, 3]);
        let child = Node::new_leaf(Nibbles::new(vec![4, 5]), vec![6, 7]);
        let node = Node::new_extension(key.clone(), NodeRef::Inline(Box::new(child)));
        
        let encoded = node.encode_raw();
        let decoded = Node::decode_raw(&encoded).unwrap();
        
        assert_eq!(node, decoded);
    }
}