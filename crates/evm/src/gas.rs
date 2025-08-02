use crate::error::{EvmError, EvmResult};
use ethereum_types::U256;

#[derive(Debug, Clone, Copy)]
pub struct Gas {
    limit: u64,
    used: u64,
}

impl Gas {
    pub fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    pub fn consume(&mut self, amount: u64) -> EvmResult<()> {
        let new_used = self.used.saturating_add(amount);
        if new_used > self.limit {
            Err(EvmError::OutOfGas)
        } else {
            self.used = new_used;
            Ok(())
        }
    }

    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn limit(&self) -> u64 {
        self.limit
    }

    pub fn refund(&mut self, amount: u64) {
        self.used = self.used.saturating_sub(amount);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GasCost;

impl GasCost {
    pub const ZERO: u64 = 0;
    pub const BASE: u64 = 2;
    pub const VERYLOW: u64 = 3;
    pub const LOW: u64 = 5;
    pub const MID: u64 = 8;
    pub const HIGH: u64 = 10;
    pub const EXTCODE: u64 = 2600;
    pub const BALANCE: u64 = 2600;
    pub const SLOAD: u64 = 2100;
    pub const JUMPDEST: u64 = 1;
    pub const SSET: u64 = 20000;
    pub const SRESET: u64 = 2900;
    pub const SCLEAR_REFUND: u64 = 15000;
    pub const SELFDESTRUCT: u64 = 5000;
    pub const SELFDESTRUCT_NEWACCOUNT: u64 = 25000;
    pub const CREATE: u64 = 32000;
    pub const CODEDEPOSIT: u64 = 200;
    pub const CALL: u64 = 2600;
    pub const CALLVALUE: u64 = 9000;
    pub const CALLSTIPEND: u64 = 2300;
    pub const NEWACCOUNT: u64 = 25000;
    pub const EXP: u64 = 10;
    pub const EXPBYTE: u64 = 50;
    pub const MEMORY: u64 = 3;
    pub const TXCREATE: u64 = 32000;
    pub const TXDATAZERO: u64 = 4;
    pub const TXDATANONZERO: u64 = 16;
    pub const TRANSACTION: u64 = 21000;
    pub const LOG: u64 = 375;
    pub const LOGDATA: u64 = 8;
    pub const LOGTOPIC: u64 = 375;
    pub const KECCAK256: u64 = 30;
    pub const KECCAK256WORD: u64 = 6;
    pub const COPY: u64 = 3;
    pub const BLOCKHASH: u64 = 20;
    pub const CODESIZE: u64 = 2;
    pub const EXTCODESIZE: u64 = 2600;
    pub const EXTCODECOPY: u64 = 2600;
    pub const RETURNDATASIZE: u64 = 2;
    pub const RETURNDATACOPY: u64 = 3;
    pub const EXTCODEHASH: u64 = 2600;
    pub const CHAINID: u64 = 2;
    pub const SELFBALANCE: u64 = 5;
    pub const BASEFEE: u64 = 2;
    
    pub const WARM_STORAGE_READ_COST: u64 = 100;
    pub const COLD_SLOAD_COST: u64 = 2100;
    pub const COLD_ACCOUNT_ACCESS_COST: u64 = 2600;
    pub const WARM_STORAGE_WRITE_COST: u64 = 100;

    pub fn memory_gas_cost(size: U256) -> u64 {
        let size_u64 = size.as_u64();
        let memory_size_word = (size_u64 + 31) / 32;
        
        let linear_cost = memory_size_word.saturating_mul(Self::MEMORY);
        let quadratic_cost = memory_size_word.saturating_pow(2) / 512;
        
        linear_cost.saturating_add(quadratic_cost)
    }

    pub fn exp_gas_cost(exponent: U256) -> u64 {
        let byte_size = (exponent.bits() + 7) / 8;
        Self::EXP.saturating_add(Self::EXPBYTE.saturating_mul(byte_size as u64))
    }

    pub fn keccak256_gas_cost(data_size: U256) -> u64 {
        let size_u64 = data_size.as_u64();
        let word_size = (size_u64 + 31) / 32;
        Self::KECCAK256.saturating_add(Self::KECCAK256WORD.saturating_mul(word_size))
    }

    pub fn copy_gas_cost(data_size: U256) -> u64 {
        let size_u64 = data_size.as_u64();
        let word_size = (size_u64 + 31) / 32;
        Self::COPY.saturating_mul(word_size)
    }

    pub fn log_gas_cost(topic_count: u8, data_size: U256) -> u64 {
        let size_u64 = data_size.as_u64();
        Self::LOG
            .saturating_add(Self::LOGTOPIC.saturating_mul(topic_count as u64))
            .saturating_add(Self::LOGDATA.saturating_mul(size_u64))
    }
}