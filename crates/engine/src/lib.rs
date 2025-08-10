pub mod api;
pub mod auth;
pub mod payload;
pub mod types;
pub mod forkchoice;
pub mod builder;

pub use api::{EngineApi, EngineApiServer};
pub use auth::{JwtAuth, JwtSecret};
pub use payload::{PayloadBuilder, PayloadAttributes};
pub use types::*;
pub use forkchoice::{ForkChoiceState, ForkChoiceUpdate};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Invalid JWT token")]
    InvalidJwt,
    
    #[error("Unauthorized")]
    Unauthorized,
    
    #[error("Invalid payload: {0}")]
    InvalidPayload(String),
    
    #[error("Unknown payload")]
    UnknownPayload,
    
    #[error("Invalid fork choice state: {0}")]
    InvalidForkChoiceState(String),
    
    #[error("Syncing")]
    Syncing,
    
    #[error("Invalid terminal block")]
    InvalidTerminalBlock,
    
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, EngineError>;