pub mod compiler;
pub mod backend;
pub mod cache;
pub mod optimizer;
pub mod runtime;

pub use compiler::{JitCompiler, CompilationResult};
pub use backend::{Backend, CraneliftBackend, ExecutionContext};
pub use cache::{CodeCache, CacheKey};
pub use optimizer::{Optimizer, OptimizationLevel};
pub use runtime::{JitRuntime, JitConfig};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum JitError {
    #[error("Compilation failed: {0}")]
    CompilationFailed(String),
    
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    
    #[error("Invalid bytecode: {0}")]
    InvalidBytecode(String),
    
    #[error("Cache error: {0}")]
    CacheError(String),
    
    #[error("Backend error: {0}")]
    BackendError(String),
}

pub type Result<T> = std::result::Result<T, JitError>;