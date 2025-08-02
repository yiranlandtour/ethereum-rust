use crate::{
    error::{EvmError, EvmResult},
    execution::{ExecutionContext, ExecutionResult, HaltReason, Log},
    gas::{Gas, GasCost},
    memory::Memory,
    opcodes::Opcode,
    stack::Stack,
    state::StateDB,
};
use ethereum_crypto::keccak256;
use ethereum_types::{Address, H256, U256};
use std::cmp::min;

pub struct Interpreter<'a, S: StateDB> {
    context: ExecutionContext,
    state: &'a mut S,
    stack: Stack,
    memory: Memory,
    gas: Gas,
    pc: usize,
    return_data: Vec<u8>,
    logs: Vec<Log>,
    result: Option<ExecutionResult>,
}

impl<'a, S: StateDB> Interpreter<'a, S> {
    pub fn new(context: ExecutionContext, state: &'a mut S) -> Self {
        let gas = Gas::new(context.gas_limit);
        Self {
            context,
            state,
            stack: Stack::new(),
            memory: Memory::new(),
            gas,
            pc: 0,
            return_data: Vec::new(),
            logs: Vec::new(),
            result: None,
        }
    }

    pub fn run(&mut self) -> EvmResult<ExecutionResult> {
        while self.pc < self.context.code.len() {
            let opcode_byte = self.context.code[self.pc];
            let opcode = match Opcode::from_u8(opcode_byte) {
                Some(op) => op,
                None => {
                    return Ok(ExecutionResult::halt(
                        HaltReason::InvalidOpcode(opcode_byte),
                        self.gas.used(),
                    ));
                }
            };

            if let Err(e) = self.execute_opcode(opcode) {
                return Ok(self.handle_error(e));
            }

            if self.result.is_some() {
                break;
            }
        }

        Ok(self.result.take().unwrap_or_else(|| {
            ExecutionResult::success(Vec::new(), self.gas.used())
        }))
    }

    fn execute_opcode(&mut self, opcode: Opcode) -> EvmResult<()> {
        self.stack.require(opcode.stack_inputs())?;
        self.stack.limit_check(opcode.stack_outputs().saturating_sub(opcode.stack_inputs()))?;

        match opcode {
            // Stop and Arithmetic Operations
            Opcode::STOP => {
                self.result = Some(ExecutionResult::success(Vec::new(), self.gas.used()));
                Ok(())
            }
            Opcode::ADD => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(a.overflowing_add(b).0)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MUL => {
                self.gas.consume(GasCost::LOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(a.overflowing_mul(b).0)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SUB => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(a.overflowing_sub(b).0)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::DIV => {
                self.gas.consume(GasCost::LOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if b.is_zero() { U256::zero() } else { a / b })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SDIV => {
                self.gas.consume(GasCost::LOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(self.signed_div(a, b))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MOD => {
                self.gas.consume(GasCost::LOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if b.is_zero() { U256::zero() } else { a % b })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SMOD => {
                self.gas.consume(GasCost::LOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(self.signed_mod(a, b))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::ADDMOD => {
                self.gas.consume(GasCost::MID)?;
                let n = self.stack.pop()?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if n.is_zero() { 
                    U256::zero() 
                } else {
                    let a_big = a.full_mul(U256::one());
                    let b_big = b.full_mul(U256::one());
                    let sum = a_big.overflowing_add(b_big).0;
                    let (_, remainder) = sum.div_mod(n.into());
                    remainder.try_into().unwrap_or(U256::zero())
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MULMOD => {
                self.gas.consume(GasCost::MID)?;
                let n = self.stack.pop()?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if n.is_zero() { 
                    U256::zero() 
                } else {
                    let product = a.full_mul(b);
                    let (_, remainder) = product.div_mod(n.into());
                    remainder.try_into().unwrap_or(U256::zero())
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::EXP => {
                let exponent = self.stack.pop()?;
                self.gas.consume(GasCost::exp_gas_cost(exponent))?;
                let base = self.stack.pop()?;
                self.stack.push(base.overflowing_pow(exponent).0)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SIGNEXTEND => {
                self.gas.consume(GasCost::LOW)?;
                let ext = self.stack.pop()?;
                let x = self.stack.pop()?;
                self.stack.push(self.sign_extend(ext, x))?;
                self.pc += 1;
                Ok(())
            }

            // Comparison & Bitwise Logic Operations
            Opcode::LT => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if a < b { U256::one() } else { U256::zero() })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::GT => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if a > b { U256::one() } else { U256::zero() })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SLT => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if self.is_negative(a) == self.is_negative(b) {
                    if a < b { U256::one() } else { U256::zero() }
                } else if self.is_negative(a) {
                    U256::one()
                } else {
                    U256::zero()
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SGT => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if self.is_negative(a) == self.is_negative(b) {
                    if a > b { U256::one() } else { U256::zero() }
                } else if self.is_negative(b) {
                    U256::one()
                } else {
                    U256::zero()
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::EQ => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(if a == b { U256::one() } else { U256::zero() })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::ISZERO => {
                self.gas.consume(GasCost::VERYLOW)?;
                let a = self.stack.pop()?;
                self.stack.push(if a.is_zero() { U256::one() } else { U256::zero() })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::AND => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(a & b)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::OR => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(a | b)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::XOR => {
                self.gas.consume(GasCost::VERYLOW)?;
                let b = self.stack.pop()?;
                let a = self.stack.pop()?;
                self.stack.push(a ^ b)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::NOT => {
                self.gas.consume(GasCost::VERYLOW)?;
                let a = self.stack.pop()?;
                self.stack.push(!a)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::BYTE => {
                self.gas.consume(GasCost::VERYLOW)?;
                let i = self.stack.pop()?;
                let x = self.stack.pop()?;
                self.stack.push(if i < U256::from(32) {
                    let byte_index = i.as_u64() as usize;
                    let byte = x.byte(31 - byte_index);
                    U256::from(byte)
                } else {
                    U256::zero()
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SHL => {
                self.gas.consume(GasCost::VERYLOW)?;
                let shift = self.stack.pop()?;
                let value = self.stack.pop()?;
                self.stack.push(if shift >= U256::from(256) {
                    U256::zero()
                } else {
                    value << shift.as_usize()
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SHR => {
                self.gas.consume(GasCost::VERYLOW)?;
                let shift = self.stack.pop()?;
                let value = self.stack.pop()?;
                self.stack.push(if shift >= U256::from(256) {
                    U256::zero()
                } else {
                    value >> shift.as_usize()
                })?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SAR => {
                self.gas.consume(GasCost::VERYLOW)?;
                let shift = self.stack.pop()?;
                let value = self.stack.pop()?;
                self.stack.push(self.arithmetic_shr(value, shift))?;
                self.pc += 1;
                Ok(())
            }

            // SHA3
            Opcode::KECCAK256 => {
                let offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                self.gas.consume(GasCost::keccak256_gas_cost(size))?;
                let data = self.memory.get(offset.as_usize(), size.as_usize());
                let hash = keccak256(&data);
                self.stack.push(U256::from(hash.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }

            // Environmental Information
            Opcode::ADDRESS => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.context.address.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::BALANCE => {
                let address = self.stack.pop()?;
                self.gas.consume(GasCost::BALANCE)?;
                let balance = self.state
                    .get_account(&address_from_u256(address))
                    .map(|acc| acc.balance)
                    .unwrap_or_default();
                self.stack.push(balance)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::ORIGIN => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.context.origin.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CALLER => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.context.caller.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CALLVALUE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.context.value)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CALLDATALOAD => {
                self.gas.consume(GasCost::VERYLOW)?;
                let offset = self.stack.pop()?;
                let data = self.get_data(offset, U256::from(32));
                self.stack.push(U256::from(&data[..]))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CALLDATASIZE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.context.data.len()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CALLDATACOPY => {
                let mem_offset = self.stack.pop()?;
                let data_offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                self.gas.consume(GasCost::copy_gas_cost(size))?;
                let data = self.get_data(data_offset, size);
                self.memory.set(mem_offset.as_usize(), &data)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CODESIZE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.context.code.len()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CODECOPY => {
                let mem_offset = self.stack.pop()?;
                let code_offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                self.gas.consume(GasCost::copy_gas_cost(size))?;
                let code = self.get_code(code_offset, size);
                self.memory.set(mem_offset.as_usize(), &code)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::GASPRICE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.context.gas_price)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::EXTCODESIZE => {
                let address = self.stack.pop()?;
                self.gas.consume(GasCost::EXTCODESIZE)?;
                let size = self.state
                    .get_account(&address_from_u256(address))
                    .map(|acc| acc.code.len())
                    .unwrap_or(0);
                self.stack.push(U256::from(size))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::EXTCODECOPY => {
                let address = self.stack.pop()?;
                let mem_offset = self.stack.pop()?;
                let code_offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                self.gas.consume(GasCost::EXTCODECOPY)?;
                self.gas.consume(GasCost::copy_gas_cost(size))?;
                
                let code = self.state
                    .get_account(&address_from_u256(address))
                    .map(|acc| self.get_slice(&acc.code, code_offset, size))
                    .unwrap_or_else(|| vec![0; size.as_usize()]);
                self.memory.set(mem_offset.as_usize(), &code)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::RETURNDATASIZE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.return_data.len()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::RETURNDATACOPY => {
                let mem_offset = self.stack.pop()?;
                let data_offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                self.gas.consume(GasCost::copy_gas_cost(size))?;
                
                if data_offset.saturating_add(size) > U256::from(self.return_data.len()) {
                    return Err(EvmError::ReturnDataOutOfBounds);
                }
                
                let data = self.get_slice(&self.return_data, data_offset, size);
                self.memory.set(mem_offset.as_usize(), &data)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::EXTCODEHASH => {
                let address = self.stack.pop()?;
                self.gas.consume(GasCost::EXTCODEHASH)?;
                let hash = self.state
                    .get_account(&address_from_u256(address))
                    .map(|acc| {
                        if acc.code.is_empty() {
                            H256::zero()
                        } else {
                            keccak256(&acc.code)
                        }
                    })
                    .unwrap_or(H256::zero());
                self.stack.push(U256::from(hash.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }

            // Block Information
            Opcode::BLOCKHASH => {
                let block_number = self.stack.pop()?;
                self.gas.consume(GasCost::BLOCKHASH)?;
                let hash = if block_number >= self.context.block.number || 
                    self.context.block.number - block_number > U256::from(256) {
                    H256::zero()
                } else {
                    let index = block_number.as_usize();
                    self.context.block.block_hashes.get(index).copied().unwrap_or_default()
                };
                self.stack.push(U256::from(hash.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::COINBASE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.context.block.coinbase.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::TIMESTAMP => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.context.block.timestamp)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::NUMBER => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.context.block.number)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::DIFFICULTY => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.context.block.difficulty)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::GASLIMIT => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.context.block.gas_limit)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::CHAINID => {
                self.gas.consume(GasCost::CHAINID)?;
                self.stack.push(self.context.block.chain_id)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SELFBALANCE => {
                self.gas.consume(GasCost::SELFBALANCE)?;
                let balance = self.state
                    .get_account(&self.context.address)
                    .map(|acc| acc.balance)
                    .unwrap_or_default();
                self.stack.push(balance)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::BASEFEE => {
                self.gas.consume(GasCost::BASEFEE)?;
                let base_fee = self.context.block.base_fee.unwrap_or_default();
                self.stack.push(base_fee)?;
                self.pc += 1;
                Ok(())
            }

            // Stack, Memory, Storage and Flow Operations
            Opcode::POP => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.pop()?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MLOAD => {
                self.gas.consume(GasCost::VERYLOW)?;
                let offset = self.stack.pop()?;
                let value = self.memory.get_u256(offset.as_usize());
                self.stack.push(value)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MSTORE => {
                self.gas.consume(GasCost::VERYLOW)?;
                let offset = self.stack.pop()?;
                let value = self.stack.pop()?;
                self.memory.set_u256(offset.as_usize(), value)?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MSTORE8 => {
                self.gas.consume(GasCost::VERYLOW)?;
                let offset = self.stack.pop()?;
                let value = self.stack.pop()?;
                self.memory.set_byte(offset.as_usize(), value.byte(31))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SLOAD => {
                let key = self.stack.pop()?;
                self.gas.consume(GasCost::SLOAD)?;
                let mut key_bytes = [0u8; 32];
                key.to_big_endian(&mut key_bytes);
                let value = self.state.get_storage(&self.context.address, &H256::from(key_bytes));
                self.stack.push(U256::from(value.as_bytes()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::SSTORE => {
                if self.context.is_static {
                    return Err(EvmError::StaticCallStateModification);
                }
                let key = self.stack.pop()?;
                let value = self.stack.pop()?;
                self.gas.consume(GasCost::SSET)?;
                let mut key_bytes = [0u8; 32];
                key.to_big_endian(&mut key_bytes);
                let mut value_bytes = [0u8; 32];
                value.to_big_endian(&mut value_bytes);
                self.state.set_storage(
                    self.context.address, 
                    H256::from(key_bytes),
                    H256::from(value_bytes)
                );
                self.pc += 1;
                Ok(())
            }
            Opcode::JUMP => {
                self.gas.consume(GasCost::MID)?;
                let dest = self.stack.pop()?;
                self.jump(dest.as_usize())?;
                Ok(())
            }
            Opcode::JUMPI => {
                self.gas.consume(GasCost::HIGH)?;
                let dest = self.stack.pop()?;
                let cond = self.stack.pop()?;
                if !cond.is_zero() {
                    self.jump(dest.as_usize())?;
                } else {
                    self.pc += 1;
                }
                Ok(())
            }
            Opcode::PC => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.pc))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::MSIZE => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(self.memory.effective_len())?;
                self.pc += 1;
                Ok(())
            }
            Opcode::GAS => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::from(self.gas.remaining()))?;
                self.pc += 1;
                Ok(())
            }
            Opcode::JUMPDEST => {
                self.gas.consume(GasCost::JUMPDEST)?;
                self.pc += 1;
                Ok(())
            }

            // Push Operations
            Opcode::PUSH0 => {
                self.gas.consume(GasCost::BASE)?;
                self.stack.push(U256::zero())?;
                self.pc += 1;
                Ok(())
            }
            op if op.is_push() => {
                self.gas.consume(GasCost::VERYLOW)?;
                let n = op.push_bytes().unwrap();
                let start = self.pc + 1;
                let end = min(start + n, self.context.code.len());
                let mut bytes = [0u8; 32];
                let data = &self.context.code[start..end];
                bytes[32 - data.len()..].copy_from_slice(data);
                self.stack.push(U256::from_big_endian(&bytes))?;
                self.pc = end;
                Ok(())
            }

            // Dup Operations
            op if op as u8 >= Opcode::DUP1 as u8 && op as u8 <= Opcode::DUP16 as u8 => {
                self.gas.consume(GasCost::VERYLOW)?;
                let n = (op as u8 - Opcode::DUP1 as u8) as usize;
                self.stack.dup(n)?;
                self.pc += 1;
                Ok(())
            }

            // Swap Operations
            op if op as u8 >= Opcode::SWAP1 as u8 && op as u8 <= Opcode::SWAP16 as u8 => {
                self.gas.consume(GasCost::VERYLOW)?;
                let n = (op as u8 - Opcode::SWAP1 as u8 + 1) as usize;
                self.stack.swap(n)?;
                self.pc += 1;
                Ok(())
            }

            // Log Operations
            op if op as u8 >= Opcode::LOG0 as u8 && op as u8 <= Opcode::LOG4 as u8 => {
                if self.context.is_static {
                    return Err(EvmError::StaticCallStateModification);
                }
                let topic_count = (op as u8 - Opcode::LOG0 as u8) as u8;
                let offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                
                let mut topics = Vec::with_capacity(topic_count as usize);
                for _ in 0..topic_count {
                    let topic = self.stack.pop()?;
                    let mut topic_bytes = [0u8; 32];
                    topic.to_big_endian(&mut topic_bytes);
                    topics.push(H256::from(topic_bytes));
                }
                
                self.gas.consume(GasCost::log_gas_cost(topic_count, size))?;
                let data = self.memory.get(offset.as_usize(), size.as_usize());
                
                self.logs.push(Log {
                    address: self.context.address,
                    topics,
                    data,
                });
                
                self.pc += 1;
                Ok(())
            }

            // System Operations
            Opcode::RETURN => {
                let offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                let data = self.memory.get(offset.as_usize(), size.as_usize());
                self.result = Some(ExecutionResult::success(data, self.gas.used()));
                Ok(())
            }
            Opcode::REVERT => {
                let offset = self.stack.pop()?;
                let size = self.stack.pop()?;
                let data = self.memory.get(offset.as_usize(), size.as_usize());
                self.result = Some(ExecutionResult::revert(data, self.gas.used()));
                Ok(())
            }

            _ => {
                self.pc += 1;
                Ok(())
            }
        }
    }

    fn handle_error(&self, error: EvmError) -> ExecutionResult {
        match error {
            EvmError::OutOfGas => ExecutionResult::halt(HaltReason::OutOfGas, self.gas.limit()),
            EvmError::StackOverflow => ExecutionResult::halt(HaltReason::StackOverflow, self.gas.used()),
            EvmError::StackUnderflow => ExecutionResult::halt(HaltReason::StackUnderflow, self.gas.used()),
            EvmError::InvalidJump(_) => ExecutionResult::halt(HaltReason::InvalidJump, self.gas.used()),
            EvmError::InvalidOpcode(op) => ExecutionResult::halt(HaltReason::InvalidOpcode(op), self.gas.used()),
            EvmError::StaticCallStateModification => {
                ExecutionResult::halt(HaltReason::StateModificationInStatic, self.gas.used())
            }
            _ => ExecutionResult::halt(HaltReason::InvalidCode, self.gas.used()),
        }
    }

    fn jump(&mut self, dest: usize) -> EvmResult<()> {
        if dest >= self.context.code.len() || 
           self.context.code[dest] != Opcode::JUMPDEST as u8 {
            return Err(EvmError::InvalidJump(dest));
        }
        self.pc = dest;
        Ok(())
    }

    fn get_data(&self, offset: U256, size: U256) -> Vec<u8> {
        self.get_slice(&self.context.data, offset, size)
    }

    fn get_code(&self, offset: U256, size: U256) -> Vec<u8> {
        self.get_slice(&self.context.code, offset, size)
    }

    fn get_slice(&self, data: &[u8], offset: U256, size: U256) -> Vec<u8> {
        if size.is_zero() {
            return Vec::new();
        }

        let offset = offset.as_usize();
        let size = size.as_usize();

        if offset >= data.len() {
            vec![0; size]
        } else {
            let end = min(offset + size, data.len());
            let mut result = data[offset..end].to_vec();
            result.resize(size, 0);
            result
        }
    }

    fn signed_div(&self, a: U256, b: U256) -> U256 {
        if b.is_zero() {
            return U256::zero();
        }

        let a_negative = self.is_negative(a);
        let b_negative = self.is_negative(b);

        let a_abs = if a_negative { self.twos_complement(a) } else { a };
        let b_abs = if b_negative { self.twos_complement(b) } else { b };

        let result = a_abs / b_abs;

        if a_negative != b_negative {
            self.twos_complement(result)
        } else {
            result
        }
    }

    fn signed_mod(&self, a: U256, b: U256) -> U256 {
        if b.is_zero() {
            return U256::zero();
        }

        let a_negative = self.is_negative(a);

        let a_abs = if a_negative { self.twos_complement(a) } else { a };
        let b_abs = if self.is_negative(b) { self.twos_complement(b) } else { b };

        let result = a_abs % b_abs;

        if a_negative && !result.is_zero() {
            self.twos_complement(result)
        } else {
            result
        }
    }

    fn sign_extend(&self, ext: U256, x: U256) -> U256 {
        if ext >= U256::from(32) {
            return x;
        }

        let ext = ext.as_usize();
        let bit_index = ext * 8 + 7;
        let bit = x.bit(bit_index);

        let mask = (U256::one() << (bit_index + 1)) - U256::one();
        if bit {
            x | !mask
        } else {
            x & mask
        }
    }

    fn arithmetic_shr(&self, value: U256, shift: U256) -> U256 {
        if shift >= U256::from(256) {
            if self.is_negative(value) {
                U256::MAX
            } else {
                U256::zero()
            }
        } else {
            let shift = shift.as_usize();
            if self.is_negative(value) {
                let shifted = value >> shift;
                let mask = U256::MAX << (256 - shift);
                shifted | mask
            } else {
                value >> shift
            }
        }
    }

    fn is_negative(&self, value: U256) -> bool {
        value.bit(255)
    }

    fn twos_complement(&self, value: U256) -> U256 {
        (!value).overflowing_add(U256::one()).0
    }
}

fn address_from_u256(value: U256) -> Address {
    let mut bytes = [0u8; 32];
    value.to_big_endian(&mut bytes);
    Address::from_slice(&bytes[12..]).unwrap_or_else(|_| Address::from_bytes([0u8; 20]))
}