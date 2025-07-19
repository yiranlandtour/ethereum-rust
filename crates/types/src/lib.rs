pub mod address;
pub mod bloom;
pub mod bytes;
pub mod hash;
pub mod uint;

pub use address::Address;
pub use bloom::Bloom;
pub use bytes::Bytes;
pub use hash::{H160, H256, H512};
pub use uint::{U128, U256, U512};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TypesError {
    #[error("Invalid hex string: {0}")]
    InvalidHex(String),
    
    #[error("Invalid length: expected {expected}, got {actual}")]
    InvalidLength { expected: usize, actual: usize },
    
    #[error("Invalid address checksum")]
    InvalidChecksum,
    
    #[error("Overflow in arithmetic operation")]
    Overflow,
}

pub type Result<T> = std::result::Result<T, TypesError>;