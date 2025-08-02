use crate::error::{EvmError, EvmResult};
use ethereum_types::U256;

const STACK_LIMIT: usize = 1024;

#[derive(Debug, Clone)]
pub struct Stack {
    data: Vec<U256>,
}

impl Stack {
    pub fn new() -> Self {
        Self {
            data: Vec::with_capacity(32),
        }
    }

    pub fn push(&mut self, value: U256) -> EvmResult<()> {
        if self.data.len() >= STACK_LIMIT {
            return Err(EvmError::StackOverflow);
        }
        self.data.push(value);
        Ok(())
    }

    pub fn pop(&mut self) -> EvmResult<U256> {
        self.data.pop().ok_or(EvmError::StackUnderflow)
    }

    pub fn peek(&self, index: usize) -> EvmResult<&U256> {
        if index >= self.data.len() {
            return Err(EvmError::StackUnderflow);
        }
        Ok(&self.data[self.data.len() - 1 - index])
    }

    pub fn peek_mut(&mut self, index: usize) -> EvmResult<&mut U256> {
        let len = self.data.len();
        if index >= len {
            return Err(EvmError::StackUnderflow);
        }
        Ok(&mut self.data[len - 1 - index])
    }

    pub fn swap(&mut self, n: usize) -> EvmResult<()> {
        let len = self.data.len();
        if n >= len {
            return Err(EvmError::StackUnderflow);
        }
        self.data.swap(len - 1, len - 1 - n);
        Ok(())
    }

    pub fn dup(&mut self, n: usize) -> EvmResult<()> {
        if n >= self.data.len() {
            return Err(EvmError::StackUnderflow);
        }
        if self.data.len() >= STACK_LIMIT {
            return Err(EvmError::StackOverflow);
        }
        let value = self.data[self.data.len() - 1 - n];
        self.data.push(value);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn require(&self, n: usize) -> EvmResult<()> {
        if self.data.len() < n {
            Err(EvmError::StackUnderflow)
        } else {
            Ok(())
        }
    }

    pub fn limit_check(&self, n: usize) -> EvmResult<()> {
        if self.data.len() + n > STACK_LIMIT {
            Err(EvmError::StackOverflow)
        } else {
            Ok(())
        }
    }
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut stack = Stack::new();
        assert!(stack.push(U256::from(42)).is_ok());
        assert_eq!(stack.pop().unwrap(), U256::from(42));
        assert!(stack.pop().is_err());
    }

    #[test]
    fn test_stack_overflow() {
        let mut stack = Stack::new();
        for i in 0..STACK_LIMIT {
            assert!(stack.push(U256::from(i)).is_ok());
        }
        assert!(stack.push(U256::zero()).is_err());
    }

    #[test]
    fn test_dup() {
        let mut stack = Stack::new();
        stack.push(U256::from(1)).unwrap();
        stack.push(U256::from(2)).unwrap();
        stack.push(U256::from(3)).unwrap();
        
        stack.dup(1).unwrap();
        assert_eq!(stack.pop().unwrap(), U256::from(2));
        assert_eq!(stack.pop().unwrap(), U256::from(3));
        assert_eq!(stack.pop().unwrap(), U256::from(2));
        assert_eq!(stack.pop().unwrap(), U256::from(1));
    }

    #[test]
    fn test_swap() {
        let mut stack = Stack::new();
        stack.push(U256::from(1)).unwrap();
        stack.push(U256::from(2)).unwrap();
        stack.push(U256::from(3)).unwrap();
        
        stack.swap(1).unwrap();
        assert_eq!(stack.pop().unwrap(), U256::from(2));
        assert_eq!(stack.pop().unwrap(), U256::from(3));
        assert_eq!(stack.pop().unwrap(), U256::from(1));
    }
}