use ethereum_types::Address;
use secp256k1::{SecretKey, PublicKey, Secp256k1};
use bip39::{Mnemonic, Language};
use bip32::{XPrv, DerivationPath, Seed};
use std::str::FromStr;

use crate::{Account, AccountError, Result, public_key_to_address};

/// Simple wallet containing a single account
#[derive(Debug, Clone)]
pub struct Wallet {
    account: Account,
}

impl Wallet {
    /// Create a new random wallet
    pub fn new() -> Result<Self> {
        let account = Account::new()?;
        Ok(Self { account })
    }
    
    /// Create wallet from private key
    pub fn from_private_key(private_key: SecretKey) -> Result<Self> {
        let account = Account::from_private_key(private_key)?;
        Ok(Self { account })
    }
    
    /// Create wallet from private key string
    pub fn from_private_key_str(key: &str) -> Result<Self> {
        let key_bytes = hex::decode(key.trim_start_matches("0x"))
            .map_err(|_| AccountError::InvalidKeyFile)?;
        let account = Account::from_private_key_bytes(&key_bytes)?;
        Ok(Self { account })
    }
    
    /// Get wallet address
    pub fn address(&self) -> Address {
        self.account.address()
    }
    
    /// Get account
    pub fn account(&self) -> &Account {
        &self.account
    }
}

/// HD (Hierarchical Deterministic) wallet
#[derive(Debug, Clone)]
pub struct HDWallet {
    mnemonic: Mnemonic,
    seed: Seed,
    root_key: XPrv,
    accounts: Vec<HDAccount>,
}

/// HD Account with derivation path
#[derive(Debug, Clone)]
pub struct HDAccount {
    pub index: u32,
    pub address: Address,
    pub derivation_path: String,
    private_key: SecretKey,
    public_key: PublicKey,
}

impl HDWallet {
    /// Create a new HD wallet with random mnemonic
    pub fn new(word_count: usize) -> Result<Self> {
        let mnemonic = match word_count {
            12 => Mnemonic::generate(128),
            15 => Mnemonic::generate(160),
            18 => Mnemonic::generate(192),
            21 => Mnemonic::generate(224),
            24 => Mnemonic::generate(256),
            _ => return Err(AccountError::KeystoreError(
                "Invalid word count. Must be 12, 15, 18, 21, or 24".to_string()
            )),
        }?;
        
        Self::from_mnemonic(mnemonic, "")
    }
    
    /// Create HD wallet from mnemonic phrase
    pub fn from_mnemonic_str(mnemonic_str: &str, passphrase: &str) -> Result<Self> {
        let mnemonic = Mnemonic::from_phrase(mnemonic_str, Language::English)?;
        Self::from_mnemonic(mnemonic, passphrase)
    }
    
    /// Create HD wallet from mnemonic
    pub fn from_mnemonic(mnemonic: Mnemonic, passphrase: &str) -> Result<Self> {
        let seed = Seed::new(&mnemonic, passphrase);
        let root_key = XPrv::new(seed.as_bytes())
            .map_err(|e| AccountError::Bip32Error(e))?;
        
        Ok(Self {
            mnemonic,
            seed,
            root_key,
            accounts: Vec::new(),
        })
    }
    
    /// Get mnemonic phrase
    pub fn mnemonic_phrase(&self) -> &str {
        self.mnemonic.phrase()
    }
    
    /// Derive account at index using standard Ethereum derivation path
    /// m/44'/60'/0'/0/{index}
    pub fn derive_account(&mut self, index: u32) -> Result<Address> {
        let path = format!("m/44'/60'/0'/0/{}", index);
        self.derive_account_from_path(&path, index)
    }
    
    /// Derive account from custom derivation path
    pub fn derive_account_from_path(&mut self, path: &str, index: u32) -> Result<Address> {
        let derivation_path = DerivationPath::from_str(path)
            .map_err(|e| AccountError::Bip32Error(e))?;
        
        let child_key = self.root_key.derive_priv(&Secp256k1::new(), &derivation_path)?;
        let private_key = SecretKey::from_slice(&child_key.private_key().to_bytes())?;
        let public_key = PublicKey::from_secret_key(&Secp256k1::new(), &private_key);
        let address = public_key_to_address(&public_key);
        
        let account = HDAccount {
            index,
            address,
            derivation_path: path.to_string(),
            private_key,
            public_key,
        };
        
        self.accounts.push(account.clone());
        
        Ok(address)
    }
    
    /// Get account by address
    pub fn get_account(&self, address: Address) -> Option<&HDAccount> {
        self.accounts.iter().find(|a| a.address == address)
    }
    
    /// Get account by index
    pub fn get_account_by_index(&self, index: u32) -> Option<&HDAccount> {
        self.accounts.iter().find(|a| a.index == index)
    }
    
    /// List all derived accounts
    pub fn list_accounts(&self) -> Vec<(u32, Address)> {
        self.accounts.iter()
            .map(|a| (a.index, a.address))
            .collect()
    }
    
    /// Create standard Ethereum HD wallet (BIP-44)
    /// Derives first 10 accounts by default
    pub fn ethereum_wallet(mnemonic_str: Option<&str>, passphrase: &str) -> Result<Self> {
        let mut wallet = if let Some(mnemonic) = mnemonic_str {
            Self::from_mnemonic_str(mnemonic, passphrase)?
        } else {
            Self::new(24)?
        };
        
        // Derive first 10 accounts
        for i in 0..10 {
            wallet.derive_account(i)?;
        }
        
        Ok(wallet)
    }
    
    /// Export private key for account
    pub fn export_private_key(&self, address: Address) -> Option<String> {
        self.get_account(address)
            .map(|account| hex::encode(account.private_key.secret_bytes()))
    }
    
    /// Sign message with HD account
    pub fn sign_message(&self, address: Address, message: &[u8]) -> Result<ethereum_crypto::Signature> {
        let account = self.get_account(address)
            .ok_or(AccountError::AccountNotFound)?;
        
        let secp = Secp256k1::new();
        let msg_hash = ethereum_crypto::keccak256(message);
        let message = secp256k1::Message::from_slice(msg_hash.as_bytes())?;
        
        let sig = secp.sign_ecdsa_recoverable(&message, &account.private_key);
        let (recovery_id, sig_bytes) = sig.serialize_compact();
        
        // Create signature with recovery id
        let mut signature_bytes = [0u8; 65];
        signature_bytes[..64].copy_from_slice(&sig_bytes);
        signature_bytes[64] = recovery_id.to_i32() as u8;
        
        Ok(ethereum_crypto::Signature::from_bytes(&signature_bytes)
            .map_err(|e| AccountError::SigningError(e.to_string()))?)
    }
}

impl HDAccount {
    /// Convert to regular Account
    pub fn to_account(&self) -> Result<Account> {
        Account::from_private_key(self.private_key)
    }
    
    /// Get private key bytes
    pub fn private_key_bytes(&self) -> [u8; 32] {
        self.private_key.secret_bytes()
    }
    
    /// Get public key
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }
}

/// Ledger hardware wallet support (stub for future implementation)
pub struct LedgerWallet {
    // Hardware wallet integration would go here
}

/// Trezor hardware wallet support (stub for future implementation)
pub struct TrezorWallet {
    // Hardware wallet integration would go here
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_wallet_creation() {
        let wallet = Wallet::new().unwrap();
        assert_ne!(wallet.address(), Address::zero());
    }
    
    #[test]
    fn test_hd_wallet_creation() {
        let wallet = HDWallet::new(12).unwrap();
        assert_eq!(wallet.mnemonic_phrase().split_whitespace().count(), 12);
    }
    
    #[test]
    fn test_hd_wallet_derivation() {
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mut wallet = HDWallet::from_mnemonic_str(mnemonic, "").unwrap();
        
        let address = wallet.derive_account(0).unwrap();
        assert_ne!(address, Address::zero());
        
        // Known address for this mnemonic at m/44'/60'/0'/0/0
        let expected = "0x9858effd232b4033e47d90003d41ec34ecaeda94";
        let actual = format!("{:?}", address);
        assert_eq!(actual.to_lowercase(), expected);
    }
    
    #[test]
    fn test_multiple_account_derivation() {
        let mut wallet = HDWallet::new(12).unwrap();
        
        let addr1 = wallet.derive_account(0).unwrap();
        let addr2 = wallet.derive_account(1).unwrap();
        let addr3 = wallet.derive_account(2).unwrap();
        
        assert_ne!(addr1, addr2);
        assert_ne!(addr2, addr3);
        assert_ne!(addr1, addr3);
        
        assert_eq!(wallet.list_accounts().len(), 3);
    }
}