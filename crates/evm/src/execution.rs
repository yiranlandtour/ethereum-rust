use ethereum_types::{Address, H256, U256};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub caller: Address,
    pub address: Address,
    pub origin: Address,
    pub value: U256,
    pub code: Vec<u8>,
    pub data: Vec<u8>,
    pub gas_price: U256,
    pub gas_limit: u64,
    pub block: BlockContext,
    pub is_static: bool,
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct BlockContext {
    pub coinbase: Address,
    pub number: U256,
    pub timestamp: U256,
    pub difficulty: U256,
    pub gas_limit: U256,
    pub base_fee: Option<U256>,
    pub chain_id: U256,
    pub block_hashes: Vec<H256>,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub gas_used: u64,
    pub gas_refund: u64,
    pub return_data: Vec<u8>,
    pub logs: Vec<Log>,
    pub created_address: Option<Address>,
    pub accessed_addresses: HashSet<Address>,
    pub accessed_storage_keys: HashSet<(Address, H256)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Success,
    Revert,
    Halt(HaltReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HaltReason {
    OutOfGas,
    InvalidOpcode(u8),
    StackUnderflow,
    StackOverflow,
    InvalidJump,
    CallDepthExceeded,
    CreateCollision,
    CreateContractTooLarge,
    PrecompileFailed,
    StateModificationInStatic,
    InvalidCode,
}

#[derive(Debug, Clone)]
pub struct Log {
    pub address: Address,
    pub topics: Vec<H256>,
    pub data: Vec<u8>,
}

impl ExecutionContext {
    pub fn new(
        caller: Address,
        address: Address,
        value: U256,
        code: Vec<u8>,
        data: Vec<u8>,
        gas_limit: u64,
        block: BlockContext,
    ) -> Self {
        Self {
            caller,
            address,
            origin: caller,
            value,
            code,
            data,
            gas_price: U256::zero(),
            gas_limit,
            block,
            is_static: false,
            depth: 0,
        }
    }

    pub fn is_create(&self) -> bool {
        self.address == Address::from_bytes([0u8; 20])
    }

    pub fn with_static(&self) -> Self {
        let mut ctx = self.clone();
        ctx.is_static = true;
        ctx
    }

    pub fn with_depth(&self, depth: u32) -> Self {
        let mut ctx = self.clone();
        ctx.depth = depth;
        ctx
    }
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self {
            status: ExecutionStatus::Success,
            gas_used: 0,
            gas_refund: 0,
            return_data: Vec::new(),
            logs: Vec::new(),
            created_address: None,
            accessed_addresses: HashSet::new(),
            accessed_storage_keys: HashSet::new(),
        }
    }
}

impl ExecutionResult {
    pub fn success(return_data: Vec<u8>, gas_used: u64) -> Self {
        Self {
            status: ExecutionStatus::Success,
            gas_used,
            return_data,
            ..Default::default()
        }
    }

    pub fn revert(return_data: Vec<u8>, gas_used: u64) -> Self {
        Self {
            status: ExecutionStatus::Revert,
            gas_used,
            return_data,
            ..Default::default()
        }
    }

    pub fn halt(reason: HaltReason, gas_used: u64) -> Self {
        Self {
            status: ExecutionStatus::Halt(reason),
            gas_used,
            ..Default::default()
        }
    }
}