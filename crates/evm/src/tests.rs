#[cfg(test)]
mod tests {
    use crate::{
        execution::{BlockContext, ExecutionContext},
        Evm,
    };
    use ethereum_types::{Address, U256};

    fn create_test_context() -> ExecutionContext {
        let block = BlockContext {
            coinbase: Address::from_bytes([0u8; 20]),
            number: U256::from(1),
            timestamp: U256::from(1000),
            difficulty: U256::from(1000000),
            gas_limit: U256::from(10000000),
            base_fee: Some(U256::from(1000)),
            chain_id: U256::from(1),
            block_hashes: vec![],
        };

        ExecutionContext::new(
            Address::from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]).unwrap(),
            Address::from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]).unwrap(),
            U256::zero(),
            vec![],
            vec![],
            1000000,
            block,
        )
    }

    #[test]
    fn test_simple_addition() {
        let mut evm = Evm::new();
        let mut context = create_test_context();
        
        // PUSH1 0x02, PUSH1 0x03, ADD, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        context.code = vec![
            0x60, 0x02,  // PUSH1 0x02
            0x60, 0x03,  // PUSH1 0x03
            0x01,        // ADD
            0x60, 0x00,  // PUSH1 0x00
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 0x20
            0x60, 0x00,  // PUSH1 0x00
            0xf3,        // RETURN
        ];

        let result = evm.execute(context).unwrap();
        assert_eq!(result.return_data.len(), 32);
        assert_eq!(U256::from(&result.return_data[..]), U256::from(5));
    }

    #[test]
    fn test_storage_operations() {
        let mut evm = Evm::new();
        let mut context = create_test_context();
        
        // PUSH1 0x42, PUSH1 0x01, SSTORE, PUSH1 0x01, SLOAD, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        context.code = vec![
            0x60, 0x42,  // PUSH1 0x42
            0x60, 0x01,  // PUSH1 0x01
            0x55,        // SSTORE
            0x60, 0x01,  // PUSH1 0x01
            0x54,        // SLOAD
            0x60, 0x00,  // PUSH1 0x00
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 0x20
            0x60, 0x00,  // PUSH1 0x00
            0xf3,        // RETURN
        ];

        let result = evm.execute(context).unwrap();
        assert_eq!(result.return_data.len(), 32);
        assert_eq!(U256::from(&result.return_data[..]), U256::from(0x42));
    }

    #[test]
    fn test_jumps() {
        let mut evm = Evm::new();
        let mut context = create_test_context();
        
        // PUSH1 0x08, JUMP, PUSH1 0x00, PUSH1 0x00, REVERT, JUMPDEST, PUSH1 0x01, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        context.code = vec![
            0x60, 0x08,  // PUSH1 0x08
            0x56,        // JUMP
            0x60, 0x00,  // PUSH1 0x00
            0x60, 0x00,  // PUSH1 0x00
            0xfd,        // REVERT
            0x5b,        // JUMPDEST
            0x60, 0x01,  // PUSH1 0x01
            0x60, 0x00,  // PUSH1 0x00
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 0x20
            0x60, 0x00,  // PUSH1 0x00
            0xf3,        // RETURN
        ];

        let result = evm.execute(context).unwrap();
        assert_eq!(result.return_data.len(), 32);
        assert_eq!(U256::from(&result.return_data[..]), U256::from(1));
    }

    #[test]
    fn test_conditional_jump() {
        let mut evm = Evm::new();
        let mut context = create_test_context();
        
        // PUSH1 0x01, PUSH1 0x0a, JUMPI, PUSH1 0x00, PUSH1 0x00, REVERT, JUMPDEST, PUSH1 0x42, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        context.code = vec![
            0x60, 0x01,  // PUSH1 0x01
            0x60, 0x0a,  // PUSH1 0x0a
            0x57,        // JUMPI
            0x60, 0x00,  // PUSH1 0x00
            0x60, 0x00,  // PUSH1 0x00
            0xfd,        // REVERT
            0x5b,        // JUMPDEST
            0x60, 0x42,  // PUSH1 0x42
            0x60, 0x00,  // PUSH1 0x00
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 0x20
            0x60, 0x00,  // PUSH1 0x00
            0xf3,        // RETURN
        ];

        let result = evm.execute(context).unwrap();
        assert_eq!(result.return_data.len(), 32);
        assert_eq!(U256::from(&result.return_data[..]), U256::from(0x42));
    }

    #[test]
    fn test_stack_operations() {
        let mut evm = Evm::new();
        let mut context = create_test_context();
        
        // PUSH1 0x01, PUSH1 0x02, DUP2, ADD, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        context.code = vec![
            0x60, 0x01,  // PUSH1 0x01
            0x60, 0x02,  // PUSH1 0x02
            0x81,        // DUP2
            0x01,        // ADD
            0x60, 0x00,  // PUSH1 0x00
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 0x20
            0x60, 0x00,  // PUSH1 0x00
            0xf3,        // RETURN
        ];

        let result = evm.execute(context).unwrap();
        assert_eq!(result.return_data.len(), 32);
        assert_eq!(U256::from(&result.return_data[..]), U256::from(3));
    }
}