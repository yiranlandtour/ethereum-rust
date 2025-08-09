use thiserror::Error;

pub mod server;
pub mod types;
pub mod methods;
pub mod eth;
pub mod net;
pub mod web3;

pub use server::*;
pub use types::*;
pub use methods::*;

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("Invalid request")]
    InvalidRequest,
    
    #[error("Method not found: {0}")]
    MethodNotFound(String),
    
    #[error("Invalid params: {0}")]
    InvalidParams(String),
    
    #[error("Internal error: {0}")]
    InternalError(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Resource not found")]
    ResourceNotFound,
}

impl RpcError {
    pub fn code(&self) -> i32 {
        match self {
            RpcError::InvalidRequest => -32600,
            RpcError::MethodNotFound(_) => -32601,
            RpcError::InvalidParams(_) => -32602,
            RpcError::InternalError(_) => -32603,
            RpcError::ParseError(_) => -32700,
            RpcError::ResourceNotFound => -32001,
        }
    }
}

pub type Result<T> = std::result::Result<T, RpcError>;
