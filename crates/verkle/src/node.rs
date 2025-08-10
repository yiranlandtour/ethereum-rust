use serde::{Deserialize, Serialize};
use ethereum_types::H256;

use crate::commitment::Commitment;

/// Verkle tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerkleNode {
    pub node_type: NodeType,
    pub commitment: Commitment,
    pub depth: u32,
}

impl VerkleNode {
    pub fn new_extension(stem: Vec<u8>) -> Self {
        Self {
            node_type: NodeType::Extension(Extension {
                stem,
                suffix_tree: None,
            }),
            commitment: Commitment::default(),
            depth: 0,
        }
    }
    
    pub fn new_branch() -> Self {
        Self {
            node_type: NodeType::Branch(Branch {
                children: vec![None; 256],
                value: None,
            }),
            commitment: Commitment::default(),
            depth: 0,
        }
    }
    
    pub fn new_leaf(value: Vec<u8>) -> Self {
        Self {
            node_type: NodeType::Leaf(value),
            commitment: Commitment::default(),
            depth: 0,
        }
    }
    
    pub fn is_leaf(&self) -> bool {
        matches!(self.node_type, NodeType::Leaf(_))
    }
    
    pub fn is_branch(&self) -> bool {
        matches!(self.node_type, NodeType::Branch(_))
    }
    
    pub fn is_extension(&self) -> bool {
        matches!(self.node_type, NodeType::Extension(_))
    }
}

/// Node types in Verkle tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Extension(Extension),
    Branch(Branch),
    Leaf(Vec<u8>),
}

/// Extension node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Extension {
    /// The stem (partial key)
    pub stem: Vec<u8>,
    /// Suffix tree (child node)
    pub suffix_tree: Option<Box<VerkleNode>>,
}

impl Extension {
    pub fn new(stem: Vec<u8>) -> Self {
        Self {
            stem,
            suffix_tree: None,
        }
    }
    
    pub fn with_suffix(mut self, suffix: VerkleNode) -> Self {
        self.suffix_tree = Some(Box::new(suffix));
        self
    }
}

/// Branch node with 256 children
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// Children nodes (256-way branching)
    pub children: Vec<Option<Box<VerkleNode>>>,
    /// Optional value at this branch
    pub value: Option<Vec<u8>>,
}

impl Branch {
    pub fn new() -> Self {
        Self {
            children: vec![None; 256],
            value: None,
        }
    }
    
    pub fn set_child(&mut self, index: u8, child: VerkleNode) {
        self.children[index as usize] = Some(Box::new(child));
    }
    
    pub fn get_child(&self, index: u8) -> Option<&VerkleNode> {
        self.children[index as usize].as_ref().map(|b| b.as_ref())
    }
    
    pub fn get_child_mut(&mut self, index: u8) -> Option<&mut VerkleNode> {
        self.children[index as usize].as_mut().map(|b| b.as_mut())
    }
    
    pub fn remove_child(&mut self, index: u8) -> Option<Box<VerkleNode>> {
        self.children[index as usize].take()
    }
    
    pub fn child_count(&self) -> usize {
        self.children.iter().filter(|c| c.is_some()).count()
    }
    
    pub fn is_empty(&self) -> bool {
        self.value.is_none() && self.child_count() == 0
    }
}

/// Verkle node path for traversal
#[derive(Debug, Clone)]
pub struct NodePath {
    pub nodes: Vec<(VerkleNode, usize)>,
    pub key: Vec<u8>,
}

impl NodePath {
    pub fn new(key: Vec<u8>) -> Self {
        Self {
            nodes: Vec::new(),
            key,
        }
    }
    
    pub fn push(&mut self, node: VerkleNode, depth: usize) {
        self.nodes.push((node, depth));
    }
    
    pub fn pop(&mut self) -> Option<(VerkleNode, usize)> {
        self.nodes.pop()
    }
    
    pub fn depth(&self) -> usize {
        self.nodes.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}