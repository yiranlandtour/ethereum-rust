use ethereum_types::{H256, U256, Address};
use ethereum_core::Transaction;
use ethereum_crypto::{recover_address, keccak256};

use crate::{Result, VerificationError};

/// Transaction verifier
pub struct TransactionVerifier {
    chain_id: u64,
}

impl TransactionVerifier {
    pub fn new(chain_id: u64) -> Self {
        Self { chain_id }
    }
    
    /// Verify transaction
    pub fn verify(&self, tx: &Transaction) -> Result<()> {
        // Verify signature
        self.verify_signature(tx)?;
        
        // Verify chain ID (EIP-155)
        self.verify_chain_id(tx)?;
        
        // Verify gas parameters
        self.verify_gas_parameters(tx)?;
        
        // Verify transaction type
        self.verify_transaction_type(tx)?;
        
        // Verify nonce (basic check)
        self.verify_nonce(tx)?;
        
        Ok(())
    }
    
    /// Verify transaction signature
    fn verify_signature(&self, tx: &Transaction) -> Result<()> {
        if !tx.signature.is_valid() {
            return Err(VerificationError::InvalidTransaction(
                "Invalid signature".to_string()
            ));
        }
        
        // Verify we can recover sender
        let sender = self.recover_sender(tx)?;
        
        // Sender must not be zero address
        if sender == Address::zero() {
            return Err(VerificationError::InvalidTransaction(
                "Sender is zero address".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Recover transaction sender
    fn recover_sender(&self, tx: &Transaction) -> Result<Address> {
        let message = self.signing_hash(tx);
        
        recover_address(&message, &tx.signature)
            .map_err(|_| VerificationError::InvalidTransaction(
                "Failed to recover sender".to_string()
            ))
    }
    
    /// Calculate signing hash for transaction
    fn signing_hash(&self, tx: &Transaction) -> [u8; 32] {
        // Build message based on transaction type
        let mut data = Vec::new();
        
        // Add transaction fields
        data.extend_from_slice(&tx.nonce.to_le_bytes());
        
        if let Some(gas_price) = tx.gas_price {
            data.extend_from_slice(&gas_price.to_le_bytes());
        } else if let Some(max_fee) = tx.max_fee_per_gas {
            data.extend_from_slice(&max_fee.to_le_bytes());
            if let Some(priority_fee) = tx.max_priority_fee_per_gas {
                data.extend_from_slice(&priority_fee.to_le_bytes());
            }
        }
        
        data.extend_from_slice(&tx.gas_limit.to_le_bytes());
        
        if let Some(to) = tx.to {
            data.extend_from_slice(to.as_bytes());
        }
        
        data.extend_from_slice(&tx.value.to_le_bytes());
        data.extend_from_slice(&tx.input);
        
        // Add chain ID for EIP-155
        if let Some(chain_id) = tx.chain_id {
            data.extend_from_slice(&chain_id.to_le_bytes());
        }
        
        keccak256(&data)
    }
    
    /// Verify chain ID
    fn verify_chain_id(&self, tx: &Transaction) -> Result<()> {
        // For legacy transactions, chain ID is optional
        if tx.transaction_type == 0 {
            return Ok(());
        }
        
        // For EIP-155 and later, chain ID must match
        if let Some(tx_chain_id) = tx.chain_id {
            if tx_chain_id != self.chain_id {
                return Err(VerificationError::InvalidTransaction(
                    format!("Wrong chain ID: expected {}, got {}", 
                            self.chain_id, tx_chain_id)
                ));
            }
        } else if tx.transaction_type > 0 {
            return Err(VerificationError::InvalidTransaction(
                "Missing chain ID for typed transaction".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Verify gas parameters
    fn verify_gas_parameters(&self, tx: &Transaction) -> Result<()> {
        // Check gas limit
        const MIN_GAS: u64 = 21000; // Minimum gas for simple transfer
        const MAX_GAS: u64 = 30_000_000; // Maximum block gas limit
        
        if tx.gas_limit < U256::from(MIN_GAS) {
            return Err(VerificationError::InvalidTransaction(
                format!("Gas limit too low: {} < {}", tx.gas_limit, MIN_GAS)
            ));
        }
        
        if tx.gas_limit > U256::from(MAX_GAS) {
            return Err(VerificationError::InvalidTransaction(
                format!("Gas limit too high: {} > {}", tx.gas_limit, MAX_GAS)
            ));
        }
        
        // Check gas price parameters based on transaction type
        match tx.transaction_type {
            0 | 1 => {
                // Legacy or EIP-2930
                if tx.gas_price.is_none() || tx.gas_price == Some(U256::zero()) {
                    return Err(VerificationError::InvalidTransaction(
                        "Gas price cannot be zero".to_string()
                    ));
                }
            }
            2 => {
                // EIP-1559
                if tx.max_fee_per_gas.is_none() || tx.max_priority_fee_per_gas.is_none() {
                    return Err(VerificationError::InvalidTransaction(
                        "Missing EIP-1559 gas parameters".to_string()
                    ));
                }
                
                let max_fee = tx.max_fee_per_gas.unwrap();
                let priority_fee = tx.max_priority_fee_per_gas.unwrap();
                
                if max_fee < priority_fee {
                    return Err(VerificationError::InvalidTransaction(
                        "Max fee less than priority fee".to_string()
                    ));
                }
            }
            _ => {
                return Err(VerificationError::InvalidTransaction(
                    format!("Unknown transaction type: {}", tx.transaction_type)
                ));
            }
        }
        
        Ok(())
    }
    
    /// Verify transaction type
    fn verify_transaction_type(&self, tx: &Transaction) -> Result<()> {
        // Currently support types 0 (legacy), 1 (EIP-2930), 2 (EIP-1559)
        if tx.transaction_type > 2 {
            return Err(VerificationError::InvalidTransaction(
                format!("Unsupported transaction type: {}", tx.transaction_type)
            ));
        }
        
        // Verify access list for type 1 and 2
        if tx.transaction_type >= 1 && tx.access_list.is_none() {
            return Err(VerificationError::InvalidTransaction(
                "Missing access list for typed transaction".to_string()
            ));
        }
        
        Ok(())
    }
    
    /// Verify nonce
    fn verify_nonce(&self, tx: &Transaction) -> Result<()> {
        // Basic check - nonce should not be unreasonably high
        const MAX_NONCE: u64 = u64::MAX / 2;
        
        if tx.nonce > MAX_NONCE {
            return Err(VerificationError::InvalidTransaction(
                format!("Nonce too high: {} > {}", tx.nonce, MAX_NONCE)
            ));
        }
        
        // Account state check would be done during execution
        
        Ok(())
    }
    
    /// Verify transaction for mempool inclusion
    pub fn verify_for_mempool(&self, tx: &Transaction) -> Result<()> {
        // Basic verification
        self.verify(tx)?;
        
        // Additional mempool-specific checks
        
        // Check transaction is not too large
        let tx_size = bincode::serialize(tx)
            .map_err(|_| VerificationError::InvalidTransaction("Failed to serialize".to_string()))?
            .len();
        
        const MAX_TX_SIZE: usize = 128 * 1024; // 128KB
        if tx_size > MAX_TX_SIZE {
            return Err(VerificationError::InvalidTransaction(
                format!("Transaction too large: {} > {}", tx_size, MAX_TX_SIZE)
            ));
        }
        
        Ok(())
    }
}