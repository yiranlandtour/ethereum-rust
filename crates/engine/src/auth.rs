use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use rand::Rng;
use base64::{Engine as _, engine::general_purpose};

use crate::{EngineError, Result};

const JWT_ALGORITHM: Algorithm = Algorithm::HS256;
const JWT_VERSION: &str = "0x00";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iat: u64,
    exp: Option<u64>,
}

#[derive(Clone)]
pub struct JwtSecret {
    secret: Vec<u8>,
}

impl JwtSecret {
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut secret = vec![0u8; 32];
        rng.fill(&mut secret[..]);
        Self { secret }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self> {
        if bytes.len() != 32 {
            return Err(EngineError::InvalidJwt);
        }
        Ok(Self { secret: bytes })
    }

    pub fn from_hex(hex: &str) -> Result<Self> {
        let hex = hex.trim_start_matches("0x");
        let bytes = hex::decode(hex)
            .map_err(|_| EngineError::InvalidJwt)?;
        Self::from_bytes(bytes)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .map_err(|e| EngineError::Internal(format!("Failed to read JWT secret: {}", e)))?;
        let hex = contents.trim();
        Self::from_hex(hex)
    }

    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let hex = format!("0x{}", hex::encode(&self.secret));
        fs::write(path, hex)
            .map_err(|e| EngineError::Internal(format!("Failed to save JWT secret: {}", e)))?;
        Ok(())
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.secret
    }

    pub fn to_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.secret))
    }
}

pub struct JwtAuth {
    secret: JwtSecret,
    validation: Validation,
}

impl JwtAuth {
    pub fn new(secret: JwtSecret) -> Self {
        let mut validation = Validation::new(JWT_ALGORITHM);
        validation.validate_exp = false;
        validation.required_spec_claims.clear();
        
        Self {
            secret,
            validation,
        }
    }

    pub fn create_token(&self) -> Result<String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| EngineError::Internal(format!("System time error: {}", e)))?
            .as_secs();

        let claims = Claims {
            iat: now,
            exp: None,
        };

        let token = encode(
            &Header::new(JWT_ALGORITHM),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| EngineError::Internal(format!("Failed to create JWT: {}", e)))?;

        Ok(token)
    }

    pub fn validate_token(&self, token: &str) -> Result<()> {
        let token = token.trim_start_matches("Bearer ").trim();
        
        decode::<Claims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &self.validation,
        )
        .map_err(|_| EngineError::Unauthorized)?;

        Ok(())
    }

    pub fn extract_bearer_token(auth_header: Option<&str>) -> Result<String> {
        let header = auth_header.ok_or(EngineError::Unauthorized)?;
        
        if !header.starts_with("Bearer ") {
            return Err(EngineError::Unauthorized);
        }
        
        Ok(header[7..].to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_jwt_secret_creation() {
        let secret = JwtSecret::new();
        assert_eq!(secret.as_bytes().len(), 32);
    }

    #[test]
    fn test_jwt_secret_from_hex() {
        let hex = "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
        let secret = JwtSecret::from_hex(hex).unwrap();
        assert_eq!(secret.to_hex(), hex);
    }

    #[test]
    fn test_jwt_secret_file_io() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("jwt.hex");
        
        let secret = JwtSecret::new();
        secret.save_to_file(&path).unwrap();
        
        let loaded = JwtSecret::from_file(&path).unwrap();
        assert_eq!(secret.as_bytes(), loaded.as_bytes());
    }

    #[test]
    fn test_jwt_auth() {
        let secret = JwtSecret::new();
        let auth = JwtAuth::new(secret);
        
        let token = auth.create_token().unwrap();
        assert!(auth.validate_token(&token).is_ok());
        
        let with_bearer = format!("Bearer {}", token);
        assert!(auth.validate_token(&with_bearer).is_ok());
    }

    #[test]
    fn test_invalid_token() {
        let secret = JwtSecret::new();
        let auth = JwtAuth::new(secret);
        
        assert!(auth.validate_token("invalid_token").is_err());
        
        let other_secret = JwtSecret::new();
        let other_auth = JwtAuth::new(other_secret);
        let other_token = other_auth.create_token().unwrap();
        
        assert!(auth.validate_token(&other_token).is_err());
    }
}