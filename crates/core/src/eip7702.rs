use ethereum_types::{Address, H256, U256};
use ethereum_crypto::{keccak256, recover_address, Signature};
use ethereum_rlp::{Decode, Encode};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Eip7702Error {
    #[error("Invalid authorization signature")]
    InvalidSignature,
    
    #[error("Invalid chain ID")]
    InvalidChainId,
    
    #[error("Nonce mismatch")]
    NonceMismatch,
    
    #[error("Authorization expired")]
    Expired,
    
    #[error("Invalid authority")]
    InvalidAuthority,
}

pub type Result<T> = std::result::Result<T, Eip7702Error>;

/// EIP-7702 Authorization tuple
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Authorization {
    pub chain_id: u64,
    pub address: Address,
    pub nonce: U256,
    pub y_parity: bool,
    pub r: U256,
    pub s: U256,
}

impl Authorization {
    pub fn new(
        chain_id: u64,
        address: Address,
        nonce: U256,
    ) -> Self {
        Self {
            chain_id,
            address,
            nonce,
            y_parity: false,
            r: U256::zero(),
            s: U256::zero(),
        }
    }

    pub fn sign(&mut self, private_key: &[u8; 32]) -> Result<()> {
        let message = self.signing_hash();
        
        let signature = ethereum_crypto::sign_message(&message, private_key)
            .map_err(|_| Eip7702Error::InvalidSignature)?;
        
        self.y_parity = signature.v == 1;
        self.r = U256::from_big_endian(&signature.r);
        self.s = U256::from_big_endian(&signature.s);
        
        Ok(())
    }

    pub fn verify(&self) -> Result<Address> {
        let message = self.signing_hash();
        
        let recovery_id = if self.y_parity { 1 } else { 0 };
        
        let mut r_bytes = [0u8; 32];
        let mut s_bytes = [0u8; 32];
        self.r.to_big_endian(&mut r_bytes);
        self.s.to_big_endian(&mut s_bytes);
        
        let authority = recover_address(&message, recovery_id, &r_bytes, &s_bytes)
            .map_err(|_| Eip7702Error::InvalidSignature)?;
        
        Ok(authority)
    }

    pub fn signing_hash(&self) -> H256 {
        let mut encoder = ethereum_rlp::Encoder::new();
        encoder.encode(&self.chain_id);
        encoder.encode(&self.address);
        encoder.encode(&self.nonce);
        
        keccak256(&[&[0x05], &encoder.finish()].concat())
    }

    pub fn is_valid_for_chain(&self, chain_id: u64) -> bool {
        self.chain_id == chain_id
    }
}

impl Encode for Authorization {
    fn encode(&self, encoder: &mut ethereum_rlp::Encoder) {
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.address),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.y_parity),
            ethereum_rlp::encode(&self.r),
            ethereum_rlp::encode(&self.s),
        ]);
    }
}

impl Decode for Authorization {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        Ok(Self {
            chain_id: u64::decode(decoder)?,
            address: Address::decode(decoder)?,
            nonce: U256::decode(decoder)?,
            y_parity: bool::decode(decoder)?,
            r: U256::decode(decoder)?,
            s: U256::decode(decoder)?,
        })
    }
}

/// EIP-7702 Transaction type (0x04)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip7702Transaction {
    pub chain_id: u64,
    pub nonce: U256,
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub to: Address,
    pub value: U256,
    pub data: ethereum_types::Bytes,
    pub access_list: Vec<super::transaction::AccessListItem>,
    pub authorization_list: Vec<Authorization>,
    pub y_parity: bool,
    pub r: U256,
    pub s: U256,
}

impl Eip7702Transaction {
    pub fn hash(&self) -> H256 {
        keccak256(&[&[0x04], &ethereum_rlp::encode(self)[..]].concat())
    }

    pub fn signing_hash(&self) -> H256 {
        let mut encoder = ethereum_rlp::Encoder::new();
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.max_priority_fee_per_gas),
            ethereum_rlp::encode(&self.max_fee_per_gas),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            super::transaction::encode_access_list(&self.access_list),
            self.encode_authorization_list(),
        ]);
        
        keccak256(&[&[0x04], &encoder.finish()[..]].concat())
    }

    fn encode_authorization_list(&self) -> ethereum_types::Bytes {
        let mut encoder = ethereum_rlp::Encoder::new();
        encoder.encode_list(&self.authorization_list);
        ethereum_types::Bytes::from_vec(encoder.finish())
    }

    pub fn sender(&self) -> Result<Address> {
        let message = self.signing_hash();
        let recovery_id = if self.y_parity { 1 } else { 0 };
        
        let mut r_bytes = [0u8; 32];
        let mut s_bytes = [0u8; 32];
        self.r.to_big_endian(&mut r_bytes);
        self.s.to_big_endian(&mut s_bytes);
        
        recover_address(&message, recovery_id, &r_bytes, &s_bytes)
            .map_err(|_| Eip7702Error::InvalidSignature)
    }

    pub fn validate_authorizations(&self, chain_id: u64) -> Result<Vec<(Address, Address)>> {
        let mut delegations = Vec::new();
        
        for auth in &self.authorization_list {
            if !auth.is_valid_for_chain(chain_id) {
                return Err(Eip7702Error::InvalidChainId);
            }
            
            let authority = auth.verify()?;
            delegations.push((authority, auth.address));
        }
        
        Ok(delegations)
    }
}

impl Encode for Eip7702Transaction {
    fn encode(&self, encoder: &mut ethereum_rlp::Encoder) {
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.max_priority_fee_per_gas),
            ethereum_rlp::encode(&self.max_fee_per_gas),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            super::transaction::encode_access_list(&self.access_list),
            self.encode_authorization_list(),
            ethereum_rlp::encode(&self.y_parity),
            ethereum_rlp::encode(&self.r),
            ethereum_rlp::encode(&self.s),
        ]);
    }
}

impl Decode for Eip7702Transaction {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        Ok(Self {
            chain_id: u64::decode(decoder)?,
            nonce: U256::decode(decoder)?,
            max_priority_fee_per_gas: U256::decode(decoder)?,
            max_fee_per_gas: U256::decode(decoder)?,
            gas_limit: U256::decode(decoder)?,
            to: Address::decode(decoder)?,
            value: U256::decode(decoder)?,
            data: ethereum_types::Bytes::decode(decoder)?,
            access_list: decoder.decode_list()?,
            authorization_list: decoder.decode_list()?,
            y_parity: bool::decode(decoder)?,
            r: U256::decode(decoder)?,
            s: U256::decode(decoder)?,
        })
    }
}

/// Account delegation state for EIP-7702
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegatedAccount {
    pub authority: Address,
    pub implementation: Address,
    pub nonce: U256,
}

impl DelegatedAccount {
    pub fn new(authority: Address, implementation: Address, nonce: U256) -> Self {
        Self {
            authority,
            implementation,
            nonce,
        }
    }

    pub fn is_active(&self) -> bool {
        self.implementation != Address::zero()
    }

    pub fn revoke(&mut self) {
        self.implementation = Address::zero();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_authorization_signing() {
        let mut auth = Authorization::new(
            1,
            Address::from([1u8; 20]),
            U256::from(0),
        );
        
        let private_key = [0x42u8; 32];
        auth.sign(&private_key).unwrap();
        
        assert!(auth.r != U256::zero());
        assert!(auth.s != U256::zero());
    }

    #[test]
    fn test_authorization_verification() {
        let mut auth = Authorization::new(
            1,
            Address::from([1u8; 20]),
            U256::from(0),
        );
        
        let private_key = [0x42u8; 32];
        auth.sign(&private_key).unwrap();
        
        let authority = auth.verify().unwrap();
        assert!(authority != Address::zero());
    }

    #[test]
    fn test_eip7702_transaction_hash() {
        let tx = Eip7702Transaction {
            chain_id: 1,
            nonce: U256::from(0),
            max_priority_fee_per_gas: U256::from(1_000_000_000),
            max_fee_per_gas: U256::from(20_000_000_000u64),
            gas_limit: U256::from(21_000),
            to: Address::from([1u8; 20]),
            value: U256::from(1_000_000_000_000_000_000u64),
            data: ethereum_types::Bytes::from(vec![]),
            access_list: vec![],
            authorization_list: vec![],
            y_parity: false,
            r: U256::zero(),
            s: U256::zero(),
        };
        
        let hash = tx.hash();
        assert!(hash != H256::zero());
    }

    #[test]
    fn test_delegated_account() {
        let mut account = DelegatedAccount::new(
            Address::from([1u8; 20]),
            Address::from([2u8; 20]),
            U256::from(0),
        );
        
        assert!(account.is_active());
        
        account.revoke();
        assert!(!account.is_active());
    }
}