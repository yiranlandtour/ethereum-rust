use crate::{CryptoError, Result};
use ethereum_types::{Address, H256};
use secp256k1::{
    ecdsa::{RecoverableSignature, RecoveryId},
    Message, PublicKey, SecretKey, Secp256k1,
};

/// ECDSA signature with recovery ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub r: H256,
    pub s: H256,
    pub v: u8,
}

impl Signature {
    /// Create a new signature from r, s, and v components
    pub fn new(r: H256, s: H256, v: u8) -> Self {
        Signature { r, s, v }
    }
    
    /// Convert to compact representation (65 bytes: r || s || v)
    pub fn to_bytes(&self) -> [u8; 65] {
        let mut bytes = [0u8; 65];
        bytes[0..32].copy_from_slice(self.r.as_bytes());
        bytes[32..64].copy_from_slice(self.s.as_bytes());
        bytes[64] = self.v;
        bytes
    }
    
    /// Parse from compact representation
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 65 {
            return Err(CryptoError::InvalidSignature);
        }
        
        let r = H256::from_slice(&bytes[0..32]);
        let s = H256::from_slice(&bytes[32..64]);
        let v = bytes[64];
        
        Ok(Signature { r, s, v })
    }
}

/// Sign a message with a private key
pub fn sign_message(message: &H256, private_key: &SecretKey) -> Result<Signature> {
    let secp = Secp256k1::new();
    let message = Message::from_slice(message.as_bytes())?;
    let recoverable_sig = secp.sign_ecdsa_recoverable(&message, private_key);
    let (recovery_id, sig_bytes) = recoverable_sig.serialize_compact();
    
    let mut r = [0u8; 32];
    let mut s = [0u8; 32];
    r.copy_from_slice(&sig_bytes[0..32]);
    s.copy_from_slice(&sig_bytes[32..64]);
    
    let v = recovery_id.to_i32() as u8 + 27;
    
    Ok(Signature {
        r: H256::from(r),
        s: H256::from(s),
        v,
    })
}

/// Recover the public key from a signature
pub fn recover_public_key(message: &H256, signature: &Signature) -> Result<PublicKey> {
    let secp = Secp256k1::new();
    let message = Message::from_slice(message.as_bytes())?;
    
    let recovery_id = RecoveryId::from_i32((signature.v - 27) as i32)
        .map_err(|_| CryptoError::InvalidSignature)?;
    
    let mut sig_bytes = [0u8; 64];
    sig_bytes[0..32].copy_from_slice(signature.r.as_bytes());
    sig_bytes[32..64].copy_from_slice(signature.s.as_bytes());
    
    let recoverable_sig = RecoverableSignature::from_compact(&sig_bytes, recovery_id)?;
    let public_key = secp.recover_ecdsa(&message, &recoverable_sig)?;
    
    Ok(public_key)
}

/// Recover the Ethereum address from a signature
pub fn recover_address(message: &H256, signature: &Signature) -> Result<Address> {
    let public_key = recover_public_key(message, signature)?;
    Ok(public_key_to_address(&public_key))
}

/// Convert a public key to an Ethereum address
pub fn public_key_to_address(public_key: &PublicKey) -> Address {
    let public_key_bytes = public_key.serialize_uncompressed();
    // Skip the first byte (0x04) and hash the remaining 64 bytes
    let hash = crate::keccak256(&public_key_bytes[1..]);
    // Take the last 20 bytes of the hash
    Address::from_slice(&hash.as_bytes()[12..]).expect("Hash is always 32 bytes, so last 20 bytes are valid")
}

/// Generate a new random private key
pub fn generate_private_key() -> SecretKey {
    SecretKey::new(&mut rand::thread_rng())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hex;
    
    #[test]
    fn test_sign_and_recover() {
        let secp = Secp256k1::new();
        let private_key = SecretKey::from_slice(&[0x01; 32]).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        let message = H256::from_slice(&[0x02; 32]);
        
        let signature = sign_message(&message, &private_key).unwrap();
        let recovered_public_key = recover_public_key(&message, &signature).unwrap();
        
        assert_eq!(public_key, recovered_public_key);
    }
    
    #[test]
    fn test_public_key_to_address() {
        let secp = Secp256k1::new();
        // Test vector from Ethereum yellow paper
        let private_key_hex = "c85ef7d79691fe79573b1a7064c19c1a9819ebdbd1faaab1a8ec92344438aaf4";
        let expected_address = "cd2a3d9f938e13cd947ec05abc7fe734df8dd826";
        
        let private_key = SecretKey::from_slice(&hex::decode(private_key_hex).unwrap()).unwrap();
        let public_key = PublicKey::from_secret_key(&secp, &private_key);
        let address = public_key_to_address(&public_key);
        
        assert_eq!(format!("{:x}", address), format!("0x{}", expected_address));
    }
    
    #[test]
    fn test_signature_serialization() {
        let r = H256::from_slice(&[0x01; 32]);
        let s = H256::from_slice(&[0x02; 32]);
        let v = 27;
        
        let sig = Signature::new(r, s, v);
        let bytes = sig.to_bytes();
        let recovered_sig = Signature::from_bytes(&bytes).unwrap();
        
        assert_eq!(sig, recovered_sig);
    }
}