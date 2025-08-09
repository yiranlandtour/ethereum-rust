use ethereum_types::{H256, Address};
use ethereum_crypto::Signature;
use secp256k1::{SecretKey, PublicKey, Secp256k1, Message};
use std::path::Path;
use std::collections::HashMap;
use thiserror::Error;

pub mod keystore;
pub mod wallet;
pub mod signer;

pub use keystore::{KeyStore, KeyFile, CryptoParams};
pub use wallet::{Wallet, HDWallet};
pub use signer::{Signer, TransactionSigner};

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("Invalid password")]
    InvalidPassword,
    
    #[error("Account not found")]
    AccountNotFound,
    
    #[error("Invalid keyfile")]
    InvalidKeyFile,
    
    #[error("Keystore error: {0}")]
    KeystoreError(String),
    
    #[error("Signing error: {0}")]
    SigningError(String),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(#[from] secp256k1::Error),
    
    #[error("BIP39 error: {0}")]
    Bip39Error(#[from] bip39::Error),
    
    #[error("BIP32 error: {0}")]
    Bip32Error(#[from] bip32::Error),
}

pub type Result<T> = std::result::Result<T, AccountError>;

/// Account represents an Ethereum account
#[derive(Debug, Clone)]
pub struct Account {
    address: Address,
    private_key: SecretKey,
    public_key: PublicKey,
}

impl Account {
    /// Create a new random account
    pub fn new() -> Result<Self> {
        let secp = Secp256k1::new();
        let private_key = SecretKey::new(&mut rand::thread_rng());
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        let address = public_key_to_address(&public_key);
        
        Ok(Self {
            address,
            private_key,
            public_key,
        })
    }
    
    /// Create account from private key
    pub fn from_private_key(private_key: SecretKey) -> Result<Self> {
        let secp = Secp256k1::new();
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        let address = public_key_to_address(&public_key);
        
        Ok(Self {
            address,
            private_key,
            public_key,
        })
    }
    
    /// Create account from private key bytes
    pub fn from_private_key_bytes(bytes: &[u8]) -> Result<Self> {
        let private_key = SecretKey::from_slice(bytes)?;
        Self::from_private_key(private_key)
    }
    
    /// Get account address
    pub fn address(&self) -> Address {
        self.address
    }
    
    /// Get private key
    pub fn private_key(&self) -> &SecretKey {
        &self.private_key
    }
    
    /// Get public key
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }
    
    /// Sign a message
    pub fn sign_message(&self, message: &[u8]) -> Result<Signature> {
        let secp = Secp256k1::new();
        let msg_hash = ethereum_crypto::keccak256(message);
        let message = Message::from_slice(msg_hash.as_bytes())
            .map_err(|e| AccountError::SigningError(e.to_string()))?;
        
        let sig = secp.sign_ecdsa_recoverable(&message, &self.private_key);
        let (recovery_id, sig_bytes) = sig.serialize_compact();
        
        // Create signature with recovery id
        let mut signature_bytes = [0u8; 65];
        signature_bytes[..64].copy_from_slice(&sig_bytes);
        signature_bytes[64] = recovery_id.to_i32() as u8;
        
        Signature::from_bytes(&signature_bytes)
            .map_err(|e| AccountError::SigningError(e.to_string()))
    }
    
    /// Sign a transaction hash
    pub fn sign_transaction_hash(&self, tx_hash: &H256) -> Result<Signature> {
        let secp = Secp256k1::new();
        let message = Message::from_slice(tx_hash.as_bytes())
            .map_err(|e| AccountError::SigningError(e.to_string()))?;
        
        let sig = secp.sign_ecdsa_recoverable(&message, &self.private_key);
        let (recovery_id, sig_bytes) = sig.serialize_compact();
        
        // Create signature with recovery id
        let mut signature_bytes = [0u8; 65];
        signature_bytes[..64].copy_from_slice(&sig_bytes);
        signature_bytes[64] = recovery_id.to_i32() as u8;
        
        Signature::from_bytes(&signature_bytes)
            .map_err(|e| AccountError::SigningError(e.to_string()))
    }
    
    /// Verify a signature
    pub fn verify_signature(&self, message: &[u8], signature: &Signature) -> bool {
        let secp = Secp256k1::new();
        let msg_hash = ethereum_crypto::keccak256(message);
        
        if let Ok(message) = Message::from_slice(msg_hash.as_bytes()) {
            // Extract r, s, v from signature
            let sig_bytes = signature.to_bytes();
            if sig_bytes.len() == 65 {
                if let Some(recovery_id) = secp256k1::ecdsa::RecoveryId::from_i32(sig_bytes[64] as i32).ok() {
                    if let Ok(sig) = secp256k1::ecdsa::RecoverableSignature::from_compact(&sig_bytes[..64], recovery_id) {
                        if let Ok(pubkey) = secp.recover_ecdsa(&message, &sig) {
                            return pubkey == self.public_key;
                        }
                    }
                }
            }
        }
        
        false
    }
}

/// Account manager handles multiple accounts
pub struct AccountManager {
    accounts: HashMap<Address, Account>,
    keystore: KeyStore,
    default_account: Option<Address>,
}

impl AccountManager {
    /// Create a new account manager
    pub fn new<P: AsRef<Path>>(keystore_dir: P) -> Result<Self> {
        let keystore = KeyStore::new(keystore_dir)?;
        
        Ok(Self {
            accounts: HashMap::new(),
            keystore,
            default_account: None,
        })
    }
    
    /// Create a new account with password
    pub async fn new_account(&mut self, password: &str) -> Result<Address> {
        let account = Account::new()?;
        let address = account.address();
        
        // Store in keystore
        self.keystore.store_account(&account, password).await?;
        
        // Add to memory
        self.accounts.insert(address, account);
        
        // Set as default if first account
        if self.default_account.is_none() {
            self.default_account = Some(address);
        }
        
        Ok(address)
    }
    
    /// Import account from private key
    pub async fn import_private_key(
        &mut self,
        private_key: &str,
        password: &str,
    ) -> Result<Address> {
        let key_bytes = hex::decode(private_key.trim_start_matches("0x"))
            .map_err(|e| AccountError::InvalidKeyFile)?;
        
        let account = Account::from_private_key_bytes(&key_bytes)?;
        let address = account.address();
        
        // Store in keystore
        self.keystore.store_account(&account, password).await?;
        
        // Add to memory
        self.accounts.insert(address, account);
        
        Ok(address)
    }
    
    /// Import account from keyfile
    pub async fn import_keyfile(
        &mut self,
        keyfile_path: &Path,
        password: &str,
    ) -> Result<Address> {
        let account = self.keystore.load_from_file(keyfile_path, password).await?;
        let address = account.address();
        
        // Add to memory
        self.accounts.insert(address, account);
        
        Ok(address)
    }
    
    /// Unlock account with password
    pub async fn unlock_account(
        &mut self,
        address: Address,
        password: &str,
    ) -> Result<()> {
        if self.accounts.contains_key(&address) {
            return Ok(()); // Already unlocked
        }
        
        let account = self.keystore.unlock_account(address, password).await?;
        self.accounts.insert(address, account);
        
        Ok(())
    }
    
    /// Lock account
    pub fn lock_account(&mut self, address: Address) {
        self.accounts.remove(&address);
    }
    
    /// Get account
    pub fn get_account(&self, address: Address) -> Option<&Account> {
        self.accounts.get(&address)
    }
    
    /// Get default account
    pub fn default_account(&self) -> Option<Address> {
        self.default_account
    }
    
    /// Set default account
    pub fn set_default_account(&mut self, address: Address) -> Result<()> {
        if self.keystore.has_account(address) {
            self.default_account = Some(address);
            Ok(())
        } else {
            Err(AccountError::AccountNotFound)
        }
    }
    
    /// List all accounts
    pub fn list_accounts(&self) -> Vec<Address> {
        self.keystore.list_accounts()
    }
    
    /// Sign message with account
    pub fn sign_message(
        &self,
        address: Address,
        message: &[u8],
    ) -> Result<Signature> {
        let account = self.accounts.get(&address)
            .ok_or(AccountError::AccountNotFound)?;
        
        account.sign_message(message)
    }
    
    /// Sign transaction with account
    pub fn sign_transaction(
        &self,
        address: Address,
        tx_hash: &H256,
    ) -> Result<Signature> {
        let account = self.accounts.get(&address)
            .ok_or(AccountError::AccountNotFound)?;
        
        account.sign_transaction_hash(tx_hash)
    }
    
    /// Export account as keyfile
    pub async fn export_account(
        &self,
        address: Address,
        password: &str,
        output_path: &Path,
    ) -> Result<()> {
        self.keystore.export_account(address, password, output_path).await
    }
}

/// Convert public key to Ethereum address
pub fn public_key_to_address(public_key: &PublicKey) -> Address {
    let public_key_bytes = public_key.serialize_uncompressed();
    // Skip the first byte (0x04) and hash the remaining 64 bytes
    let hash = ethereum_crypto::keccak256(&public_key_bytes[1..]);
    // Take the last 20 bytes of the hash
    Address::from_slice(&hash[12..]).expect("keccak256 hash should be 32 bytes")
}

/// Ethereum address checksum encoding (EIP-55)
pub fn to_checksum_address(address: &Address) -> String {
    let hex_address = hex::encode(address.as_bytes());
    let hash = ethereum_crypto::keccak256(hex_address.as_bytes());
    
    let mut checksum = String::with_capacity(42);
    checksum.push_str("0x");
    
    for (i, ch) in hex_address.chars().enumerate() {
        if ch.is_alphabetic() {
            let hash_byte = hash[i / 2];
            let hash_nibble = if i % 2 == 0 {
                hash_byte >> 4
            } else {
                hash_byte & 0xf
            };
            
            if hash_nibble >= 8 {
                checksum.push(ch.to_ascii_uppercase());
            } else {
                checksum.push(ch.to_ascii_lowercase());
            }
        } else {
            checksum.push(ch);
        }
    }
    
    checksum
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_account_creation() {
        let account = Account::new().unwrap();
        assert_ne!(account.address(), Address::zero());
    }
    
    #[test]
    fn test_sign_and_verify() {
        let account = Account::new().unwrap();
        let message = b"Hello, Ethereum!";
        
        let signature = account.sign_message(message).unwrap();
        assert!(account.verify_signature(message, &signature));
    }
    
    #[test]
    fn test_checksum_address() {
        let address = Address::from_slice(&hex::decode("5aaeb6053f3e94c9b9a09f33669435e7ef1beaed").unwrap());
        let checksum = to_checksum_address(&address);
        assert_eq!(checksum, "0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed");
    }
}