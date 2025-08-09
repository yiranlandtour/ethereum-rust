use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Transaction};
use ethereum_evm::{EVM, Opcode};
use std::sync::Arc;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::{Result, DebugError};

/// Gas profiling results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GasProfile {
    pub total_gas_used: U256,
    pub execution_gas: U256,
    pub intrinsic_gas: U256,
    pub refund: U256,
    pub opcode_costs: HashMap<String, U256>,
    pub call_costs: Vec<CallCost>,
    pub storage_costs: StorageCosts,
}

/// Call cost breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallCost {
    pub call_type: String,
    pub target: Option<Address>,
    pub gas_provided: U256,
    pub gas_used: U256,
    pub depth: usize,
}

/// Storage operation costs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageCosts {
    pub sload_count: u64,
    pub sload_gas: U256,
    pub sstore_count: u64,
    pub sstore_gas: U256,
    pub storage_refund: U256,
}

/// Opcode statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpcodeStats {
    pub opcode_counts: HashMap<String, u64>,
    pub opcode_gas: HashMap<String, U256>,
    pub most_expensive_ops: Vec<ExpensiveOp>,
    pub hot_spots: Vec<HotSpot>,
}

/// Expensive operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpensiveOp {
    pub pc: u64,
    pub opcode: String,
    pub gas_cost: U256,
    pub count: u64,
}

/// Hot spot in code
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotSpot {
    pub pc_range: (u64, u64),
    pub gas_consumed: U256,
    pub execution_count: u64,
}

/// Gas profiler
pub struct Profiler;

impl Profiler {
    pub fn new() -> Self {
        Self
    }
    
    /// Profile transaction gas usage
    pub async fn profile_transaction<D: ethereum_storage::Database>(
        &self,
        tx: &Transaction,
        block: &Block,
        evm: Arc<EVM<D>>,
    ) -> Result<GasProfile> {
        let context = self.create_context(block);
        
        // Calculate intrinsic gas
        let intrinsic_gas = self.calculate_intrinsic_gas(tx);
        
        let mut opcode_costs = HashMap::new();
        let mut call_costs = Vec::new();
        let mut storage_costs = StorageCosts {
            sload_count: 0,
            sload_gas: U256::zero(),
            sstore_count: 0,
            sstore_gas: U256::zero(),
            storage_refund: U256::zero(),
        };
        
        let mut last_gas = tx.gas_limit;
        
        // Execute transaction with profiling
        let result = evm.execute_transaction_with_tracer(
            tx,
            self.get_state(block),
            &context,
            |pc, op, stack, memory, storage| {
                let gas_cost = last_gas - evm.get_gas_left();
                
                // Track opcode costs
                let op_str = format!("{:?}", op);
                *opcode_costs.entry(op_str.clone()).or_insert(U256::zero()) += gas_cost;
                
                // Track storage operations
                match op {
                    Opcode::SLOAD => {
                        storage_costs.sload_count += 1;
                        storage_costs.sload_gas += gas_cost;
                    }
                    Opcode::SSTORE => {
                        storage_costs.sstore_count += 1;
                        storage_costs.sstore_gas += gas_cost;
                    }
                    _ => {}
                }
                
                // Track calls
                match op {
                    Opcode::CALL | Opcode::CALLCODE | Opcode::DELEGATECALL | Opcode::STATICCALL => {
                        if stack.len() >= 2 {
                            let gas_provided = U256::from(stack[0].as_bytes());
                            call_costs.push(CallCost {
                                call_type: op_str,
                                target: None, // Would extract from stack
                                gas_provided,
                                gas_used: gas_cost,
                                depth: evm.get_call_depth(),
                            });
                        }
                    }
                    _ => {}
                }
                
                last_gas = evm.get_gas_left();
            }
        ).await.map_err(|e| DebugError::EvmError(e.to_string()))?;
        
        let execution_gas = result.gas_used - intrinsic_gas;
        
        Ok(GasProfile {
            total_gas_used: result.gas_used,
            execution_gas,
            intrinsic_gas,
            refund: result.gas_refund,
            opcode_costs,
            call_costs,
            storage_costs,
        })
    }
    
    /// Get opcode statistics
    pub async fn get_opcode_stats<D: ethereum_storage::Database>(
        &self,
        tx: &Transaction,
        block: &Block,
        evm: Arc<EVM<D>>,
    ) -> Result<OpcodeStats> {
        let context = self.create_context(block);
        
        let mut opcode_counts: HashMap<String, u64> = HashMap::new();
        let mut opcode_gas: HashMap<String, U256> = HashMap::new();
        let mut pc_costs: HashMap<u64, (String, U256, u64)> = HashMap::new();
        
        let mut last_gas = tx.gas_limit;
        
        // Execute transaction with statistics collection
        let _result = evm.execute_transaction_with_tracer(
            tx,
            self.get_state(block),
            &context,
            |pc, op, _stack, _memory, _storage| {
                let gas_cost = last_gas - evm.get_gas_left();
                let op_str = format!("{:?}", op);
                
                // Count opcodes
                *opcode_counts.entry(op_str.clone()).or_insert(0) += 1;
                
                // Sum gas per opcode
                *opcode_gas.entry(op_str.clone()).or_insert(U256::zero()) += gas_cost;
                
                // Track per-PC costs
                let entry = pc_costs.entry(pc as u64).or_insert((op_str, U256::zero(), 0));
                entry.1 += gas_cost;
                entry.2 += 1;
                
                last_gas = evm.get_gas_left();
            }
        ).await.map_err(|e| DebugError::EvmError(e.to_string()))?;
        
        // Find most expensive operations
        let mut expensive_ops: Vec<ExpensiveOp> = pc_costs
            .into_iter()
            .map(|(pc, (opcode, gas_cost, count))| ExpensiveOp {
                pc,
                opcode,
                gas_cost,
                count,
            })
            .collect();
        
        expensive_ops.sort_by(|a, b| b.gas_cost.cmp(&a.gas_cost));
        expensive_ops.truncate(10);
        
        // Find hot spots (simplified - would need more sophisticated analysis)
        let hot_spots = self.find_hot_spots(&expensive_ops);
        
        Ok(OpcodeStats {
            opcode_counts,
            opcode_gas,
            most_expensive_ops: expensive_ops,
            hot_spots,
        })
    }
    
    /// Calculate intrinsic gas for transaction
    fn calculate_intrinsic_gas(&self, tx: &Transaction) -> U256 {
        let mut gas = U256::from(21000); // Base transaction cost
        
        // Add data costs
        for byte in &tx.input {
            if *byte == 0 {
                gas += U256::from(4); // Zero byte
            } else {
                gas += U256::from(16); // Non-zero byte
            }
        }
        
        // Add access list costs (EIP-2930)
        if let Some(ref access_list) = tx.access_list {
            gas += U256::from(access_list.len() * 2400); // Per address
            // Would also add per-storage-key costs
        }
        
        gas
    }
    
    /// Find hot spots in code
    fn find_hot_spots(&self, expensive_ops: &[ExpensiveOp]) -> Vec<HotSpot> {
        let mut hot_spots = Vec::new();
        
        if expensive_ops.is_empty() {
            return hot_spots;
        }
        
        // Simple clustering of expensive operations
        let mut current_start = expensive_ops[0].pc;
        let mut current_end = expensive_ops[0].pc;
        let mut current_gas = expensive_ops[0].gas_cost;
        let mut current_count = expensive_ops[0].count;
        
        for op in expensive_ops.iter().skip(1) {
            if op.pc <= current_end + 10 {
                // Extend current hot spot
                current_end = op.pc;
                current_gas += op.gas_cost;
                current_count += op.count;
            } else {
                // Start new hot spot
                if current_gas > U256::from(1000) {
                    hot_spots.push(HotSpot {
                        pc_range: (current_start, current_end),
                        gas_consumed: current_gas,
                        execution_count: current_count,
                    });
                }
                
                current_start = op.pc;
                current_end = op.pc;
                current_gas = op.gas_cost;
                current_count = op.count;
            }
        }
        
        // Add final hot spot
        if current_gas > U256::from(1000) {
            hot_spots.push(HotSpot {
                pc_range: (current_start, current_end),
                gas_consumed: current_gas,
                execution_count: current_count,
            });
        }
        
        hot_spots
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
    
    fn get_state<D: ethereum_storage::Database>(&self, block: &Block) -> ethereum_trie::PatriciaTrie<D> {
        // Simplified - would get actual state
        ethereum_trie::PatriciaTrie::new_with_root(
            Arc::new(ethereum_storage::MemoryDatabase::new()),
            block.header.state_root,
        )
    }
}