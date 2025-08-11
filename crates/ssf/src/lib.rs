pub mod finality;
pub mod aggregator;
pub mod validator;
pub mod committee;
pub mod signature_aggregation;

pub use finality::{SingleSlotFinality, FinalityConfig, FinalityStatus};
pub use aggregator::{SignatureAggregator, AggregationStrategy};
pub use validator::{SSFValidator, ValidatorSet};
pub use committee::{Committee, CommitteeSelection};
pub use signature_aggregation::{BLSAggregation, AggregatedSignature};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SSFError {
    #[error("Finality failed: {0}")]
    FinalityFailed(String),
    
    #[error("Aggregation error: {0}")]
    AggregationError(String),
    
    #[error("Validator error: {0}")]
    ValidatorError(String),
    
    #[error("Committee error: {0}")]
    CommitteeError(String),
    
    #[error("Signature error: {0}")]
    SignatureError(String),
}

pub type Result<T> = std::result::Result<T, SSFError>;