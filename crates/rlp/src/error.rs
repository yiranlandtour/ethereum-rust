use thiserror::Error;

#[derive(Debug, Error)]
pub enum RlpError {
    #[error("Decoder error: {0}")]
    Decoder(#[from] DecoderError),
    
    #[error("Encoder error: {0}")]
    Encoder(#[from] EncoderError),
}

#[derive(Debug, Error)]
pub enum DecoderError {
    #[error("Unexpected end of input")]
    UnexpectedEof,
    
    #[error("Invalid RLP data: {0}")]
    InvalidData(String),
    
    #[error("Integer overflow")]
    IntegerOverflow,
    
    #[error("Leading zeros in integer")]
    LeadingZeros,
    
    #[error("List length mismatch: expected {expected}, got {actual}")]
    ListLengthMismatch { expected: usize, actual: usize },
    
    #[error("String length mismatch: expected {expected}, got {actual}")]
    StringLengthMismatch { expected: usize, actual: usize },
    
    #[error("Invalid list prefix: {0}")]
    InvalidListPrefix(u8),
    
    #[error("Invalid string prefix: {0}")]
    InvalidStringPrefix(u8),
}

#[derive(Debug, Error)]
pub enum EncoderError {
    #[error("Integer overflow")]
    IntegerOverflow,
    
    #[error("Invalid data: {0}")]
    InvalidData(String),
}