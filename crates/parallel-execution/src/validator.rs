use ethereum_types::{H256, U256};
use ethereum_core::Receipt;
use std::collections::HashMap;

use crate::{Result, ParallelExecutionError};
use crate::executor::ExecutedTransaction;

/// Parallel execution validator
pub struct ParallelValidator {
    sequential_results: HashMap<H256, Receipt>,
}

impl ParallelValidator {
    pub fn new() -> Self {
        Self {
            sequential_results: HashMap::new(),
        }
    }
    
    /// Validate parallel execution results against sequential execution
    pub fn validate(
        &self,
        parallel_results: &[ExecutedTransaction],
        sequential_results: &[ExecutedTransaction],
    ) -> Result<ValidationResult> {
        if parallel_results.len() != sequential_results.len() {
            return Ok(ValidationResult {
                is_valid: false,
                mismatches: vec![ValidationMismatch::CountMismatch {
                    parallel: parallel_results.len(),
                    sequential: sequential_results.len(),
                }],
            });
        }
        
        let mut mismatches = Vec::new();
        
        for (parallel, sequential) in parallel_results.iter().zip(sequential_results.iter()) {
            // Check transaction hash
            if parallel.tx_hash != sequential.tx_hash {
                mismatches.push(ValidationMismatch::TransactionMismatch {
                    parallel_hash: parallel.tx_hash,
                    sequential_hash: sequential.tx_hash,
                });
            }
            
            // Check receipt status
            if parallel.receipt.status != sequential.receipt.status {
                mismatches.push(ValidationMismatch::StatusMismatch {
                    tx_hash: parallel.tx_hash,
                    parallel_status: parallel.receipt.status,
                    sequential_status: sequential.receipt.status,
                });
            }
            
            // Check gas used
            if parallel.receipt.gas_used != sequential.receipt.gas_used {
                mismatches.push(ValidationMismatch::GasMismatch {
                    tx_hash: parallel.tx_hash,
                    parallel_gas: parallel.receipt.gas_used,
                    sequential_gas: sequential.receipt.gas_used,
                });
            }
            
            // Check logs
            if parallel.receipt.logs.len() != sequential.receipt.logs.len() {
                mismatches.push(ValidationMismatch::LogMismatch {
                    tx_hash: parallel.tx_hash,
                    parallel_logs: parallel.receipt.logs.len(),
                    sequential_logs: sequential.receipt.logs.len(),
                });
            }
        }
        
        Ok(ValidationResult {
            is_valid: mismatches.is_empty(),
            mismatches,
        })
    }
    
    /// Add sequential result for comparison
    pub fn add_sequential_result(&mut self, tx_hash: H256, receipt: Receipt) {
        self.sequential_results.insert(tx_hash, receipt);
    }
    
    /// Validate single transaction
    pub fn validate_transaction(
        &self,
        tx_hash: H256,
        parallel_receipt: &Receipt,
    ) -> Result<bool> {
        if let Some(sequential_receipt) = self.sequential_results.get(&tx_hash) {
            Ok(Self::receipts_match(parallel_receipt, sequential_receipt))
        } else {
            Err(ParallelExecutionError::ValidationFailed(
                format!("No sequential result for transaction {:?}", tx_hash)
            ))
        }
    }
    
    fn receipts_match(r1: &Receipt, r2: &Receipt) -> bool {
        r1.status == r2.status &&
        r1.gas_used == r2.gas_used &&
        r1.logs.len() == r2.logs.len() &&
        r1.logs_bloom == r2.logs_bloom
    }
}

#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub mismatches: Vec<ValidationMismatch>,
}

#[derive(Debug, Clone)]
pub enum ValidationMismatch {
    CountMismatch {
        parallel: usize,
        sequential: usize,
    },
    TransactionMismatch {
        parallel_hash: H256,
        sequential_hash: H256,
    },
    StatusMismatch {
        tx_hash: H256,
        parallel_status: Option<U256>,
        sequential_status: Option<U256>,
    },
    GasMismatch {
        tx_hash: H256,
        parallel_gas: Option<U256>,
        sequential_gas: Option<U256>,
    },
    LogMismatch {
        tx_hash: H256,
        parallel_logs: usize,
        sequential_logs: usize,
    },
}