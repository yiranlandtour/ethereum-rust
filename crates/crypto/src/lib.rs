use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Invalid public key")]
    InvalidPublicKey,
    
    #[error("Invalid private key")]
    InvalidPrivateKey,
}

pub type Result<T> = std::result::Result<T, CryptoError>;