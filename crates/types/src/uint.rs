use primitive_types::{U128 as PrimitiveU128, U256 as PrimitiveU256, U512 as PrimitiveU512};

pub type U128 = PrimitiveU128;
pub type U256 = PrimitiveU256;
pub type U512 = PrimitiveU512;

pub trait UintExt: Sized {
    fn from_be_bytes_vec(bytes: Vec<u8>) -> Self;
    fn to_be_bytes_vec(&self) -> Vec<u8>;
}

impl UintExt for U256 {
    fn from_be_bytes_vec(bytes: Vec<u8>) -> Self {
        let mut array = [0u8; 32];
        let len = std::cmp::min(bytes.len(), 32);
        let offset = 32 - len;
        array[offset..].copy_from_slice(&bytes[..len]);
        U256::from_big_endian(&array)
    }
    
    fn to_be_bytes_vec(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 32];
        self.to_big_endian(&mut bytes);
        
        let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap_or(31);
        bytes[first_non_zero..].to_vec()
    }
}

impl UintExt for U128 {
    fn from_be_bytes_vec(bytes: Vec<u8>) -> Self {
        let mut array = [0u8; 16];
        let len = std::cmp::min(bytes.len(), 16);
        let offset = 16 - len;
        array[offset..].copy_from_slice(&bytes[..len]);
        U128::from_big_endian(&array)
    }
    
    fn to_be_bytes_vec(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 16];
        self.to_big_endian(&mut bytes);
        
        let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap_or(15);
        bytes[first_non_zero..].to_vec()
    }
}

impl UintExt for U512 {
    fn from_be_bytes_vec(bytes: Vec<u8>) -> Self {
        let mut array = [0u8; 64];
        let len = std::cmp::min(bytes.len(), 64);
        let offset = 64 - len;
        array[offset..].copy_from_slice(&bytes[..len]);
        U512::from_big_endian(&array)
    }
    
    fn to_be_bytes_vec(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 64];
        self.to_big_endian(&mut bytes);
        
        let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap_or(63);
        bytes[first_non_zero..].to_vec()
    }
}

pub const MAX_U256: U256 = U256::MAX;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_u256_from_be_bytes() {
        let bytes = vec![0x12, 0x34, 0x56, 0x78];
        let u = U256::from_be_bytes_vec(bytes);
        assert_eq!(u, U256::from(0x12345678u64));
    }
    
    #[test]
    fn test_u256_to_be_bytes() {
        let u = U256::from(0x12345678u64);
        let bytes = u.to_be_bytes_vec();
        assert_eq!(bytes, vec![0x12, 0x34, 0x56, 0x78]);
    }
    
    #[test]
    fn test_u256_zero() {
        let u = U256::zero();
        let bytes = u.to_be_bytes_vec();
        assert_eq!(bytes, vec![0]);
    }
}