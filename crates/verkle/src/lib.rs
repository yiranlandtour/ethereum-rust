pub mod tree;
pub mod node;
pub mod commitment;
pub mod migration;
pub mod proof;
pub mod witness;

pub use tree::{VerkleTree, VerkleConfig};
pub use node::{VerkleNode, NodeType, Extension, Branch};
pub use commitment::{Commitment, IPAProof, VerkleCommitment};
pub use migration::{StateMigrator, MigrationStrategy, MigrationStatus};
pub use proof::{VerkleProof, ProofVerifier};
pub use witness::{VerkleWitness, WitnessBuilder};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum VerkleError {
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    
    #[error("Invalid proof: {0}")]
    InvalidProof(String),
    
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    
    #[error("Migration failed: {0}")]
    MigrationFailed(String),
    
    #[error("Commitment error: {0}")]
    CommitmentError(String),
    
    #[error("Database error: {0}")]
    DatabaseError(String),
}

pub type Result<T> = std::result::Result<T, VerkleError>;