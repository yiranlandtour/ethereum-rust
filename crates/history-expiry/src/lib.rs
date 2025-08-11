pub mod expiry_manager;
pub mod archival;
pub mod portal_integration;
pub mod retrieval;
pub mod pruning;

pub use expiry_manager::{HistoryExpiryManager, ExpiryConfig, ExpiryPolicy};
pub use archival::{ArchivalBackend, ArchivalStrategy};
pub use portal_integration::{PortalNetworkClient, HistoryDistribution};
pub use retrieval::{HistoryRetriever, RetrievalStrategy};
pub use pruning::{PruningEngine, PruningPolicy};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HistoryExpiryError {
    #[error("Expiry failed: {0}")]
    ExpiryFailed(String),
    
    #[error("Archival error: {0}")]
    ArchivalError(String),
    
    #[error("Retrieval error: {0}")]
    RetrievalError(String),
    
    #[error("Portal network error: {0}")]
    PortalNetworkError(String),
    
    #[error("Pruning error: {0}")]
    PruningError(String),
}

pub type Result<T> = std::result::Result<T, HistoryExpiryError>;