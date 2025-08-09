use ethereum_types::{H256, U256, Address};
use ethereum_core::{Block, Transaction};
use ethereum_storage::Database;
use ethereum_evm::{EVM, Opcode};
use std::sync::Arc;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::{Result, DebugError};

/// Trace configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceConfig {
    #[serde(default)]
    pub disable_memory: bool,
    #[serde(default)]
    pub disable_stack: bool,
    #[serde(default)]
    pub disable_storage: bool,
    #[serde(default)]
    pub disable_return_data: bool,
    #[serde(default)]
    pub tracer: Option<String>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub trace_call: bool,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            disable_memory: false,
            disable_stack: false,
            disable_storage: false,
            disable_return_data: false,
            tracer: None,
            timeout: None,
            trace_call: true,
        }
    }
}

/// Trace result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TraceResult {
    CallTrace(CallTrace),
    StructLogs(StructLogs),
    Custom(serde_json::Value),
}

/// Call trace
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallTrace {
    pub from: Address,
    pub to: Option<Address>,
    pub value: U256,
    pub gas: U256,
    pub gas_used: U256,
    pub input: Vec<u8>,
    pub output: Vec<u8>,
    pub error: Option<String>,
    pub revert_reason: Option<String>,
    pub calls: Vec<CallTrace>,
    pub trace_type: TraceType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TraceType {
    Call,
    Create,
    Create2,
    Delegatecall,
    Staticcall,
    Selfdestruct,
}

/// Structured logs
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructLogs {
    pub gas: U256,
    pub return_value: Vec<u8>,
    pub struct_logs: Vec<StructLog>,
}

/// Single log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructLog {
    pub pc: u64,
    pub op: String,
    pub gas: U256,
    pub gas_cost: U256,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<Vec<H256>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<HashMap<H256, H256>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_data: Option<Vec<u8>>,
}

/// Transaction tracer
pub struct Tracer<D: Database> {
    db: Arc<D>,
    evm: Arc<EVM<D>>,
}

impl<D: Database + 'static> Tracer<D> {
    pub fn new(db: Arc<D>, evm: Arc<EVM<D>>) -> Self {
        Self { db, evm }
    }
    
    /// Trace transaction execution
    pub async fn trace_transaction(
        &self,
        tx: &Transaction,
        block: &Block,
        config: Option<TraceConfig>,
    ) -> Result<TraceResult> {
        let config = config.unwrap_or_default();
        
        // Check if custom tracer is specified
        if let Some(ref tracer_name) = config.tracer {
            return self.run_custom_tracer(tx, block, tracer_name).await;
        }
        
        // Run standard tracer
        if config.trace_call {
            let trace = self.trace_call(tx, block, &config).await?;
            Ok(TraceResult::CallTrace(trace))
        } else {
            let logs = self.trace_struct_logs(tx, block, &config).await?;
            Ok(TraceResult::StructLogs(logs))
        }
    }
    
    /// Trace block execution
    pub async fn trace_block(
        &self,
        block: &Block,
        config: Option<TraceConfig>,
    ) -> Result<Vec<TraceResult>> {
        let mut results = Vec::new();
        
        for tx in &block.body.transactions {
            let result = self.trace_transaction(tx, block, config.clone()).await?;
            results.push(result);
        }
        
        Ok(results)
    }
    
    /// Trace call execution
    async fn trace_call(
        &self,
        tx: &Transaction,
        block: &Block,
        config: &TraceConfig,
    ) -> Result<CallTrace> {
        // Create EVM context
        let context = self.create_context(block);
        
        // Create state
        let state = self.get_state_at_block(&block.header.parent_hash).await?;
        
        // Setup tracer hooks
        let mut trace = CallTrace {
            from: self.get_sender(tx)?,
            to: tx.to,
            value: tx.value,
            gas: tx.gas_limit,
            gas_used: U256::zero(),
            input: tx.input.clone(),
            output: Vec::new(),
            error: None,
            revert_reason: None,
            calls: Vec::new(),
            trace_type: if tx.to.is_none() {
                TraceType::Create
            } else {
                TraceType::Call
            },
        };
        
        // Execute transaction with tracing
        let result = self.evm.execute_transaction_with_tracer(
            tx,
            state,
            &context,
            |depth, op, stack, memory, storage| {
                // Capture subcalls
                match op {
                    Opcode::CALL | Opcode::CALLCODE | Opcode::DELEGATECALL | Opcode::STATICCALL => {
                        if let Some(subcall) = self.extract_subcall(op, stack, memory) {
                            trace.calls.push(subcall);
                        }
                    }
                    Opcode::CREATE | Opcode::CREATE2 => {
                        if let Some(subcall) = self.extract_create(op, stack, memory) {
                            trace.calls.push(subcall);
                        }
                    }
                    _ => {}
                }
            }
        ).await.map_err(|e| DebugError::EvmError(e.to_string()))?;
        
        // Update trace with result
        trace.gas_used = result.gas_used;
        trace.output = result.return_data;
        
        if !result.success {
            trace.error = Some(result.error.unwrap_or_else(|| "Execution failed".to_string()));
            if let Some(revert_data) = result.revert_data {
                trace.revert_reason = self.decode_revert_reason(&revert_data);
            }
        }
        
        Ok(trace)
    }
    
    /// Trace structured logs
    async fn trace_struct_logs(
        &self,
        tx: &Transaction,
        block: &Block,
        config: &TraceConfig,
    ) -> Result<StructLogs> {
        let context = self.create_context(block);
        let state = self.get_state_at_block(&block.header.parent_hash).await?;
        
        let mut struct_logs = Vec::new();
        let mut last_gas = tx.gas_limit;
        
        // Execute with step tracer
        let result = self.evm.execute_transaction_with_tracer(
            tx,
            state,
            &context,
            |pc, op, stack, memory, storage| {
                let gas_cost = last_gas - self.evm.get_gas_left();
                
                let mut log = StructLog {
                    pc: pc as u64,
                    op: format!("{:?}", op),
                    gas: self.evm.get_gas_left(),
                    gas_cost,
                    depth: self.evm.get_call_depth(),
                    error: None,
                    stack: None,
                    memory: None,
                    storage: None,
                    return_data: None,
                };
                
                // Add optional data based on config
                if !config.disable_stack {
                    log.stack = Some(stack.clone());
                }
                
                if !config.disable_memory {
                    log.memory = Some(memory.clone());
                }
                
                if !config.disable_storage {
                    log.storage = Some(storage.clone());
                }
                
                if !config.disable_return_data {
                    log.return_data = Some(self.evm.get_return_data());
                }
                
                struct_logs.push(log);
                last_gas = self.evm.get_gas_left();
            }
        ).await.map_err(|e| DebugError::EvmError(e.to_string()))?;
        
        Ok(StructLogs {
            gas: result.gas_used,
            return_value: result.return_data,
            struct_logs,
        })
    }
    
    /// Run custom tracer
    async fn run_custom_tracer(
        &self,
        tx: &Transaction,
        block: &Block,
        tracer_name: &str,
    ) -> Result<TraceResult> {
        match tracer_name {
            "callTracer" => {
                let trace = self.trace_call(tx, block, &TraceConfig::default()).await?;
                Ok(TraceResult::CallTrace(trace))
            }
            "prestateTracer" => {
                let prestate = self.trace_prestate(tx, block).await?;
                Ok(TraceResult::Custom(prestate))
            }
            "4byteTracer" => {
                let fourbyte = self.trace_4byte(tx, block).await?;
                Ok(TraceResult::Custom(fourbyte))
            }
            _ => Err(DebugError::InvalidTraceConfig),
        }
    }
    
    /// Trace prestate
    async fn trace_prestate(
        &self,
        tx: &Transaction,
        block: &Block,
    ) -> Result<serde_json::Value> {
        // Get accounts touched by transaction
        let mut prestate = serde_json::Map::new();
        
        // Add sender
        let sender = self.get_sender(tx)?;
        let sender_state = self.get_account_state(sender, &block.header.parent_hash).await?;
        prestate.insert(format!("{:?}", sender), sender_state);
        
        // Add recipient
        if let Some(to) = tx.to {
            let to_state = self.get_account_state(to, &block.header.parent_hash).await?;
            prestate.insert(format!("{:?}", to), to_state);
        }
        
        Ok(serde_json::Value::Object(prestate))
    }
    
    /// Trace 4byte signatures
    async fn trace_4byte(
        &self,
        tx: &Transaction,
        block: &Block,
    ) -> Result<serde_json::Value> {
        let mut signatures = serde_json::Map::new();
        
        // Extract 4-byte signature from input
        if tx.input.len() >= 4 {
            let sig = hex::encode(&tx.input[..4]);
            *signatures.entry(sig).or_insert(serde_json::Value::Number(0.into())) = 
                serde_json::Value::Number(1.into());
        }
        
        Ok(serde_json::Value::Object(signatures))
    }
    
    // Helper methods
    
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
        // Get block to get state root
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
    
    fn get_sender(&self, tx: &Transaction) -> Result<Address> {
        // Recover sender from signature
        Ok(Address::from([1u8; 20])) // Simplified
    }
    
    async fn get_account_state(
        &self,
        address: Address,
        block_hash: &H256,
    ) -> Result<serde_json::Value> {
        let state = self.get_state_at_block(block_hash).await?;
        
        // Get account data
        let account_data = state.get(address.as_bytes()).await
            .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
        
        if let Some(data) = account_data {
            let account: ethereum_core::Account = bincode::deserialize(&data)
                .map_err(|e| DebugError::ExecutionError(e.to_string()))?;
            
            Ok(serde_json::json!({
                "balance": format!("{:#x}", account.balance),
                "nonce": account.nonce,
                "code": format!("0x{}", hex::encode(account.code)),
                "storage": {}
            }))
        } else {
            Ok(serde_json::json!({
                "balance": "0x0",
                "nonce": 0,
                "code": "0x",
                "storage": {}
            }))
        }
    }
    
    fn extract_subcall(
        &self,
        op: Opcode,
        stack: &[H256],
        memory: &[u8],
    ) -> Option<CallTrace> {
        // Extract call parameters from stack
        // This is simplified - real implementation would properly decode
        
        Some(CallTrace {
            from: Address::zero(),
            to: Some(Address::zero()),
            value: U256::zero(),
            gas: U256::zero(),
            gas_used: U256::zero(),
            input: Vec::new(),
            output: Vec::new(),
            error: None,
            revert_reason: None,
            calls: Vec::new(),
            trace_type: match op {
                Opcode::DELEGATECALL => TraceType::Delegatecall,
                Opcode::STATICCALL => TraceType::Staticcall,
                _ => TraceType::Call,
            },
        })
    }
    
    fn extract_create(
        &self,
        op: Opcode,
        stack: &[H256],
        memory: &[u8],
    ) -> Option<CallTrace> {
        Some(CallTrace {
            from: Address::zero(),
            to: None,
            value: U256::zero(),
            gas: U256::zero(),
            gas_used: U256::zero(),
            input: Vec::new(),
            output: Vec::new(),
            error: None,
            revert_reason: None,
            calls: Vec::new(),
            trace_type: if op == Opcode::CREATE2 {
                TraceType::Create2
            } else {
                TraceType::Create
            },
        })
    }
    
    fn decode_revert_reason(&self, data: &[u8]) -> Option<String> {
        // Decode revert reason from return data
        // Standard format: 0x08c379a0 (Error(string)) followed by ABI-encoded string
        
        if data.len() < 4 {
            return None;
        }
        
        let selector = &data[..4];
        if selector == [0x08, 0xc3, 0x79, 0xa0] {
            // Try to decode string
            // This is simplified - real implementation would use proper ABI decoding
            Some("Execution reverted".to_string())
        } else {
            None
        }
    }
}