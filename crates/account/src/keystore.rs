use std::path::{Path, PathBuf};
use std::fs;
use std::collections::HashMap;
use ethereum_types::Address;
use secp256k1::SecretKey;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use aes::cipher::{KeyIvInit, StreamCipher};
use scrypt::{scrypt, Params as ScryptParams};
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;
use rand::Rng;

use crate::{Account, AccountError, Result};

/// Keystore for managing encrypted keys
pub struct KeyStore {
    keystore_dir: PathBuf,
    accounts: HashMap<Address, PathBuf>,
}

impl KeyStore {
    /// Create a new keystore
    pub fn new<P: AsRef<Path>>(keystore_dir: P) -> Result<Self> {
        let keystore_dir = keystore_dir.as_ref().to_path_buf();
        
        // Create directory if it doesn't exist
        fs::create_dir_all(&keystore_dir)?;
        
        let mut keystore = Self {
            keystore_dir,
            accounts: HashMap::new(),
        };
        
        // Load existing accounts
        keystore.load_accounts()?;
        
        Ok(keystore)
    }
    
    /// Load accounts from keystore directory
    fn load_accounts(&mut self) -> Result<()> {
        let entries = fs::read_dir(&self.keystore_dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("json") {
                // Try to load keyfile to get address
                if let Ok(keyfile) = self.load_keyfile(&path) {
                    if let Ok(address_bytes) = hex::decode(&keyfile.address) {
                        if address_bytes.len() == 20 {
                            let address = Address::from_slice(&address_bytes);
                            self.accounts.insert(address, path);
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Store account in keystore
    pub async fn store_account(&mut self, account: &Account, password: &str) -> Result<()> {
        let keyfile = KeyFile::encrypt(account, password)?;
        let address = account.address();
        
        // Generate filename
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S%.3fZ");
        let filename = format!("UTC--{}--{}", timestamp, hex::encode(address.as_bytes()));
        let filepath = self.keystore_dir.join(filename);
        
        // Write keyfile
        let json = serde_json::to_string_pretty(&keyfile)?;
        fs::write(&filepath, json)?;
        
        // Update accounts map
        self.accounts.insert(address, filepath);
        
        Ok(())
    }
    
    /// Unlock account from keystore
    pub async fn unlock_account(&self, address: Address, password: &str) -> Result<Account> {
        let filepath = self.accounts.get(&address)
            .ok_or(AccountError::AccountNotFound)?;
        
        self.load_from_file(filepath, password).await
    }
    
    /// Load account from keyfile
    pub async fn load_from_file(&self, path: &Path, password: &str) -> Result<Account> {
        let keyfile = self.load_keyfile(path)?;
        keyfile.decrypt(password)
    }
    
    /// Load keyfile from path
    fn load_keyfile(&self, path: &Path) -> Result<KeyFile> {
        let content = fs::read_to_string(path)?;
        let keyfile: KeyFile = serde_json::from_str(&content)?;
        Ok(keyfile)
    }
    
    /// Check if account exists
    pub fn has_account(&self, address: Address) -> bool {
        self.accounts.contains_key(&address)
    }
    
    /// List all accounts
    pub fn list_accounts(&self) -> Vec<Address> {
        self.accounts.keys().copied().collect()
    }
    
    /// Export account to keyfile
    pub async fn export_account(
        &self,
        address: Address,
        password: &str,
        output_path: &Path,
    ) -> Result<()> {
        let filepath = self.accounts.get(&address)
            .ok_or(AccountError::AccountNotFound)?;
        
        // Load and decrypt account
        let account = self.load_from_file(filepath, password).await?;
        
        // Re-encrypt with possibly new password
        let keyfile = KeyFile::encrypt(&account, password)?;
        
        // Write to output path
        let json = serde_json::to_string_pretty(&keyfile)?;
        fs::write(output_path, json)?;
        
        Ok(())
    }
    
    /// Remove account from keystore
    pub fn remove_account(&mut self, address: Address) -> Result<()> {
        if let Some(filepath) = self.accounts.remove(&address) {
            fs::remove_file(filepath)?;
            Ok(())
        } else {
            Err(AccountError::AccountNotFound)
        }
    }
}

/// Keyfile format (Web3 Secret Storage Definition)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFile {
    pub id: String,
    pub version: u32,
    pub address: String,
    pub crypto: CryptoParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CryptoParams {
    pub cipher: String,
    pub cipherparams: CipherParams,
    pub ciphertext: String,
    pub kdf: String,
    pub kdfparams: KdfParams,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CipherParams {
    pub iv: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KdfParams {
    Scrypt {
        dklen: u32,
        n: u32,
        p: u32,
        r: u32,
        salt: String,
    },
    Pbkdf2 {
        c: u32,
        dklen: u32,
        prf: String,
        salt: String,
    },
}

impl KeyFile {
    /// Encrypt account to keyfile
    pub fn encrypt(account: &Account, password: &str) -> Result<Self> {
        let mut rng = rand::thread_rng();
        
        // Generate random salt and IV
        let mut salt = [0u8; 32];
        let mut iv = [0u8; 16];
        rng.fill(&mut salt);
        rng.fill(&mut iv);
        
        // Derive key using scrypt
        let mut derived_key = [0u8; 32];
        let params = ScryptParams::new(14, 8, 1, 32)
            .map_err(|e| AccountError::KeystoreError(e.to_string()))?;
        
        scrypt(
            password.as_bytes(),
            &salt,
            &params,
            &mut derived_key,
        ).map_err(|e| AccountError::KeystoreError(e.to_string()))?;
        
        // Encrypt private key
        let private_key = account.private_key().secret_bytes();
        let mut ciphertext = private_key.to_vec();
        
        type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;
        let mut cipher = Aes128Ctr::new((&derived_key[..16]).into(), (&iv[..]).into());
        cipher.apply_keystream(&mut ciphertext);
        
        // Calculate MAC
        let mut mac_data = Vec::new();
        mac_data.extend_from_slice(&derived_key[16..32]);
        mac_data.extend_from_slice(&ciphertext);
        let mac = ethereum_crypto::keccak256(&mac_data);
        
        Ok(KeyFile {
            id: Uuid::new_v4().to_string(),
            version: 3,
            address: hex::encode(account.address().as_bytes()),
            crypto: CryptoParams {
                cipher: "aes-128-ctr".to_string(),
                cipherparams: CipherParams {
                    iv: hex::encode(iv),
                },
                ciphertext: hex::encode(ciphertext),
                kdf: "scrypt".to_string(),
                kdfparams: KdfParams::Scrypt {
                    dklen: 32,
                    n: 8192,
                    p: 1,
                    r: 8,
                    salt: hex::encode(salt),
                },
                mac: hex::encode(mac),
            },
        })
    }
    
    /// Decrypt keyfile to account
    pub fn decrypt(&self, password: &str) -> Result<Account> {
        if self.version != 3 {
            return Err(AccountError::InvalidKeyFile);
        }
        
        // Derive key
        let derived_key = match &self.crypto.kdfparams {
            KdfParams::Scrypt { dklen, n, p, r, salt } => {
                let salt = hex::decode(salt)
                    .map_err(|_| AccountError::InvalidKeyFile)?;
                
                let mut derived_key = vec![0u8; *dklen as usize];
                let params = ScryptParams::new((*n as f64).log2() as u8, *r, *p, *dklen as usize)
                    .map_err(|e| AccountError::KeystoreError(e.to_string()))?;
                
                scrypt(
                    password.as_bytes(),
                    &salt,
                    &params,
                    &mut derived_key,
                ).map_err(|e| AccountError::KeystoreError(e.to_string()))?;
                
                derived_key
            }
            KdfParams::Pbkdf2 { c, dklen, prf: _, salt } => {
                let salt = hex::decode(salt)
                    .map_err(|_| AccountError::InvalidKeyFile)?;
                
                let mut derived_key = vec![0u8; *dklen as usize];
                pbkdf2_hmac::<Sha256>(
                    password.as_bytes(),
                    &salt,
                    *c,
                    &mut derived_key,
                );
                
                derived_key
            }
        };
        
        // Verify MAC
        let ciphertext = hex::decode(&self.crypto.ciphertext)
            .map_err(|_| AccountError::InvalidKeyFile)?;
        
        let mut mac_data = Vec::new();
        mac_data.extend_from_slice(&derived_key[16..32]);
        mac_data.extend_from_slice(&ciphertext);
        let mac = ethereum_crypto::keccak256(&mac_data);
        
        let expected_mac = hex::decode(&self.crypto.mac)
            .map_err(|_| AccountError::InvalidKeyFile)?;
        
        if mac != expected_mac.as_slice() {
            return Err(AccountError::InvalidPassword);
        }
        
        // Decrypt private key
        let iv = hex::decode(&self.crypto.cipherparams.iv)
            .map_err(|_| AccountError::InvalidKeyFile)?;
        
        let mut private_key = ciphertext;
        
        type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;
        let mut cipher = Aes128Ctr::new(
            (&derived_key[..16]).try_into()
                .map_err(|_| AccountError::InvalidKeyFile)?,
            (&iv[..]).try_into()
                .map_err(|_| AccountError::InvalidKeyFile)?,
        );
        cipher.apply_keystream(&mut private_key);
        
        // Create account from private key
        let secret_key = SecretKey::from_slice(&private_key)?;
        Account::from_private_key(secret_key)
    }
}

// Add chrono dependency for timestamp
use chrono;