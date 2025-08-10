pub mod relay;
pub mod builder;
pub mod proposer;
pub mod auction;
pub mod bundle;
pub mod flashbots;

pub use relay::{Relay, RelayClient, RelayInfo};
pub use builder::{BlockBuilder, BuilderApi, BuilderConfig};
pub use proposer::{ProposerApi, ProposerRegistry, ValidatorPreferences};
pub use auction::{Auction, AuctionResult, Bid};
pub use bundle::{Bundle, BundleTransaction, BundlePool};
pub use flashbots::{FlashbotsClient, FlashbotsBundle};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MevError {
    #[error("Invalid bid: {0}")]
    InvalidBid(String),
    
    #[error("Invalid bundle: {0}")]
    InvalidBundle(String),
    
    #[error("Relay error: {0}")]
    RelayError(String),
    
    #[error("Builder error: {0}")]
    BuilderError(String),
    
    #[error("Auction failed: {0}")]
    AuctionFailed(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
    
    #[error("Signature error: {0}")]
    SignatureError(String),
}

pub type Result<T> = std::result::Result<T, MevError>;