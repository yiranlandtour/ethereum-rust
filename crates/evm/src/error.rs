use thiserror::Error;

pub type EvmResult<T> = Result<T, EvmError>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EvmError {
    #[error("Stack overflow")]
    StackOverflow,
    
    #[error("Stack underflow")]
    StackUnderflow,
    
    #[error("Invalid jump destination: {0}")]
    InvalidJump(usize),
    
    #[error("Invalid opcode: {0:#x}")]
    InvalidOpcode(u8),
    
    #[error("Out of gas")]
    OutOfGas,
    
    #[error("Insufficient balance")]
    InsufficientBalance,
    
    #[error("Contract creation failed")]
    ContractCreationFailed,
    
    #[error("Call depth exceeded")]
    CallDepthExceeded,
    
    #[error("Invalid memory access")]
    InvalidMemoryAccess,
    
    #[error("Write protection violation")]
    WriteProtection,
    
    #[error("Return data out of bounds")]
    ReturnDataOutOfBounds,
    
    #[error("Static call state modification")]
    StaticCallStateModification,
    
    #[error("Precompile failed: {0}")]
    PrecompileFailed(String),
    
    #[error("Invalid signature")]
    InvalidSignature,
    
    #[error("Code size exceeded")]
    CodeSizeExceeded,
    
    #[error("Invalid init code")]
    InvalidInitCode,
    
    #[error("Nonce overflow")]
    NonceOverflow,
    
    #[error("Invalid input")]
    InvalidInput,
}