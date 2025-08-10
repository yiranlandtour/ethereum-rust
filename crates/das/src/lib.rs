pub mod sampling;
pub mod reconstruction;
pub mod peer_das;
pub mod erasure;
pub mod distribution;

pub use sampling::{DataSampler, SampleRequest, SampleResponse};
pub use reconstruction::{DataReconstructor, ReconstructionResult};
pub use peer_das::{PeerDAS, DASConfig, DASStatus};
pub use erasure::{ErasureCoding, CodedData};
pub use distribution::{DataDistributor, DistributionStrategy};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DASError {
    #[error("Sampling failed: {0}")]
    SamplingFailed(String),
    
    #[error("Reconstruction failed: {0}")]
    ReconstructionFailed(String),
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
    
    #[error("Not enough samples: got {0}, need {1}")]
    InsufficientSamples(usize, usize),
    
    #[error("KZG error: {0}")]
    KzgError(String),
    
    #[error("Network error: {0}")]
    NetworkError(String),
}

pub type Result<T> = std::result::Result<T, DASError>;