pub mod executor;
pub mod scheduler;
pub mod conflict_detector;
pub mod state_manager;
pub mod dependency_graph;
pub mod validator;

pub use executor::{ParallelExecutor, ExecutionResult};
pub use scheduler::{TransactionScheduler, SchedulingStrategy};
pub use conflict_detector::{ConflictDetector, AccessSet};
pub use state_manager::{StateManager, StateSnapshot};
pub use dependency_graph::{DependencyGraph, TransactionNode};
pub use validator::{ParallelValidator, ValidationResult};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParallelExecutionError {
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Conflict detected: {0}")]
    ConflictDetected(String),
    
    #[error("Scheduling error: {0}")]
    SchedulingError(String),
    
    #[error("State error: {0}")]
    StateError(String),
    
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
}

pub type Result<T> = std::result::Result<T, ParallelExecutionError>;