use crate::{Result, TypesError};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::ops::{BitOr, BitOrAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bloom([u8; 256]);

impl Serialize for Bloom {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(&self.0)
    }
}

impl<'de> Deserialize<'de> for Bloom {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = <Vec<u8>>::deserialize(deserializer)?;
        if bytes.len() != 256 {
            return Err(serde::de::Error::custom(format!(
                "Invalid Bloom filter length: expected 256, got {}",
                bytes.len()
            )));
        }
        let mut array = [0u8; 256];
        array.copy_from_slice(&bytes);
        Ok(Bloom(array))
    }
}

impl Bloom {
    pub const ZERO: Bloom = Bloom([0u8; 256]);
    
    pub fn new() -> Self {
        Self::ZERO
    }
    
    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 256 {
            return Err(TypesError::InvalidLength {
                expected: 256,
                actual: slice.len(),
            });
        }
        
        let mut bloom = Self::new();
        bloom.0.copy_from_slice(slice);
        Ok(bloom)
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
    
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
    
    pub fn contains(&self, other: &Bloom) -> bool {
        for i in 0..256 {
            if (self.0[i] & other.0[i]) != other.0[i] {
                return false;
            }
        }
        true
    }
    
    pub fn set(&mut self, index: usize) {
        if index < 2048 {
            let byte_index = index / 8;
            let bit_index = index % 8;
            self.0[byte_index] |= 1 << bit_index;
        }
    }
    
    pub fn is_set(&self, index: usize) -> bool {
        if index < 2048 {
            let byte_index = index / 8;
            let bit_index = index % 8;
            (self.0[byte_index] & (1 << bit_index)) != 0
        } else {
            false
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

impl Default for Bloom {
    fn default() -> Self {
        Self::new()
    }
}

impl BitOr for Bloom {
    type Output = Self;
    
    fn bitor(mut self, rhs: Self) -> Self::Output {
        self |= rhs;
        self
    }
}

impl BitOrAssign for Bloom {
    fn bitor_assign(&mut self, rhs: Self) {
        for i in 0..256 {
            self.0[i] |= rhs.0[i];
        }
    }
}

impl fmt::LowerHex for Bloom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(&self.0))
    }
}

impl From<[u8; 256]> for Bloom {
    fn from(bytes: [u8; 256]) -> Self {
        Bloom(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_set_and_check() {
        let mut bloom = Bloom::new();
        bloom.set(42);
        bloom.set(100);
        bloom.set(255);
        
        assert!(bloom.is_set(42));
        assert!(bloom.is_set(100));
        assert!(bloom.is_set(255));
        assert!(!bloom.is_set(43));
    }
    
    #[test]
    fn test_bloom_contains() {
        let mut bloom1 = Bloom::new();
        bloom1.set(10);
        bloom1.set(20);
        bloom1.set(30);
        
        let mut bloom2 = Bloom::new();
        bloom2.set(10);
        bloom2.set(20);
        
        assert!(bloom1.contains(&bloom2));
        assert!(!bloom2.contains(&bloom1));
    }
    
    #[test]
    fn test_bloom_or() {
        let mut bloom1 = Bloom::new();
        bloom1.set(10);
        
        let mut bloom2 = Bloom::new();
        bloom2.set(20);
        
        let bloom3 = bloom1 | bloom2;
        assert!(bloom3.is_set(10));
        assert!(bloom3.is_set(20));
    }
}