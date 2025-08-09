use thiserror::Error;
use ethereum_types::{H256, U256};

pub mod traits;
pub mod memory;
pub mod rocksdb;

pub use traits::*;
pub use memory::*;
pub use rocksdb::*;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Key not found")]
    KeyNotFound,
    
    #[error("Database error: {0}")]
    DatabaseError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Key-value pair type alias
pub type KeyValue = (Vec<u8>, Vec<u8>);

/// Common key prefixes for different data types
#[derive(Debug, Clone, Copy)]
pub enum KeyPrefix {
    Header = 0x00,
    Body = 0x01,
    Receipt = 0x02,
    State = 0x03,
    Code = 0x04,
    Transaction = 0x05,
    CanonicalHash = 0x06,
    TotalDifficulty = 0x07,
}

impl KeyPrefix {
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }
    
    pub fn make_key(&self, suffix: &[u8]) -> Vec<u8> {
        let mut key = Vec::with_capacity(1 + suffix.len());
        key.push(self.as_byte());
        key.extend_from_slice(suffix);
        key
    }
}

/// Helper functions for encoding/decoding common key types
pub mod keys {
    use super::*;
    
    pub fn header_key(block_hash: &H256) -> Vec<u8> {
        KeyPrefix::Header.make_key(block_hash.as_bytes())
    }
    
    pub fn body_key(block_hash: &H256) -> Vec<u8> {
        KeyPrefix::Body.make_key(block_hash.as_bytes())
    }
    
    pub fn receipt_key(block_hash: &H256) -> Vec<u8> {
        KeyPrefix::Receipt.make_key(block_hash.as_bytes())
    }
    
    pub fn canonical_hash_key(block_number: u64) -> Vec<u8> {
        KeyPrefix::CanonicalHash.make_key(&block_number.to_be_bytes())
    }
    
    pub fn total_difficulty_key(block_hash: &H256) -> Vec<u8> {
        KeyPrefix::TotalDifficulty.make_key(block_hash.as_bytes())
    }
    
    pub fn state_key(address: &[u8], key: &H256) -> Vec<u8> {
        let mut result = Vec::with_capacity(1 + address.len() + 32);
        result.push(KeyPrefix::State.as_byte());
        result.extend_from_slice(address);
        result.extend_from_slice(key.as_bytes());
        result
    }
    
    pub fn code_key(code_hash: &H256) -> Vec<u8> {
        KeyPrefix::Code.make_key(code_hash.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_key_prefix() {
        let hash = H256::from_low_u64_be(123);
        let header_key = keys::header_key(&hash);
        assert_eq!(header_key[0], KeyPrefix::Header.as_byte());
        assert_eq!(&header_key[1..], hash.as_bytes());
    }
    
    #[test]
    fn test_canonical_hash_key() {
        let block_num = 12345u64;
        let key = keys::canonical_hash_key(block_num);
        assert_eq!(key[0], KeyPrefix::CanonicalHash.as_byte());
        assert_eq!(&key[1..], &block_num.to_be_bytes());
    }
}