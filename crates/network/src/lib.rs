use thiserror::Error;

pub mod rlpx;
pub mod discovery;
pub mod peer;
pub mod protocol;
pub mod messages;

pub use rlpx::*;
pub use discovery::*;
pub use peer::*;
pub use protocol::*;
pub use messages::*;

#[derive(Debug, Error)]
pub enum NetworkError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("RLP error: {0}")]
    RlpError(#[from] ethereum_rlp::RlpError),
    
    #[error("Handshake failed: {0}")]
    HandshakeFailed(String),
    
    #[error("Invalid message: {0}")]
    InvalidMessage(String),
    
    #[error("Peer disconnected: {0}")]
    PeerDisconnected(String),
    
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    
    #[error("Crypto error: {0}")]
    CryptoError(String),
    
    #[error("Timeout")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, NetworkError>;