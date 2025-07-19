use thiserror::Error;
use sha3::{Digest, Keccak256};
use ethereum_types::H256;

pub mod secp256k1_crypto;
pub use secp256k1_crypto::*;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Invalid public key")]
    InvalidPublicKey,
    
    #[error("Invalid private key")]
    InvalidPrivateKey,
    
    #[error("Secp256k1 error: {0}")]
    Secp256k1(#[from] secp256k1::Error),
}

pub type Result<T> = std::result::Result<T, CryptoError>;

/// Compute the Keccak-256 hash of the input data
pub fn keccak256(data: &[u8]) -> H256 {
    let mut hasher = Keccak256::new();
    hasher.update(data);
    let result = hasher.finalize();
    H256::from_slice(&result)
}

/// Compute the Keccak-256 hash of multiple slices of data
pub fn keccak256_concat(data: &[&[u8]]) -> H256 {
    let mut hasher = Keccak256::new();
    for slice in data {
        hasher.update(slice);
    }
    let result = hasher.finalize();
    H256::from_slice(&result)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_keccak256_empty() {
        let hash = keccak256(b"");
        let expected = "c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470";
        assert_eq!(format!("{:x}", hash), expected);
    }
    
    #[test]
    fn test_keccak256_hello_world() {
        let hash = keccak256(b"hello world");
        let expected = "47173285a8d7341e5e972fc677286384f802f8ef42a5ec5f03bbfa254cb01fad";
        assert_eq!(format!("{:x}", hash), expected);
    }
    
    #[test]
    fn test_keccak256_concat() {
        let hash1 = keccak256(b"helloworld");
        let hash2 = keccak256_concat(&[b"hello", b"world"]);
        assert_eq!(hash1, hash2);
    }
}