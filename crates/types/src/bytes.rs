use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Deref;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Bytes(Vec<u8>);

impl Bytes {
    pub fn new() -> Self {
        Bytes(Vec::new())
    }
    
    pub fn from_vec(vec: Vec<u8>) -> Self {
        Bytes(vec)
    }
    
    pub fn from_slice(slice: &[u8]) -> Self {
        Bytes(slice.to_vec())
    }
    
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }
    
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }
    
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.0.len()
    }
    
    pub fn push(&mut self, byte: u8) {
        self.0.push(byte);
    }
    
    pub fn extend_from_slice(&mut self, slice: &[u8]) {
        self.0.extend_from_slice(slice);
    }
}

impl Deref for Bytes {
    type Target = [u8];
    
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(vec: Vec<u8>) -> Self {
        Bytes::from_vec(vec)
    }
}

impl From<&[u8]> for Bytes {
    fn from(slice: &[u8]) -> Self {
        Bytes::from_slice(slice)
    }
}

impl From<&str> for Bytes {
    fn from(s: &str) -> Self {
        Bytes::from_slice(s.as_bytes())
    }
}

impl fmt::LowerHex for Bytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bytes_creation() {
        let bytes1 = Bytes::from_vec(vec![1, 2, 3]);
        let bytes2 = Bytes::from_slice(&[1, 2, 3]);
        let bytes3 = Bytes::from("abc");
        
        assert_eq!(bytes1.as_slice(), &[1, 2, 3]);
        assert_eq!(bytes2.as_slice(), &[1, 2, 3]);
        assert_eq!(bytes3.as_slice(), b"abc");
    }
    
    #[test]
    fn test_bytes_hex() {
        let bytes = Bytes::from_vec(vec![0x12, 0x34, 0x56]);
        assert_eq!(format!("{:x}", bytes), "0x123456");
    }
}