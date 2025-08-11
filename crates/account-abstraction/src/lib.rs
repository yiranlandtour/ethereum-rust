pub mod smart_account;
pub mod ai_simulator;
pub mod quantum_signatures;
pub mod session_keys;
pub mod biometric_auth;
pub mod migration;
pub mod bundler;
pub mod paymaster;

pub use smart_account::{SmartAccount, AccountFactory, AccountConfig};
pub use ai_simulator::{AITransactionSimulator, SimulationResult};
pub use quantum_signatures::{QuantumSigner, PostQuantumAlgorithm};
pub use session_keys::{SessionKey, SessionKeyManager};
pub use biometric_auth::{BiometricAuthenticator, BiometricType};
pub use migration::{AccountMigrator, MigrationStrategy};
pub use bundler::{UserOpBundler, BundleResult};
pub use paymaster::{Paymaster, PaymasterPolicy};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AccountAbstractionError {
    #[error("Account error: {0}")]
    AccountError(String),
    
    #[error("Simulation failed: {0}")]
    SimulationFailed(String),
    
    #[error("Signature error: {0}")]
    SignatureError(String),
    
    #[error("Session key error: {0}")]
    SessionKeyError(String),
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Migration error: {0}")]
    MigrationError(String),
    
    #[error("Bundler error: {0}")]
    BundlerError(String),
}

pub type Result<T> = std::result::Result<T, AccountAbstractionError>;