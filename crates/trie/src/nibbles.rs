use crate::{Result, TrieError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nibbles {
    data: Vec<u8>,
}

impl Nibbles {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut nibbles = Vec::with_capacity(bytes.len() * 2);
        for byte in bytes {
            nibbles.push(byte >> 4);
            nibbles.push(byte & 0x0f);
        }
        Self { data: nibbles }
    }
    
    pub fn from_hex(hex: &str) -> Result<Self> {
        let hex = hex.trim_start_matches("0x");
        if hex.len() % 2 != 0 {
            return Err(TrieError::InvalidNibbles);
        }
        
        let bytes = hex::decode(hex)
            .map_err(|_| TrieError::InvalidNibbles)?;
        Ok(Self::from_bytes(&bytes))
    }
    
    pub fn to_bytes(&self) -> Vec<u8> {
        if self.data.len() % 2 != 0 {
            panic!("Nibbles must have even length to convert to bytes");
        }
        
        let mut bytes = Vec::with_capacity(self.data.len() / 2);
        for i in (0..self.data.len()).step_by(2) {
            bytes.push((self.data[i] << 4) | self.data[i + 1]);
        }
        bytes
    }
    
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    pub fn get(&self, index: usize) -> Option<u8> {
        self.data.get(index).copied()
    }
    
    pub fn slice(&self, start: usize, end: usize) -> Self {
        Self {
            data: self.data[start..end].to_vec(),
        }
    }
    
    pub fn slice_from(&self, start: usize) -> Self {
        Self {
            data: self.data[start..].to_vec(),
        }
    }
    
    pub fn common_prefix_len(&self, other: &Self) -> usize {
        self.data
            .iter()
            .zip(other.data.iter())
            .take_while(|(a, b)| a == b)
            .count()
    }
    
    pub fn push(&mut self, nibble: u8) {
        assert!(nibble < 16, "Nibble must be less than 16");
        self.data.push(nibble);
    }
    
    pub fn extend(&mut self, other: &Self) {
        self.data.extend_from_slice(&other.data);
    }
    
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
    
    pub fn encode_compact(&self, is_leaf: bool) -> Vec<u8> {
        let mut encoded = Vec::new();
        let odd_len = self.data.len() % 2 != 0;
        
        // First nibble: flags
        let flags = match (is_leaf, odd_len) {
            (false, false) => 0x00,
            (false, true) => 0x10,
            (true, false) => 0x20,
            (true, true) => 0x30,
        };
        
        if odd_len {
            encoded.push(flags | self.data[0]);
            for i in (1..self.data.len()).step_by(2) {
                encoded.push((self.data[i] << 4) | self.data.get(i + 1).unwrap_or(&0));
            }
        } else {
            encoded.push(flags);
            for i in (0..self.data.len()).step_by(2) {
                encoded.push((self.data[i] << 4) | self.data.get(i + 1).unwrap_or(&0));
            }
        }
        
        encoded
    }
    
    pub fn decode_compact(encoded: &[u8]) -> Result<(Self, bool)> {
        if encoded.is_empty() {
            return Err(TrieError::InvalidNibbles);
        }
        
        let flags = encoded[0];
        let is_leaf = (flags & 0x20) != 0;
        let odd_len = (flags & 0x10) != 0;
        
        let mut nibbles = Vec::new();
        
        if odd_len {
            nibbles.push(flags & 0x0f);
        }
        
        for byte in &encoded[1..] {
            nibbles.push(byte >> 4);
            nibbles.push(byte & 0x0f);
        }
        
        // Remove padding if necessary
        if !odd_len && nibbles.len() > 0 && nibbles[nibbles.len() - 1] == 0 {
            nibbles.pop();
        }
        
        Ok((Self { data: nibbles }, is_leaf))
    }
}

impl From<Vec<u8>> for Nibbles {
    fn from(data: Vec<u8>) -> Self {
        Self::new(data)
    }
}

impl AsRef<[u8]> for Nibbles {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_nibbles_from_bytes() {
        let bytes = vec![0xab, 0xcd];
        let nibbles = Nibbles::from_bytes(&bytes);
        assert_eq!(nibbles.as_slice(), &[0xa, 0xb, 0xc, 0xd]);
    }
    
    #[test]
    fn test_nibbles_to_bytes() {
        let nibbles = Nibbles::new(vec![0xa, 0xb, 0xc, 0xd]);
        let bytes = nibbles.to_bytes();
        assert_eq!(bytes, vec![0xab, 0xcd]);
    }
    
    #[test]
    fn test_common_prefix() {
        let n1 = Nibbles::new(vec![1, 2, 3, 4, 5]);
        let n2 = Nibbles::new(vec![1, 2, 3, 6, 7]);
        assert_eq!(n1.common_prefix_len(&n2), 3);
    }
    
    #[test]
    fn test_compact_encoding_even_extension() {
        let nibbles = Nibbles::new(vec![1, 2, 3, 4]);
        let encoded = nibbles.encode_compact(false);
        assert_eq!(encoded, vec![0x00, 0x12, 0x34]);
        
        let (decoded, is_leaf) = Nibbles::decode_compact(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(!is_leaf);
    }
    
    #[test]
    fn test_compact_encoding_odd_extension() {
        let nibbles = Nibbles::new(vec![1, 2, 3]);
        let encoded = nibbles.encode_compact(false);
        assert_eq!(encoded, vec![0x11, 0x23]);
        
        let (decoded, is_leaf) = Nibbles::decode_compact(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(!is_leaf);
    }
    
    #[test]
    fn test_compact_encoding_even_leaf() {
        let nibbles = Nibbles::new(vec![1, 2, 3, 4]);
        let encoded = nibbles.encode_compact(true);
        assert_eq!(encoded, vec![0x20, 0x12, 0x34]);
        
        let (decoded, is_leaf) = Nibbles::decode_compact(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(is_leaf);
    }
    
    #[test]
    fn test_compact_encoding_odd_leaf() {
        let nibbles = Nibbles::new(vec![1, 2, 3]);
        let encoded = nibbles.encode_compact(true);
        assert_eq!(encoded, vec![0x31, 0x23]);
        
        let (decoded, is_leaf) = Nibbles::decode_compact(&encoded).unwrap();
        assert_eq!(decoded, nibbles);
        assert!(is_leaf);
    }
}