use crate::error::{EvmError, EvmResult};
use ethereum_types::U256;

#[derive(Debug, Clone, Default)]
pub struct Memory {
    data: Vec<u8>,
}

impl Memory {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn resize(&mut self, new_size: usize) {
        if new_size > self.data.len() {
            self.data.resize(new_size, 0);
        }
    }

    pub fn get(&self, offset: usize, size: usize) -> Vec<u8> {
        if size == 0 {
            return Vec::new();
        }

        let end = offset.saturating_add(size);
        if offset >= self.data.len() {
            vec![0; size]
        } else if end > self.data.len() {
            let mut result = self.data[offset..].to_vec();
            result.resize(size, 0);
            result
        } else {
            self.data[offset..end].to_vec()
        }
    }

    pub fn get_u256(&self, offset: usize) -> U256 {
        let data = self.get(offset, 32);
        U256::from_big_endian(&data)
    }

    pub fn set(&mut self, offset: usize, data: &[u8]) -> EvmResult<()> {
        if data.is_empty() {
            return Ok(());
        }

        let end = offset.checked_add(data.len())
            .ok_or(EvmError::InvalidMemoryAccess)?;
        
        self.resize(end);
        self.data[offset..end].copy_from_slice(data);
        Ok(())
    }

    pub fn set_u256(&mut self, offset: usize, value: U256) -> EvmResult<()> {
        let mut data = [0u8; 32];
        value.to_big_endian(&mut data);
        self.set(offset, &data)
    }

    pub fn set_byte(&mut self, offset: usize, byte: u8) -> EvmResult<()> {
        let end = offset.checked_add(1)
            .ok_or(EvmError::InvalidMemoryAccess)?;
        
        self.resize(end);
        self.data[offset] = byte;
        Ok(())
    }

    pub fn copy_within(&mut self, dst: usize, src: usize, len: usize) -> EvmResult<()> {
        if len == 0 {
            return Ok(());
        }

        let src_end = src.checked_add(len)
            .ok_or(EvmError::InvalidMemoryAccess)?;
        let dst_end = dst.checked_add(len)
            .ok_or(EvmError::InvalidMemoryAccess)?;
        
        self.resize(src_end.max(dst_end));
        
        let data = self.get(src, len);
        self.data[dst..dst_end].copy_from_slice(&data);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn effective_len(&self) -> U256 {
        U256::from(self.data.len())
    }

    pub fn required_size(&self, offset: U256, size: U256) -> Option<U256> {
        if size.is_zero() {
            return Some(U256::zero());
        }
        
        offset.checked_add(size).map(|end| {
            let rounded = (end + 31) / 32 * 32;
            rounded
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_resize() {
        let mut memory = Memory::new();
        memory.resize(32);
        assert_eq!(memory.len(), 32);
        assert_eq!(memory.data, vec![0; 32]);
    }

    #[test]
    fn test_memory_set_get() {
        let mut memory = Memory::new();
        let data = vec![1, 2, 3, 4, 5];
        memory.set(10, &data).unwrap();
        
        assert_eq!(memory.get(10, 5), data);
        assert_eq!(memory.get(8, 9), vec![0, 0, 1, 2, 3, 4, 5, 0, 0]);
    }

    #[test]
    fn test_memory_u256() {
        let mut memory = Memory::new();
        let value = U256::from(0x1234567890abcdef_u64);
        memory.set_u256(0, value).unwrap();
        
        assert_eq!(memory.get_u256(0), value);
    }

    #[test]
    fn test_memory_copy() {
        let mut memory = Memory::new();
        let data = vec![1, 2, 3, 4, 5];
        memory.set(0, &data).unwrap();
        memory.copy_within(10, 0, 5).unwrap();
        
        assert_eq!(memory.get(10, 5), data);
    }
}