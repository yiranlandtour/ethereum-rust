use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Transaction};
use ethereum_storage::Database;
use ethereum_evm::{EVM, Opcode};
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};

use crate::{Result, DebugError};

/// Breakpoint for debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub pc: Option<u64>,
    pub opcode: Option<String>,
    pub condition: Option<String>,
}

/// Debugger state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebuggerState {
    pub pc: u64,
    pub op: String,
    pub gas: U256,
    pub gas_cost: U256,
    pub memory: Vec<u8>,
    pub stack: Vec<H256>,
    pub storage: HashMap<H256, H256>,
    pub depth: usize,
    pub return_data: Vec<u8>,
    pub breakpoints_hit: Vec<usize>,
    pub steps: Vec<StepInfo>,
}

/// Step information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepInfo {
    pub pc: u64,
    pub op: String,
    pub gas: U256,
    pub stack_size: usize,
    pub memory_size: usize,
    pub storage_changes: HashMap<H256, H256>,
}

/// Transaction debugger
pub struct Debugger<D: Database> {
    db: Arc<D>,
    evm: Arc<EVM<D>>,
}

impl<D: Database + 'static> Debugger<D> {
    pub fn new(db: Arc<D>, evm: Arc<EVM<D>>) -> Self {
        Self { db, evm }
    }
    
    /// Debug transaction with breakpoints
    pub async fn debug_transaction(
        &self,
        tx: &Transaction,
        block: &Block,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<DebuggerState> {
        let context = self.create_context(block);
        let state = self.get_state_at_block(&block.header.parent_hash).await?;
        
        let mut debugger_state = DebuggerState {
            pc: 0,
            op: String::new(),
            gas: tx.gas_limit,
            gas_cost: U256::zero(),
            memory: Vec::new(),
            stack: Vec::new(),
            storage: HashMap::new(),
            depth: 0,
            return_data: Vec::new(),
            breakpoints_hit: Vec::new(),
            steps: Vec::new(),
        };
        
        let mut step_count = 0;
        let mut storage_snapshot = HashMap::new();
        
        // Execute with debugger
        let result = self.evm.execute_transaction_with_tracer(
            tx,
            state,
            &context,
            |pc, op, stack, memory, storage| {
                // Check breakpoints
                for (i, bp) in breakpoints.iter().enumerate() {
                    if self.should_break(bp, pc, op, stack, memory) {
                        debugger_state.breakpoints_hit.push(i);
                        
                        // Update debugger state
                        debugger_state.pc = pc as u64;
                        debugger_state.op = format!("{:?}", op);
                        debugger_state.gas = self.evm.get_gas_left();
                        debugger_state.memory = memory.clone();
                        debugger_state.stack = stack.clone();
                        debugger_state.storage = storage.clone();
                        debugger_state.depth = self.evm.get_call_depth();
                    }
                }
                
                // Track storage changes
                let mut storage_changes = HashMap::new();
                for (key, value) in storage {
                    if storage_snapshot.get(key) != Some(value) {
                        storage_changes.insert(*key, *value);
                        storage_snapshot.insert(*key, *value);
                    }
                }
                
                // Record step
                debugger_state.steps.push(StepInfo {
                    pc: pc as u64,
                    op: format!("{:?}", op),
                    gas: self.evm.get_gas_left(),
                    stack_size: stack.len(),
                    memory_size: memory.len(),
                    storage_changes,
                });
                
                step_count += 1;
                
                // Limit steps to prevent excessive data
                if step_count > 10000 {
                    return;
                }
            }
        ).await.map_err(|e| DebugError::EvmError(e.to_string()))?;
        
        debugger_state.return_data = result.return_data;
        
        Ok(debugger_state)
    }
    
    /// Check if should break at current state
    fn should_break(
        &self,
        breakpoint: &Breakpoint,
        pc: usize,
        op: Opcode,
        stack: &[H256],
        memory: &[u8],
    ) -> bool {
        // Check PC breakpoint
        if let Some(bp_pc) = breakpoint.pc {
            if pc as u64 != bp_pc {
                return false;
            }
        }
        
        // Check opcode breakpoint
        if let Some(ref bp_op) = breakpoint.opcode {
            if format!("{:?}", op) != *bp_op {
                return false;
            }
        }
        
        // Check condition
        if let Some(ref condition) = breakpoint.condition {
            if !self.evaluate_condition(condition, stack, memory) {
                return false;
            }
        }
        
        true
    }
    
    /// Evaluate breakpoint condition
    fn evaluate_condition(&self, condition: &str, stack: &[H256], memory: &[u8]) -> bool {
        // Simple condition evaluation
        // Real implementation would parse and evaluate complex expressions
        
        if condition.starts_with("stack[0] == ") {
            if let Some(value_str) = condition.strip_prefix("stack[0] == ") {
                if !stack.is_empty() {
                    if let Ok(value) = U256::from_dec_str(value_str) {
                        return U256::from(stack[0].as_bytes()) == value;
                    }
                }
            }
        }
        
        // Default to true if condition cannot be evaluated
        true
    }
    
    fn create_context(&self, block: &Block) -> ethereum_evm::Context {
        ethereum_evm::Context {
            block_number: block.header.number,
            timestamp: block.header.timestamp,
            gas_limit: block.header.gas_limit,
            coinbase: block.header.author,
            difficulty: block.header.difficulty,
            chain_id: 1,
        }
    }
    
    async fn get_state_at_block(&self, block_hash: &H256) -> Result<ethereum_trie::PatriciaTrie<D>> {
        let key = format!("block:{}", hex::encode(block_hash));
        let block_data = self.db.get(key.as_bytes())?
            .ok_or(DebugError::BlockNotFound)?;
        
        let block: Block = bincode::deserialize(&block_data)
            .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
        
        Ok(ethereum_trie::PatriciaTrie::new_with_root(
            self.db.clone(),
            block.header.state_root,
        ))
    }
}