use thiserror::Error;

pub mod node;
pub mod nibbles;
pub mod trie;
pub mod proof;

pub use node::*;
pub use nibbles::*;
pub use trie::*;
pub use proof::*;

#[derive(Debug, Error)]
pub enum TrieError {
    #[error("Key not found")]
    KeyNotFound,
    
    #[error("Invalid node encoding")]
    InvalidNode,
    
    #[error("Invalid proof")]
    InvalidProof,
    
    #[error("Storage error: {0}")]
    StorageError(#[from] ethereum_storage::StorageError),
    
    #[error("RLP error: {0}")]
    RlpError(#[from] ethereum_rlp::RlpError),
    
    #[error("Invalid key length")]
    InvalidKeyLength,
    
    #[error("Invalid nibbles")]
    InvalidNibbles,
}

pub type Result<T> = std::result::Result<T, TrieError>;