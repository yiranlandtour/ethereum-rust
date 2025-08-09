use crate::{Result, TypesError, H160};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Address(H160);

impl Address {
    pub const ZERO: Address = Address(H160::zero());
    
    pub fn zero() -> Self {
        Self::ZERO
    }
    
    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() != 20 {
            return Err(TypesError::InvalidLength {
                expected: 20,
                actual: slice.len(),
            });
        }
        Ok(Address(H160::from_slice(slice)))
    }
    
    pub fn from_bytes(bytes: [u8; 20]) -> Self {
        Address(H160::from(bytes))
    }
    
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
    
    pub fn to_bytes(&self) -> [u8; 20] {
        self.0.to_fixed_bytes()
    }
    
    pub fn checksum(&self) -> String {
        let address_hex = hex::encode(self.0.as_bytes());
        let hash = Keccak256::digest(address_hex.as_bytes());
        
        let mut checksum = String::with_capacity(40);
        for (i, ch) in address_hex.chars().enumerate() {
            if ch.is_alphabetic() {
                let hash_byte = hash[i / 2];
                let hash_nibble = if i % 2 == 0 {
                    hash_byte >> 4
                } else {
                    hash_byte & 0xf
                };
                
                if hash_nibble >= 8 {
                    checksum.push(ch.to_ascii_uppercase());
                } else {
                    checksum.push(ch.to_ascii_lowercase());
                }
            } else {
                checksum.push(ch);
            }
        }
        
        format!("0x{}", checksum)
    }
    
    pub fn is_valid_checksum(s: &str) -> bool {
        match Self::from_str(s) {
            Ok(addr) => {
                let checksum = addr.checksum();
                s == checksum || s == checksum.to_lowercase()
            }
            Err(_) => false,
        }
    }
}

impl FromStr for Address {
    type Err = TypesError;
    
    fn from_str(s: &str) -> Result<Self> {
        let s = s.strip_prefix("0x").unwrap_or(s);
        
        if s.len() != 40 {
            return Err(TypesError::InvalidLength {
                expected: 40,
                actual: s.len(),
            });
        }
        
        let bytes = hex::decode(s).map_err(|_| TypesError::InvalidHex(s.to_string()))?;
        
        if bytes.len() != 20 {
            return Err(TypesError::InvalidLength {
                expected: 20,
                actual: bytes.len(),
            });
        }
        
        let mut array = [0u8; 20];
        array.copy_from_slice(&bytes);
        
        let addr = Address::from_bytes(array);
        
        if s.chars().any(|c| c.is_uppercase()) {
            if addr.checksum().strip_prefix("0x").unwrap() != s {
                return Err(TypesError::InvalidChecksum);
            }
        }
        
        Ok(addr)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.checksum())
    }
}

impl fmt::LowerHex for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0.as_bytes()))
    }
}

impl From<H160> for Address {
    fn from(hash: H160) -> Self {
        Address(hash)
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Address::from_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_address_checksum() {
        let addr = Address::from_str("0x5aAeb6053f3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        assert_eq!(
            addr.checksum(),
            "0x5aAeb6053f3E94C9b9A09f33669435E7Ef1BeAed"
        );
    }
    
    #[test]
    fn test_address_from_str() {
        let addr1 = Address::from_str("0x5aAeb6053f3E94C9b9A09f33669435E7Ef1BeAed").unwrap();
        let addr2 = Address::from_str("0x5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").unwrap();
        assert_eq!(addr1, addr2);
        
        assert!(Address::from_str("0x5aAeb6053f3E94C9b9A09f33669435E7Ef1BeAeD").is_err());
    }
    
    #[test]
    fn test_zero_address() {
        assert_eq!(
            Address::ZERO.to_string(),
            "0x0000000000000000000000000000000000000000"
        );
    }
}