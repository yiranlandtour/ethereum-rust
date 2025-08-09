use ethereum_types::{H256, Address};
use ethereum_core::Transaction;
use ethereum_crypto::Signature;
use secp256k1::SecretKey;

use crate::{Account, AccountError, Result};

/// Transaction signer trait
pub trait Signer: Send + Sync {
    /// Sign a message
    fn sign_message(&self, message: &[u8]) -> Result<Signature>;
    
    /// Sign a transaction
    fn sign_transaction(&self, tx: &Transaction) -> Result<Transaction>;
    
    /// Get signer address
    fn address(&self) -> Address;
    
    /// Get chain ID
    fn chain_id(&self) -> Option<u64>;
}

/// Local signer using private key
pub struct LocalSigner {
    account: Account,
    chain_id: Option<u64>,
}

impl LocalSigner {
    /// Create a new local signer
    pub fn new(account: Account, chain_id: Option<u64>) -> Self {
        Self { account, chain_id }
    }
    
    /// Create from private key
    pub fn from_private_key(private_key: SecretKey, chain_id: Option<u64>) -> Result<Self> {
        let account = Account::from_private_key(private_key)?;
        Ok(Self { account, chain_id })
    }
    
    /// Create from private key string
    pub fn from_private_key_str(key: &str, chain_id: Option<u64>) -> Result<Self> {
        let key_bytes = hex::decode(key.trim_start_matches("0x"))
            .map_err(|_| AccountError::InvalidKeyFile)?;
        let account = Account::from_private_key_bytes(&key_bytes)?;
        Ok(Self { account, chain_id })
    }
}

impl Signer for LocalSigner {
    fn sign_message(&self, message: &[u8]) -> Result<Signature> {
        self.account.sign_message(message)
    }
    
    fn sign_transaction(&self, tx: &Transaction) -> Result<Transaction> {
        // Calculate transaction hash for signing
        let tx_hash = calculate_signing_hash(tx);
        
        // Sign the hash
        let signature = self.account.sign_transaction_hash(&tx_hash)?;
        
        // Create signed transaction by applying signature
        // This is simplified - in reality would need to properly encode based on transaction type
        let signed_tx = tx.clone();
        
        Ok(signed_tx)
    }
    
    fn address(&self) -> Address {
        self.account.address()
    }
    
    fn chain_id(&self) -> Option<u64> {
        self.chain_id
    }
}

/// Transaction signer for signing transactions
pub struct TransactionSigner {
    signers: Vec<Box<dyn Signer>>,
    default_signer: Option<usize>,
}

impl TransactionSigner {
    /// Create a new transaction signer
    pub fn new() -> Self {
        Self {
            signers: Vec::new(),
            default_signer: None,
        }
    }
    
    /// Add a signer
    pub fn add_signer(&mut self, signer: Box<dyn Signer>) -> usize {
        let index = self.signers.len();
        self.signers.push(signer);
        
        // Set as default if first signer
        if self.default_signer.is_none() {
            self.default_signer = Some(index);
        }
        
        index
    }
    
    /// Add local signer from private key
    pub fn add_local_signer(&mut self, private_key: SecretKey, chain_id: Option<u64>) -> Result<usize> {
        let signer = LocalSigner::from_private_key(private_key, chain_id)?;
        Ok(self.add_signer(Box::new(signer)))
    }
    
    /// Set default signer
    pub fn set_default_signer(&mut self, index: usize) -> Result<()> {
        if index >= self.signers.len() {
            return Err(AccountError::AccountNotFound);
        }
        self.default_signer = Some(index);
        Ok(())
    }
    
    /// Sign transaction with specific signer
    pub fn sign_transaction_with(&self, tx: &Transaction, signer_index: usize) -> Result<Transaction> {
        let signer = self.signers.get(signer_index)
            .ok_or(AccountError::AccountNotFound)?;
        
        signer.sign_transaction(tx)
    }
    
    /// Sign transaction with default signer
    pub fn sign_transaction(&self, tx: &Transaction) -> Result<Transaction> {
        let index = self.default_signer
            .ok_or(AccountError::AccountNotFound)?;
        
        self.sign_transaction_with(tx, index)
    }
    
    /// Sign transaction with specific address
    pub fn sign_transaction_from(&self, tx: &Transaction, from: Address) -> Result<Transaction> {
        for signer in &self.signers {
            if signer.address() == from {
                return signer.sign_transaction(tx);
            }
        }
        
        Err(AccountError::AccountNotFound)
    }
    
    /// List all signer addresses
    pub fn list_signers(&self) -> Vec<Address> {
        self.signers.iter()
            .map(|s| s.address())
            .collect()
    }
    
    /// Get signer by address
    pub fn get_signer(&self, address: Address) -> Option<&dyn Signer> {
        self.signers.iter()
            .find(|s| s.address() == address)
            .map(|s| s.as_ref())
    }
}

/// Calculate signing hash for transaction (EIP-155)
fn calculate_signing_hash(tx: &Transaction) -> H256 {
    // This is a simplified version
    // Real implementation would need to properly encode transaction based on type
    let mut data = Vec::new();
    
    // Add transaction fields
    data.extend_from_slice(&tx.nonce().to_le_bytes());
    
    if let Some(gas_price) = tx.gas_price() {
        data.extend_from_slice(&gas_price.to_le_bytes());
    }
    
    data.extend_from_slice(&tx.gas_limit().to_le_bytes());
    
    if let Some(to) = tx.to() {
        data.extend_from_slice(to.as_bytes());
    }
    
    data.extend_from_slice(&tx.value().to_le_bytes());
    data.extend_from_slice(tx.data());
    
    // Add chain ID for EIP-155
    if let Some(chain_id) = tx.chain_id() {
        data.extend_from_slice(&chain_id.to_le_bytes());
        data.extend_from_slice(&[0u8; 8]); // r = 0
        data.extend_from_slice(&[0u8; 8]); // s = 0
    }
    
    ethereum_crypto::keccak256(&data)
}

/// Hardware wallet signer (stub for future implementation)
pub struct HardwareWalletSigner {
    address: Address,
    chain_id: Option<u64>,
}

impl HardwareWalletSigner {
    pub fn new(address: Address, chain_id: Option<u64>) -> Self {
        Self { address, chain_id }
    }
}

impl Signer for HardwareWalletSigner {
    fn sign_message(&self, _message: &[u8]) -> Result<Signature> {
        // Hardware wallet signing would be implemented here
        Err(AccountError::SigningError("Hardware wallet not implemented".to_string()))
    }
    
    fn sign_transaction(&self, _tx: &Transaction) -> Result<Transaction> {
        // Hardware wallet signing would be implemented here
        Err(AccountError::SigningError("Hardware wallet not implemented".to_string()))
    }
    
    fn address(&self) -> Address {
        self.address
    }
    
    fn chain_id(&self) -> Option<u64> {
        self.chain_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_local_signer() {
        let account = Account::new().unwrap();
        let signer = LocalSigner::new(account.clone(), Some(1));
        
        assert_eq!(signer.address(), account.address());
        assert_eq!(signer.chain_id(), Some(1));
    }
    
    #[test]
    fn test_transaction_signer() {
        let mut tx_signer = TransactionSigner::new();
        
        let account1 = Account::new().unwrap();
        let account2 = Account::new().unwrap();
        
        let signer1 = LocalSigner::new(account1.clone(), Some(1));
        let signer2 = LocalSigner::new(account2.clone(), Some(1));
        
        let idx1 = tx_signer.add_signer(Box::new(signer1));
        let idx2 = tx_signer.add_signer(Box::new(signer2));
        
        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        
        let signers = tx_signer.list_signers();
        assert_eq!(signers.len(), 2);
        assert_eq!(signers[0], account1.address());
        assert_eq!(signers[1], account2.address());
    }
}