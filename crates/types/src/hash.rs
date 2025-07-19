use primitive_types::{H160 as PrimitiveH160, H256 as PrimitiveH256, H512 as PrimitiveH512};

pub type H160 = PrimitiveH160;
pub type H256 = PrimitiveH256;
pub type H512 = PrimitiveH512;

pub trait HashExt {
    fn from_slice(slice: &[u8]) -> Self;
}

impl HashExt for H160 {
    fn from_slice(slice: &[u8]) -> Self {
        let mut hash = H160::zero();
        let len = std::cmp::min(slice.len(), 20);
        hash.as_bytes_mut()[..len].copy_from_slice(&slice[..len]);
        hash
    }
}

impl HashExt for H256 {
    fn from_slice(slice: &[u8]) -> Self {
        let mut hash = H256::zero();
        let len = std::cmp::min(slice.len(), 32);
        hash.as_bytes_mut()[..len].copy_from_slice(&slice[..len]);
        hash
    }
}

impl HashExt for H512 {
    fn from_slice(slice: &[u8]) -> Self {
        let mut hash = H512::zero();
        let len = std::cmp::min(slice.len(), 64);
        hash.as_bytes_mut()[..len].copy_from_slice(&slice[..len]);
        hash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hash_from_slice() {
        let data = vec![1u8; 100];
        
        let h160 = H160::from_slice(&data);
        assert_eq!(h160.as_bytes()[0], 1);
        assert_eq!(h160.as_bytes()[19], 1);
        
        let h256 = H256::from_slice(&data);
        assert_eq!(h256.as_bytes()[0], 1);
        assert_eq!(h256.as_bytes()[31], 1);
        
        let h512 = H512::from_slice(&data);
        assert_eq!(h512.as_bytes()[0], 1);
        assert_eq!(h512.as_bytes()[63], 1);
    }
    
    #[test]
    fn test_hash_from_short_slice() {
        let data = vec![0xffu8; 10];
        
        let h160 = H160::from_slice(&data);
        assert_eq!(h160.as_bytes()[9], 0xff);
        assert_eq!(h160.as_bytes()[10], 0);
    }
}