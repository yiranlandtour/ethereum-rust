pub mod prover;
pub mod verifier;
pub mod model;
pub mod oracle;
pub mod circuits;
pub mod marketplace;

pub use prover::{MLProver, ProofSystem, MLProof};
pub use verifier::{MLVerifier, VerificationResult};
pub use model::{MLModel, ModelFormat, ModelRegistry};
pub use oracle::{MLOracle, OracleRequest, OracleResponse};
pub use circuits::{MLCircuit, CircuitBuilder};
pub use marketplace::{ModelMarketplace, ModelListing};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ZkMLError {
    #[error("Proof generation failed: {0}")]
    ProofGenerationFailed(String),
    
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Model error: {0}")]
    ModelError(String),
    
    #[error("Oracle error: {0}")]
    OracleError(String),
    
    #[error("Circuit error: {0}")]
    CircuitError(String),
}

pub type Result<T> = std::result::Result<T, ZkMLError>;