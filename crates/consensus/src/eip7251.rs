use ethereum_types::{Address, H256, U256};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// EIP-7251: Increase the MAX_EFFECTIVE_BALANCE
/// Changes the maximum effective balance for validators from 32 ETH to 2048 ETH

pub const MIN_ACTIVATION_BALANCE: U256 = U256([32_000_000_000_000_000_000u64, 0, 0, 0]); // 32 ETH
pub const MAX_EFFECTIVE_BALANCE_EIP7251: U256 = U256([2048_000_000_000_000_000_000u64, 0, 0, 0]); // 2048 ETH
pub const OLD_MAX_EFFECTIVE_BALANCE: U256 = U256([32_000_000_000_000_000_000u64, 0, 0, 0]); // 32 ETH

#[derive(Debug, Error)]
pub enum Eip7251Error {
    #[error("Invalid balance: {0}")]
    InvalidBalance(String),
    
    #[error("Below minimum activation balance")]
    BelowMinimumBalance,
    
    #[error("Exceeds maximum effective balance")]
    ExceedsMaximumBalance,
    
    #[error("Invalid withdrawal credential")]
    InvalidWithdrawalCredential,
    
    #[error("Validator not found")]
    ValidatorNotFound,
}

pub type Result<T> = std::result::Result<T, Eip7251Error>;

/// Withdrawal credential types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WithdrawalCredentialType {
    /// BLS withdrawal (0x00)
    Bls = 0x00,
    /// Eth1 withdrawal (0x01)
    Eth1 = 0x01,
    /// Compounding withdrawal (0x02) - New in EIP-7251
    Compounding = 0x02,
}

impl WithdrawalCredentialType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x00 => Some(Self::Bls),
            0x01 => Some(Self::Eth1),
            0x02 => Some(Self::Compounding),
            _ => None,
        }
    }
    
    pub fn to_byte(&self) -> u8 {
        *self as u8
    }
}

/// Enhanced validator struct supporting EIP-7251
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorEip7251 {
    pub pubkey: [u8; 48],
    pub withdrawal_credentials: [u8; 32],
    pub effective_balance: U256,
    pub slashed: bool,
    pub activation_eligibility_epoch: u64,
    pub activation_epoch: u64,
    pub exit_epoch: u64,
    pub withdrawable_epoch: u64,
    pub pending_balance_to_deposit: U256,
}

impl ValidatorEip7251 {
    pub fn new(
        pubkey: [u8; 48],
        withdrawal_credentials: [u8; 32],
        initial_balance: U256,
    ) -> Result<Self> {
        if initial_balance < MIN_ACTIVATION_BALANCE {
            return Err(Eip7251Error::BelowMinimumBalance);
        }
        
        let withdrawal_type = WithdrawalCredentialType::from_byte(withdrawal_credentials[0])
            .ok_or(Eip7251Error::InvalidWithdrawalCredential)?;
        
        let max_effective = match withdrawal_type {
            WithdrawalCredentialType::Compounding => MAX_EFFECTIVE_BALANCE_EIP7251,
            _ => OLD_MAX_EFFECTIVE_BALANCE,
        };
        
        let effective_balance = if initial_balance > max_effective {
            max_effective
        } else {
            initial_balance
        };
        
        Ok(Self {
            pubkey,
            withdrawal_credentials,
            effective_balance,
            slashed: false,
            activation_eligibility_epoch: u64::MAX,
            activation_epoch: u64::MAX,
            exit_epoch: u64::MAX,
            withdrawable_epoch: u64::MAX,
            pending_balance_to_deposit: U256::zero(),
        })
    }
    
    pub fn get_withdrawal_credential_type(&self) -> Option<WithdrawalCredentialType> {
        WithdrawalCredentialType::from_byte(self.withdrawal_credentials[0])
    }
    
    pub fn is_compounding(&self) -> bool {
        self.get_withdrawal_credential_type() == Some(WithdrawalCredentialType::Compounding)
    }
    
    pub fn get_max_effective_balance(&self) -> U256 {
        if self.is_compounding() {
            MAX_EFFECTIVE_BALANCE_EIP7251
        } else {
            OLD_MAX_EFFECTIVE_BALANCE
        }
    }
    
    pub fn add_balance(&mut self, amount: U256) -> Result<()> {
        let new_balance = self.effective_balance + amount;
        let max_effective = self.get_max_effective_balance();
        
        if new_balance > max_effective {
            self.effective_balance = max_effective;
            self.pending_balance_to_deposit = new_balance - max_effective;
        } else {
            self.effective_balance = new_balance;
        }
        
        Ok(())
    }
    
    pub fn consolidate_balance(&mut self) -> U256 {
        let consolidatable = self.pending_balance_to_deposit;
        let max_effective = self.get_max_effective_balance();
        
        let new_effective = self.effective_balance + consolidatable;
        if new_effective > max_effective {
            self.effective_balance = max_effective;
            self.pending_balance_to_deposit = new_effective - max_effective;
        } else {
            self.effective_balance = new_effective;
            self.pending_balance_to_deposit = U256::zero();
        }
        
        consolidatable
    }
    
    pub fn switch_to_compounding_validator(&mut self) -> Result<()> {
        if self.get_withdrawal_credential_type() != Some(WithdrawalCredentialType::Eth1) {
            return Err(Eip7251Error::InvalidWithdrawalCredential);
        }
        
        // Switch withdrawal credential type to compounding
        self.withdrawal_credentials[0] = WithdrawalCredentialType::Compounding as u8;
        
        // Consolidate any pending balance
        self.consolidate_balance();
        
        Ok(())
    }
    
    pub fn is_active(&self, epoch: u64) -> bool {
        self.activation_epoch <= epoch && epoch < self.exit_epoch
    }
    
    pub fn is_eligible_for_activation(&self, finalized_epoch: u64) -> bool {
        self.activation_eligibility_epoch <= finalized_epoch
            && self.activation_epoch == u64::MAX
    }
    
    pub fn initiate_exit(&mut self, current_epoch: u64) {
        if self.exit_epoch == u64::MAX {
            self.exit_epoch = current_epoch;
        }
    }
}

/// Validator registry supporting EIP-7251
#[derive(Debug, Clone)]
pub struct ValidatorRegistry {
    validators: Vec<ValidatorEip7251>,
    balances: Vec<U256>,
}

impl ValidatorRegistry {
    pub fn new() -> Self {
        Self {
            validators: Vec::new(),
            balances: Vec::new(),
        }
    }
    
    pub fn add_validator(&mut self, validator: ValidatorEip7251, balance: U256) -> usize {
        let index = self.validators.len();
        self.validators.push(validator);
        self.balances.push(balance);
        index
    }
    
    pub fn get_validator(&self, index: usize) -> Option<&ValidatorEip7251> {
        self.validators.get(index)
    }
    
    pub fn get_validator_mut(&mut self, index: usize) -> Option<&mut ValidatorEip7251> {
        self.validators.get_mut(index)
    }
    
    pub fn get_balance(&self, index: usize) -> Option<U256> {
        self.balances.get(index).copied()
    }
    
    pub fn set_balance(&mut self, index: usize, balance: U256) -> Result<()> {
        if index >= self.balances.len() {
            return Err(Eip7251Error::ValidatorNotFound);
        }
        self.balances[index] = balance;
        Ok(())
    }
    
    pub fn increase_balance(&mut self, index: usize, delta: U256) -> Result<()> {
        if index >= self.balances.len() {
            return Err(Eip7251Error::ValidatorNotFound);
        }
        
        self.balances[index] = self.balances[index] + delta;
        
        // Update effective balance if needed
        if let Some(validator) = self.validators.get_mut(index) {
            validator.add_balance(delta)?;
        }
        
        Ok(())
    }
    
    pub fn decrease_balance(&mut self, index: usize, delta: U256) -> Result<()> {
        if index >= self.balances.len() {
            return Err(Eip7251Error::ValidatorNotFound);
        }
        
        if self.balances[index] < delta {
            self.balances[index] = U256::zero();
        } else {
            self.balances[index] = self.balances[index] - delta;
        }
        
        // Update effective balance
        if let Some(validator) = self.validators.get_mut(index) {
            if self.balances[index] < validator.effective_balance {
                validator.effective_balance = self.balances[index];
            }
        }
        
        Ok(())
    }
    
    pub fn get_active_validator_indices(&self, epoch: u64) -> Vec<usize> {
        self.validators
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_active(epoch))
            .map(|(i, _)| i)
            .collect()
    }
    
    pub fn get_total_active_balance(&self, epoch: u64) -> U256 {
        self.get_active_validator_indices(epoch)
            .iter()
            .filter_map(|&i| self.validators.get(i))
            .map(|v| v.effective_balance)
            .fold(U256::zero(), |acc, b| acc + b)
    }
    
    pub fn process_pending_balance_deposits(&mut self, max_deposits: usize) -> Vec<(usize, U256)> {
        let mut processed = Vec::new();
        let mut count = 0;
        
        for (index, validator) in self.validators.iter_mut().enumerate() {
            if count >= max_deposits {
                break;
            }
            
            if validator.pending_balance_to_deposit > U256::zero() {
                let amount = validator.consolidate_balance();
                if amount > U256::zero() {
                    processed.push((index, amount));
                    count += 1;
                }
            }
        }
        
        processed
    }
}

/// Consolidation request for validator consolidation (EIP-7251)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsolidationRequest {
    pub source_address: Address,
    pub source_pubkey: [u8; 48],
    pub target_pubkey: [u8; 48],
}

impl ConsolidationRequest {
    pub fn new(
        source_address: Address,
        source_pubkey: [u8; 48],
        target_pubkey: [u8; 48],
    ) -> Self {
        Self {
            source_address,
            source_pubkey,
            target_pubkey,
        }
    }
    
    pub fn process(
        &self,
        registry: &mut ValidatorRegistry,
    ) -> Result<()> {
        // Find source and target validators
        let source_index = registry.validators
            .iter()
            .position(|v| v.pubkey == self.source_pubkey)
            .ok_or(Eip7251Error::ValidatorNotFound)?;
            
        let target_index = registry.validators
            .iter()
            .position(|v| v.pubkey == self.target_pubkey)
            .ok_or(Eip7251Error::ValidatorNotFound)?;
        
        // Get source balance
        let source_balance = registry.get_balance(source_index)
            .ok_or(Eip7251Error::ValidatorNotFound)?;
        
        // Exit source validator
        registry.validators[source_index].initiate_exit(0);
        
        // Add balance to target
        registry.increase_balance(target_index, source_balance)?;
        
        // Switch target to compounding if needed
        if let Some(target) = registry.get_validator_mut(target_index) {
            if !target.is_compounding() {
                target.switch_to_compounding_validator()?;
            }
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validator_creation() {
        let pubkey = [0u8; 48];
        let mut withdrawal_creds = [0u8; 32];
        withdrawal_creds[0] = WithdrawalCredentialType::Eth1 as u8;
        
        let validator = ValidatorEip7251::new(
            pubkey,
            withdrawal_creds,
            U256::from(32_000_000_000_000_000_000u64),
        ).unwrap();
        
        assert_eq!(validator.effective_balance, OLD_MAX_EFFECTIVE_BALANCE);
        assert!(!validator.is_compounding());
    }
    
    #[test]
    fn test_compounding_validator() {
        let pubkey = [0u8; 48];
        let mut withdrawal_creds = [0u8; 32];
        withdrawal_creds[0] = WithdrawalCredentialType::Compounding as u8;
        
        let validator = ValidatorEip7251::new(
            pubkey,
            withdrawal_creds,
            U256::from(100_000_000_000_000_000_000u64), // 100 ETH
        ).unwrap();
        
        assert_eq!(validator.effective_balance, U256::from(100_000_000_000_000_000_000u64));
        assert!(validator.is_compounding());
    }
    
    #[test]
    fn test_switch_to_compounding() {
        let pubkey = [0u8; 48];
        let mut withdrawal_creds = [0u8; 32];
        withdrawal_creds[0] = WithdrawalCredentialType::Eth1 as u8;
        
        let mut validator = ValidatorEip7251::new(
            pubkey,
            withdrawal_creds,
            U256::from(32_000_000_000_000_000_000u64),
        ).unwrap();
        
        validator.switch_to_compounding_validator().unwrap();
        assert!(validator.is_compounding());
        assert_eq!(validator.get_max_effective_balance(), MAX_EFFECTIVE_BALANCE_EIP7251);
    }
    
    #[test]
    fn test_balance_consolidation() {
        let mut registry = ValidatorRegistry::new();
        
        let pubkey = [1u8; 48];
        let mut withdrawal_creds = [0u8; 32];
        withdrawal_creds[0] = WithdrawalCredentialType::Compounding as u8;
        
        let validator = ValidatorEip7251::new(
            pubkey,
            withdrawal_creds,
            U256::from(1000_000_000_000_000_000_000u64), // 1000 ETH
        ).unwrap();
        
        registry.add_validator(validator, U256::from(1000_000_000_000_000_000_000u64));
        
        // Add more balance
        registry.increase_balance(0, U256::from(1500_000_000_000_000_000_000u64)).unwrap();
        
        let processed = registry.process_pending_balance_deposits(10);
        assert!(processed.len() > 0);
    }
}