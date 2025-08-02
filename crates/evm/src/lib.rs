pub mod error;
pub mod execution;
pub mod gas;
pub mod interpreter;
pub mod memory;
pub mod opcodes;
pub mod stack;
pub mod state;

#[cfg(test)]
mod tests;

pub use error::{EvmError, EvmResult};
pub use execution::{ExecutionContext, ExecutionResult};
pub use interpreter::Interpreter;

use ethereum_types::{Address, H256, U256};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Evm {
    state: HashMap<Address, Account>,
}

#[derive(Debug, Clone, Default)]
pub struct Account {
    pub balance: U256,
    pub nonce: u64,
    pub code: Vec<u8>,
    pub storage: HashMap<H256, H256>,
}

impl Evm {
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
        }
    }

    pub fn execute(
        &mut self,
        context: ExecutionContext,
    ) -> EvmResult<ExecutionResult> {
        let mut interpreter = Interpreter::new(context, &mut self.state);
        interpreter.run()
    }
}

impl Default for Evm {
    fn default() -> Self {
        Self::new()
    }
}