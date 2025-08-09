use aes::cipher::{KeyIvInit, StreamCipher};
use ctr::Ctr128BE;
use aes::Aes256;
use hmac::{Hmac, Mac};
use sha2::{Sha256, Digest};
use secp256k1::{PublicKey, SecretKey, Secp256k1, ecdsa::RecoverableSignature};
use ethereum_types::H256;
// use ethereum_rlp::{Encode, Decode, Encoder, Decoder}; // Unused imports
use rand::Rng;
// use bytes::{Bytes, BytesMut, BufMut}; // Unused imports
// use std::io; // Unused import

use crate::{Result, NetworkError};

type Aes256Ctr = Ctr128BE<Aes256>;

const MAC_SIZE: usize = 16;
const PROTOCOL_VERSION: u8 = 5;

pub struct RLPxHandshake {
    pub static_key: SecretKey,
    pub ephemeral_key: SecretKey,
    pub nonce: H256,
    pub remote_id: Option<PublicKey>,
    pub remote_ephemeral: Option<PublicKey>,
    pub remote_nonce: Option<H256>,
    pub auth_sent: Option<Vec<u8>>,
    pub ack_sent: Option<Vec<u8>>,
    pub auth_received: Option<Vec<u8>>,
    pub ack_received: Option<Vec<u8>>,
}

impl RLPxHandshake {
    pub fn new(static_key: SecretKey, remote_id: Option<PublicKey>) -> Self {
        let secp = Secp256k1::new();
        let mut rng = rand::thread_rng();
        
        let ephemeral_key = SecretKey::new(&mut rng);
        let nonce: [u8; 32] = rng.gen();
        
        Self {
            static_key,
            ephemeral_key,
            nonce: H256::from(nonce),
            remote_id,
            remote_ephemeral: None,
            remote_nonce: None,
            auth_sent: None,
            ack_sent: None,
            auth_received: None,
            ack_received: None,
        }
    }
    
    pub fn create_auth_message(&mut self) -> Result<Vec<u8>> {
        let remote_id = self.remote_id
            .ok_or_else(|| NetworkError::HandshakeFailed("Remote ID not set".to_string()))?;
        
        let secp = Secp256k1::new();
        let ephemeral_pubkey = PublicKey::from_secret_key(&secp, &self.ephemeral_key);
        
        // Create signature
        let shared_secret = compute_shared_secret(&self.static_key, &remote_id)?;
        let msg_hash = H256::from_slice(&shared_secret[..32]);
        
        let sig = secp.sign_ecdsa_recoverable(
            &secp256k1::Message::from_slice(&msg_hash[..])
                .map_err(|e| NetworkError::CryptoError(e.to_string()))?,
            &self.ephemeral_key,
        );
        
        let (recovery_id, sig_bytes) = sig.serialize_compact();
        
        // Build auth message
        let mut auth = Vec::new();
        auth.extend_from_slice(&sig_bytes);
        auth.push(recovery_id.to_i32() as u8);
        auth.extend_from_slice(&PublicKey::from_secret_key(&secp, &self.static_key).serialize_uncompressed()[1..65]);
        auth.extend_from_slice(&self.nonce[..]);
        auth.push(PROTOCOL_VERSION);
        
        // Encrypt with ECIES
        let encrypted = ecies_encrypt(&remote_id, &auth)?;
        self.auth_sent = Some(encrypted.clone());
        
        Ok(encrypted)
    }
    
    pub fn handle_auth_message(&mut self, data: &[u8]) -> Result<()> {
        self.auth_received = Some(data.to_vec());
        
        // Decrypt with ECIES
        let decrypted = ecies_decrypt(&self.static_key, data)?;
        
        if decrypted.len() < 65 + 32 + 32 + 1 {
            return Err(NetworkError::HandshakeFailed("Auth message too short".to_string()));
        }
        
        // Parse auth message
        let sig = &decrypted[0..65];
        let pubkey_bytes = &decrypted[65..65+64];
        let nonce = &decrypted[65+64..65+64+32];
        let version = decrypted[65+64+32];
        
        if version != PROTOCOL_VERSION {
            return Err(NetworkError::HandshakeFailed(
                format!("Unsupported protocol version: {}", version)
            ));
        }
        
        // Recover ephemeral public key from signature
        let secp = Secp256k1::new();
        let recovery_id = secp256k1::ecdsa::RecoveryId::from_i32(sig[64] as i32)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        let _signature = RecoverableSignature::from_compact(&sig[0..64], recovery_id)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        
        // Extract remote static public key
        let mut uncompressed = vec![0x04];
        uncompressed.extend_from_slice(pubkey_bytes);
        self.remote_id = Some(PublicKey::from_slice(&uncompressed)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?);
        
        self.remote_nonce = Some(H256::from_slice(nonce));
        
        Ok(())
    }
    
    pub fn create_ack_message(&mut self) -> Result<Vec<u8>> {
        let secp = Secp256k1::new();
        let ephemeral_pubkey = PublicKey::from_secret_key(&secp, &self.ephemeral_key);
        
        // Build ack message
        let mut ack = Vec::new();
        ack.extend_from_slice(&ephemeral_pubkey.serialize_uncompressed()[1..65]);
        ack.extend_from_slice(&self.nonce[..]);
        ack.push(PROTOCOL_VERSION);
        
        // Encrypt with ECIES
        let remote_id = self.remote_id
            .ok_or_else(|| NetworkError::HandshakeFailed("Remote ID not set".to_string()))?;
        let encrypted = ecies_encrypt(&remote_id, &ack)?;
        self.ack_sent = Some(encrypted.clone());
        
        Ok(encrypted)
    }
    
    pub fn handle_ack_message(&mut self, data: &[u8]) -> Result<()> {
        self.ack_received = Some(data.to_vec());
        
        // Decrypt with ECIES
        let decrypted = ecies_decrypt(&self.static_key, data)?;
        
        if decrypted.len() < 64 + 32 + 1 {
            return Err(NetworkError::HandshakeFailed("Ack message too short".to_string()));
        }
        
        // Parse ack message
        let ephemeral_bytes = &decrypted[0..64];
        let nonce = &decrypted[64..64+32];
        let version = decrypted[64+32];
        
        if version != PROTOCOL_VERSION {
            return Err(NetworkError::HandshakeFailed(
                format!("Unsupported protocol version: {}", version)
            ));
        }
        
        // Extract remote ephemeral public key
        let mut uncompressed = vec![0x04];
        uncompressed.extend_from_slice(ephemeral_bytes);
        self.remote_ephemeral = Some(PublicKey::from_slice(&uncompressed)
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?);
        
        self.remote_nonce = Some(H256::from_slice(nonce));
        
        Ok(())
    }
    
    pub fn derive_secrets(&self) -> Result<Secrets> {
        let remote_ephemeral = self.remote_ephemeral
            .ok_or_else(|| NetworkError::HandshakeFailed("Remote ephemeral not set".to_string()))?;
        let remote_nonce = self.remote_nonce
            .ok_or_else(|| NetworkError::HandshakeFailed("Remote nonce not set".to_string()))?;
        
        // Compute shared secret
        let ephemeral_shared = compute_shared_secret(&self.ephemeral_key, &remote_ephemeral)?;
        
        // Derive encryption and MAC keys
        let mut hasher = Sha256::new();
        hasher.update(&ephemeral_shared);
        hasher.update(&self.nonce[..]);
        hasher.update(&remote_nonce[..]);
        let shared = hasher.finalize();
        
        // KDF
        let aes_secret = H256::from_slice(&shared[0..32]);
        let mac_secret = H256::from_slice(&keccak256(&shared));
        
        // Setup initial MACs
        let mut egress_mac = Hmac::<Sha256>::new_from_slice(&mac_secret[..])
            .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
        let mut ingress_mac = egress_mac.clone();
        
        // Update MACs with handshake data
        if let (Some(auth_sent), Some(ack_received)) = (&self.auth_sent, &self.ack_received) {
            egress_mac.update(&xor_bytes(&mac_secret[..], &self.nonce[..]));
            egress_mac.update(auth_sent);
            ingress_mac.update(&xor_bytes(&mac_secret[..], &remote_nonce[..]));
            ingress_mac.update(ack_received);
        } else if let (Some(auth_received), Some(ack_sent)) = (&self.auth_received, &self.ack_sent) {
            ingress_mac.update(&xor_bytes(&mac_secret[..], &remote_nonce[..]));
            ingress_mac.update(auth_received);
            egress_mac.update(&xor_bytes(&mac_secret[..], &self.nonce[..]));
            egress_mac.update(ack_sent);
        }
        
        Ok(Secrets {
            aes_secret,
            mac_secret,
            egress_mac,
            ingress_mac,
        })
    }
}

pub struct Secrets {
    pub aes_secret: H256,
    pub mac_secret: H256,
    pub egress_mac: Hmac<Sha256>,
    pub ingress_mac: Hmac<Sha256>,
}

pub struct RLPxSession {
    secrets: Secrets,
    ingress_aes: Aes256Ctr,
    egress_aes: Aes256Ctr,
}

impl RLPxSession {
    pub fn new(secrets: Secrets) -> Self {
        let iv = [0u8; 16];
        let ingress_aes = Aes256Ctr::new((&secrets.aes_secret[..]).into(), (&iv[..]).into());
        let egress_aes = Aes256Ctr::new((&secrets.aes_secret[..]).into(), (&iv[..]).into());
        
        Self {
            secrets,
            ingress_aes,
            egress_aes,
        }
    }
    
    pub fn write_frame(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        let mut frame = Vec::new();
        
        // Frame header
        let frame_size = data.len();
        let mut header = [0u8; 16];
        header[0..3].copy_from_slice(&(frame_size as u32).to_be_bytes()[1..4]);
        
        // Encrypt header
        self.egress_aes.apply_keystream(&mut header);
        
        // Update MAC with encrypted header
        self.secrets.egress_mac.update(&header);
        let header_mac = self.secrets.egress_mac.clone().finalize();
        
        frame.extend_from_slice(&header);
        frame.extend_from_slice(&header_mac.into_bytes()[0..MAC_SIZE]);
        
        // Encrypt frame data
        let mut encrypted_data = data.to_vec();
        self.egress_aes.apply_keystream(&mut encrypted_data);
        
        // Update MAC with encrypted data
        self.secrets.egress_mac.update(&encrypted_data);
        let frame_mac = self.secrets.egress_mac.clone().finalize();
        
        frame.extend_from_slice(&encrypted_data);
        frame.extend_from_slice(&frame_mac.into_bytes()[0..MAC_SIZE]);
        
        Ok(frame)
    }
    
    pub fn read_frame(&mut self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 32 {
            return Err(NetworkError::InvalidMessage("Frame too short".to_string()));
        }
        
        // Read and verify header
        let mut header = data[0..16].to_vec();
        let header_mac = &data[16..32];
        
        // Verify header MAC
        self.secrets.ingress_mac.update(&header);
        let computed_mac = self.secrets.ingress_mac.clone().finalize();
        if &computed_mac.into_bytes()[0..MAC_SIZE] != header_mac {
            return Err(NetworkError::InvalidMessage("Invalid header MAC".to_string()));
        }
        
        // Decrypt header
        self.ingress_aes.apply_keystream(&mut header);
        
        // Parse frame size
        let frame_size = u32::from_be_bytes([0, header[0], header[1], header[2]]) as usize;
        
        if data.len() < 32 + frame_size + MAC_SIZE {
            return Err(NetworkError::InvalidMessage("Incomplete frame".to_string()));
        }
        
        // Read and verify frame data
        let mut frame_data = data[32..32+frame_size].to_vec();
        let frame_mac = &data[32+frame_size..32+frame_size+MAC_SIZE];
        
        // Verify frame MAC
        self.secrets.ingress_mac.update(&frame_data);
        let computed_mac = self.secrets.ingress_mac.clone().finalize();
        if &computed_mac.into_bytes()[0..MAC_SIZE] != frame_mac {
            return Err(NetworkError::InvalidMessage("Invalid frame MAC".to_string()));
        }
        
        // Decrypt frame data
        self.ingress_aes.apply_keystream(&mut frame_data);
        
        Ok(frame_data)
    }
}

fn compute_shared_secret(private_key: &SecretKey, public_key: &PublicKey) -> Result<[u8; 32]> {
    let secp = Secp256k1::new();
    let shared_point = public_key.mul_tweak(&secp, &(*private_key).into())
        .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
    
    let serialized = shared_point.serialize_uncompressed();
    let mut hasher = Sha256::new();
    hasher.update(&serialized[1..33]); // Use x-coordinate only
    Ok(hasher.finalize().into())
}

fn ecies_encrypt(public_key: &PublicKey, data: &[u8]) -> Result<Vec<u8>> {
    // Simplified ECIES encryption (full implementation would include more steps)
    let mut rng = rand::thread_rng();
    let secp = Secp256k1::new();
    
    let ephemeral_key = SecretKey::new(&mut rng);
    let ephemeral_pubkey = PublicKey::from_secret_key(&secp, &ephemeral_key);
    
    let shared_secret = compute_shared_secret(&ephemeral_key, public_key)?;
    
    // Derive encryption key
    let mut hasher = Sha256::new();
    hasher.update(&shared_secret);
    let key = hasher.finalize();
    
    // Encrypt data with AES-CTR
    let iv = [0u8; 16];
    let mut cipher = Aes256Ctr::new((&key[..]).into(), (&iv[..]).into());
    let mut encrypted = data.to_vec();
    cipher.apply_keystream(&mut encrypted);
    
    // Build output
    let mut output = Vec::new();
    output.extend_from_slice(&ephemeral_pubkey.serialize_uncompressed()[1..65]);
    output.extend_from_slice(&encrypted);
    
    Ok(output)
}

fn ecies_decrypt(private_key: &SecretKey, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 64 {
        return Err(NetworkError::CryptoError("Invalid ECIES data".to_string()));
    }
    
    // Extract ephemeral public key
    let mut pubkey_bytes = vec![0x04];
    pubkey_bytes.extend_from_slice(&data[0..64]);
    let ephemeral_pubkey = PublicKey::from_slice(&pubkey_bytes)
        .map_err(|e| NetworkError::CryptoError(e.to_string()))?;
    
    let shared_secret = compute_shared_secret(private_key, &ephemeral_pubkey)?;
    
    // Derive decryption key
    let mut hasher = Sha256::new();
    hasher.update(&shared_secret);
    let key = hasher.finalize();
    
    // Decrypt data with AES-CTR
    let iv = [0u8; 16];
    let mut cipher = Aes256Ctr::new((&key[..]).into(), (&iv[..]).into());
    let mut decrypted = data[64..].to_vec();
    cipher.apply_keystream(&mut decrypted);
    
    Ok(decrypted)
}

fn xor_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    ethereum_crypto::keccak256(data).into()
}