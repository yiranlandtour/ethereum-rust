use ethereum_crypto::{keccak256, recover_address, Signature};
use ethereum_rlp::{Decode, Encode, Encoder};
use ethereum_types::{Address, Bytes, H256, U256};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Invalid transaction type: {0}")]
    InvalidTransactionType(u8),
    #[error("Missing required field: {0}")]
    MissingField(&'static str),
    #[error("Invalid chain ID")]
    InvalidChainId,
    #[error("Crypto error: {0}")]
    Crypto(#[from] ethereum_crypto::CryptoError),
    #[error("RLP error: {0}")]
    Rlp(#[from] ethereum_rlp::RlpError),
}

pub type Result<T> = std::result::Result<T, TransactionError>;

fn encode_h256_list(list: &[H256]) -> ethereum_types::Bytes {
    let mut encoder = Encoder::new();
    encoder.encode_list(list);
    ethereum_types::Bytes::from_vec(encoder.finish())
}

pub fn encode_access_list(list: &[AccessListItem]) -> ethereum_types::Bytes {
    let mut encoder = Encoder::new();
    encoder.encode_list(list);
    ethereum_types::Bytes::from_vec(encoder.finish())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Transaction {
    Legacy(LegacyTransaction),
    Eip2930(Eip2930Transaction),
    Eip1559(Eip1559Transaction),
    Eip4844(Eip4844Transaction),
    Eip7702(crate::eip7702::Eip7702Transaction),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyTransaction {
    pub nonce: U256,
    pub gas_price: U256,
    pub gas_limit: U256,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Bytes,
    pub v: u64,
    pub r: U256,
    pub s: U256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessListItem {
    pub address: Address,
    pub storage_keys: Vec<H256>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip2930Transaction {
    pub chain_id: u64,
    pub nonce: U256,
    pub gas_price: U256,
    pub gas_limit: U256,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Bytes,
    pub access_list: Vec<AccessListItem>,
    pub y_parity: bool,
    pub r: U256,
    pub s: U256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip1559Transaction {
    pub chain_id: u64,
    pub nonce: U256,
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub to: Option<Address>,
    pub value: U256,
    pub data: Bytes,
    pub access_list: Vec<AccessListItem>,
    pub y_parity: bool,
    pub r: U256,
    pub s: U256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eip4844Transaction {
    pub chain_id: u64,
    pub nonce: U256,
    pub max_priority_fee_per_gas: U256,
    pub max_fee_per_gas: U256,
    pub gas_limit: U256,
    pub to: Address,
    pub value: U256,
    pub data: Bytes,
    pub access_list: Vec<AccessListItem>,
    pub max_fee_per_blob_gas: U256,
    pub blob_versioned_hashes: Vec<H256>,
    pub y_parity: bool,
    pub r: U256,
    pub s: U256,
}

impl Transaction {
    pub fn hash(&self) -> H256 {
        match self {
            Transaction::Legacy(tx) => tx.hash(),
            Transaction::Eip2930(tx) => tx.hash(),
            Transaction::Eip1559(tx) => tx.hash(),
            Transaction::Eip4844(tx) => tx.hash(),
            Transaction::Eip7702(tx) => tx.hash(),
        }
    }

    pub fn sender(&self) -> Result<Address> {
        match self {
            Transaction::Legacy(tx) => tx.sender(),
            Transaction::Eip2930(tx) => tx.sender(),
            Transaction::Eip1559(tx) => tx.sender(),
            Transaction::Eip4844(tx) => tx.sender(),
            Transaction::Eip7702(tx) => tx.sender().map_err(|_| TransactionError::InvalidSignature),
        }
    }

    pub fn nonce(&self) -> U256 {
        match self {
            Transaction::Legacy(tx) => tx.nonce,
            Transaction::Eip2930(tx) => tx.nonce,
            Transaction::Eip1559(tx) => tx.nonce,
            Transaction::Eip4844(tx) => tx.nonce,
            Transaction::Eip7702(tx) => tx.nonce,
        }
    }

    pub fn gas_price(&self) -> U256 {
        match self {
            Transaction::Legacy(tx) => tx.gas_price,
            Transaction::Eip2930(tx) => tx.gas_price,
            Transaction::Eip1559(tx) => tx.max_fee_per_gas,
            Transaction::Eip4844(tx) => tx.max_fee_per_gas,
            Transaction::Eip7702(tx) => tx.max_fee_per_gas,
        }
    }

    pub fn from(&self) -> Address {
        match self {
            Transaction::Legacy(tx) => tx.sender().unwrap_or(Address::zero()),
            Transaction::Eip2930(tx) => tx.sender().unwrap_or(Address::zero()),
            Transaction::Eip1559(tx) => tx.sender().unwrap_or(Address::zero()),
            Transaction::Eip4844(tx) => tx.sender().unwrap_or(Address::zero()),
            Transaction::Eip7702(tx) => tx.sender().unwrap_or(Address::zero()),
        }
    }

    pub fn gas_limit(&self) -> U256 {
        match self {
            Transaction::Legacy(tx) => tx.gas_limit,
            Transaction::Eip2930(tx) => tx.gas_limit,
            Transaction::Eip1559(tx) => tx.gas_limit,
            Transaction::Eip4844(tx) => tx.gas_limit,
            Transaction::Eip7702(tx) => tx.gas_limit,
        }
    }

    pub fn to(&self) -> Option<Address> {
        match self {
            Transaction::Legacy(tx) => tx.to,
            Transaction::Eip2930(tx) => tx.to,
            Transaction::Eip1559(tx) => tx.to,
            Transaction::Eip4844(tx) => Some(tx.to),
            Transaction::Eip7702(tx) => Some(tx.to),
        }
    }

    pub fn value(&self) -> U256 {
        match self {
            Transaction::Legacy(tx) => tx.value,
            Transaction::Eip2930(tx) => tx.value,
            Transaction::Eip1559(tx) => tx.value,
            Transaction::Eip4844(tx) => tx.value,
            Transaction::Eip7702(tx) => tx.value,
        }
    }

    pub fn data(&self) -> &Bytes {
        match self {
            Transaction::Legacy(tx) => &tx.data,
            Transaction::Eip2930(tx) => &tx.data,
            Transaction::Eip1559(tx) => &tx.data,
            Transaction::Eip4844(tx) => &tx.data,
            Transaction::Eip7702(tx) => &tx.data,
        }
    }
}

impl LegacyTransaction {
    pub fn hash(&self) -> H256 {
        keccak256(&ethereum_rlp::encode(self))
    }

    pub fn signing_hash(&self, chain_id: Option<u64>) -> H256 {
        if let Some(chain_id) = chain_id {
            let mut tx = self.clone();
            tx.v = chain_id;
            tx.r = U256::zero();
            tx.s = U256::zero();
            let mut encoder = Encoder::new();
            encoder.encode_list(&[
                ethereum_rlp::encode(&tx.nonce),
                ethereum_rlp::encode(&tx.gas_price),
                ethereum_rlp::encode(&tx.gas_limit),
                ethereum_rlp::encode(&tx.to),
                ethereum_rlp::encode(&tx.value),
                ethereum_rlp::encode(&tx.data),
                ethereum_rlp::encode(&chain_id),
                ethereum_rlp::encode(&0u8),
                ethereum_rlp::encode(&0u8),
            ]);
            keccak256(&encoder.finish())
        } else {
            let mut encoder = Encoder::new();
            encoder.encode_list(&[
                ethereum_rlp::encode(&self.nonce),
                ethereum_rlp::encode(&self.gas_price),
                ethereum_rlp::encode(&self.gas_limit),
                ethereum_rlp::encode(&self.to),
                ethereum_rlp::encode(&self.value),
                ethereum_rlp::encode(&self.data),
            ]);
            keccak256(&encoder.finish())
        }
    }

    pub fn sender(&self) -> Result<Address> {
        let chain_id = if self.v >= 35 {
            Some((self.v - 35) / 2)
        } else {
            None
        };

        let v = if self.v >= 35 {
            ((self.v - 35) % 2) as u8
        } else {
            (self.v - 27) as u8
        };

        let mut r_bytes = [0u8; 32];
        self.r.to_big_endian(&mut r_bytes);
        let mut s_bytes = [0u8; 32];
        self.s.to_big_endian(&mut s_bytes);
        
        let signature = Signature {
            r: H256::from(r_bytes),
            s: H256::from(s_bytes),
            v: v + 27,
        };

        Ok(recover_address(&self.signing_hash(chain_id), &signature)?)
    }
}

impl Eip2930Transaction {
    pub fn hash(&self) -> H256 {
        keccak256(&[&[0x01], &ethereum_rlp::encode(self)[..]].concat())
    }

    pub fn signing_hash(&self) -> H256 {
        let mut encoder = Encoder::new();
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.gas_price),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            encode_access_list(&self.access_list),
        ]);
        keccak256(&[&[0x01], &encoder.finish()[..]].concat())
    }

    pub fn sender(&self) -> Result<Address> {
        let mut r_bytes = [0u8; 32];
        self.r.to_big_endian(&mut r_bytes);
        let mut s_bytes = [0u8; 32];
        self.s.to_big_endian(&mut s_bytes);
        
        let signature = Signature {
            r: H256::from(r_bytes),
            s: H256::from(s_bytes),
            v: if self.y_parity { 28 } else { 27 },
        };

        Ok(recover_address(&self.signing_hash(), &signature)?)
    }
}

impl Eip1559Transaction {
    pub fn hash(&self) -> H256 {
        keccak256(&[&[0x02], &ethereum_rlp::encode(self)[..]].concat())
    }

    pub fn signing_hash(&self) -> H256 {
        let mut encoder = Encoder::new();
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.max_priority_fee_per_gas),
            ethereum_rlp::encode(&self.max_fee_per_gas),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            encode_access_list(&self.access_list),
        ]);
        keccak256(&[&[0x02], &encoder.finish()[..]].concat())
    }

    pub fn sender(&self) -> Result<Address> {
        let mut r_bytes = [0u8; 32];
        self.r.to_big_endian(&mut r_bytes);
        let mut s_bytes = [0u8; 32];
        self.s.to_big_endian(&mut s_bytes);
        
        let signature = Signature {
            r: H256::from(r_bytes),
            s: H256::from(s_bytes),
            v: if self.y_parity { 28 } else { 27 },
        };

        Ok(recover_address(&self.signing_hash(), &signature)?)
    }
}

impl Eip4844Transaction {
    pub fn hash(&self) -> H256 {
        keccak256(&[&[0x03], &ethereum_rlp::encode(self)[..]].concat())
    }

    pub fn signing_hash(&self) -> H256 {
        let mut encoder = Encoder::new();
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.max_priority_fee_per_gas),
            ethereum_rlp::encode(&self.max_fee_per_gas),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            encode_access_list(&self.access_list),
            ethereum_rlp::encode(&self.max_fee_per_blob_gas),
            encode_h256_list(&self.blob_versioned_hashes),
        ]);
        keccak256(&[&[0x03], &encoder.finish()[..]].concat())
    }

    pub fn sender(&self) -> Result<Address> {
        let mut r_bytes = [0u8; 32];
        self.r.to_big_endian(&mut r_bytes);
        let mut s_bytes = [0u8; 32];
        self.s.to_big_endian(&mut s_bytes);
        
        let signature = Signature {
            r: H256::from(r_bytes),
            s: H256::from(s_bytes),
            v: if self.y_parity { 28 } else { 27 },
        };

        Ok(recover_address(&self.signing_hash(), &signature)?)
    }
}

impl Encode for LegacyTransaction {
    fn encode(&self, encoder: &mut ethereum_rlp::Encoder) {
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.gas_price),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            ethereum_rlp::encode(&self.v),
            ethereum_rlp::encode(&self.r),
            ethereum_rlp::encode(&self.s),
        ]);
    }
}

impl Encode for AccessListItem {
    fn encode(&self, encoder: &mut ethereum_rlp::Encoder) {
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.address),
            encode_h256_list(&self.storage_keys),
        ]);
    }
}

impl Encode for Eip2930Transaction {
    fn encode(&self, encoder: &mut ethereum_rlp::Encoder) {
        encoder.encode_list(&[
            ethereum_rlp::encode(&self.chain_id),
            ethereum_rlp::encode(&self.nonce),
            ethereum_rlp::encode(&self.gas_price),
            ethereum_rlp::encode(&self.gas_limit),
            ethereum_rlp::encode(&self.to),
            ethereum_rlp::encode(&self.value),
            ethereum_rlp::encode(&self.data),
            encode_access_list(&self.access_list),
            ethereum_rlp::encode(&self.y_parity),
            ethereum_rlp::encode(&self.r),
            ethereum_rlp::encode(&self.s),
        ]);
    }
}

impl Encode for Eip1559Transaction {
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
            encode_access_list(&self.access_list),
            ethereum_rlp::encode(&self.y_parity),
            ethereum_rlp::encode(&self.r),
            ethereum_rlp::encode(&self.s),
        ]);
    }
}

impl Encode for Eip4844Transaction {
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
            encode_access_list(&self.access_list),
            ethereum_rlp::encode(&self.max_fee_per_blob_gas),
            encode_h256_list(&self.blob_versioned_hashes),
            ethereum_rlp::encode(&self.y_parity),
            ethereum_rlp::encode(&self.r),
            ethereum_rlp::encode(&self.s),
        ]);
    }
}


impl Decode for LegacyTransaction {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        let items: Vec<ethereum_rlp::RlpItem> = decoder.decode_list()?;
        if items.len() != 9 {
            return Err(ethereum_rlp::DecoderError::InvalidData(
                format!("Expected 9 items for legacy transaction, got {}", items.len())
            ).into());
        }
        
        Ok(LegacyTransaction {
            nonce: U256::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[0]))?)?,
            gas_price: U256::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[1]))?)?,
            gas_limit: U256::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[2]))?)?,
            to: Option::<Address>::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[3]))?)?,
            value: U256::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[4]))?)?,
            data: Bytes::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[5]))?)?,
            v: u64::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[6]))?)?,
            r: U256::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[7]))?)?,
            s: U256::decode(&mut ethereum_rlp::Decoder::new(&ethereum_rlp::encode(&items[8]))?)?,
        })
    }
}

impl Decode for AccessListItem {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        Ok(AccessListItem {
            address: Address::decode(decoder)?,
            storage_keys: decoder.decode_list()?,
        })
    }
}

impl Decode for Eip2930Transaction {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        Ok(Eip2930Transaction {
            chain_id: u64::decode(decoder)?,
            nonce: U256::decode(decoder)?,
            gas_price: U256::decode(decoder)?,
            gas_limit: U256::decode(decoder)?,
            to: Option::<Address>::decode(decoder)?,
            value: U256::decode(decoder)?,
            data: Bytes::decode(decoder)?,
            access_list: decoder.decode_list()?,
            y_parity: bool::decode(decoder)?,
            r: U256::decode(decoder)?,
            s: U256::decode(decoder)?,
        })
    }
}

impl Decode for Eip1559Transaction {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        Ok(Eip1559Transaction {
            chain_id: u64::decode(decoder)?,
            nonce: U256::decode(decoder)?,
            max_priority_fee_per_gas: U256::decode(decoder)?,
            max_fee_per_gas: U256::decode(decoder)?,
            gas_limit: U256::decode(decoder)?,
            to: Option::<Address>::decode(decoder)?,
            value: U256::decode(decoder)?,
            data: Bytes::decode(decoder)?,
            access_list: decoder.decode_list()?,
            y_parity: bool::decode(decoder)?,
            r: U256::decode(decoder)?,
            s: U256::decode(decoder)?,
        })
    }
}

impl Decode for Eip4844Transaction {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        Ok(Eip4844Transaction {
            chain_id: u64::decode(decoder)?,
            nonce: U256::decode(decoder)?,
            max_priority_fee_per_gas: U256::decode(decoder)?,
            max_fee_per_gas: U256::decode(decoder)?,
            gas_limit: U256::decode(decoder)?,
            to: Address::decode(decoder)?,
            value: U256::decode(decoder)?,
            data: Bytes::decode(decoder)?,
            access_list: decoder.decode_list()?,
            max_fee_per_blob_gas: U256::decode(decoder)?,
            blob_versioned_hashes: decoder.decode_list()?,
            y_parity: bool::decode(decoder)?,
            r: U256::decode(decoder)?,
            s: U256::decode(decoder)?,
        })
    }
}

impl Encode for Transaction {
    fn encode(&self, encoder: &mut ethereum_rlp::Encoder) {
        match self {
            Transaction::Legacy(tx) => tx.encode(encoder),
            Transaction::Eip2930(tx) => {
                let mut encoded = vec![0x01];
                encoded.extend_from_slice(&ethereum_rlp::encode(tx));
                encoder.encode_bytes(&encoded);
            }
            Transaction::Eip1559(tx) => {
                let mut encoded = vec![0x02];
                encoded.extend_from_slice(&ethereum_rlp::encode(tx));
                encoder.encode_bytes(&encoded);
            }
            Transaction::Eip4844(tx) => {
                let mut encoded = vec![0x03];
                encoded.extend_from_slice(&ethereum_rlp::encode(tx));
                encoder.encode_bytes(&encoded);
            }
            Transaction::Eip7702(tx) => {
                let mut encoded = vec![0x04];
                encoded.extend_from_slice(&ethereum_rlp::encode(tx));
                encoder.encode_bytes(&encoded);
            }
        }
    }
}

impl Decode for Transaction {
    fn decode(decoder: &mut ethereum_rlp::Decoder) -> std::result::Result<Self, ethereum_rlp::RlpError> {
        // Check if this is a typed transaction
        let bytes = decoder.peek_bytes();
        if bytes.len() > 0 && bytes[0] <= 0x7f {
            // This is a typed transaction
            let tx_type = bytes[0];
            let tx_data = &bytes[1..];
            
            match tx_type {
                0x01 => {
                    let mut decoder = ethereum_rlp::Decoder::new(tx_data);
                    Ok(Transaction::Eip2930(Eip2930Transaction::decode(&mut decoder)?))
                }
                0x02 => {
                    let mut decoder = ethereum_rlp::Decoder::new(tx_data);
                    Ok(Transaction::Eip1559(Eip1559Transaction::decode(&mut decoder)?))
                }
                0x03 => {
                    let mut decoder = ethereum_rlp::Decoder::new(tx_data);
                    Ok(Transaction::Eip4844(Eip4844Transaction::decode(&mut decoder)?))
                }
                0x04 => {
                    let mut decoder = ethereum_rlp::Decoder::new(tx_data);
                    Ok(Transaction::Eip7702(crate::eip7702::Eip7702Transaction::decode(&mut decoder)?))
                }
                _ => Err(ethereum_rlp::RlpError::Custom(format!("Unknown transaction type: {}", tx_type)))
            }
        } else {
            // Legacy transaction
            Ok(Transaction::Legacy(LegacyTransaction::decode(decoder)?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_transaction_hash() {
        let tx = LegacyTransaction {
            nonce: U256::from(0),
            gas_price: U256::from(20_000_000_000u64),
            gas_limit: U256::from(21_000),
            to: Some("0x3535353535353535353535353535353535353535".parse().unwrap()),
            value: U256::from(1_000_000_000_000_000_000u64),
            data: Bytes::new(),
            v: 27,
            r: U256::from_str_radix(
                "9cfaa76d97113b60debaf37eeb2e97eb8e99dd7ad5b1bdb9bf17a45b3541f073",
                16,
            )
            .unwrap(),
            s: U256::from_str_radix(
                "4e0c1e8b0f3f7d3c1e8b0f3f7d3c1e8b0f3f7d3c1e8b0f3f7d3c1e8b0f3f7d3c",
                16,
            )
            .unwrap(),
        };

        let hash = tx.hash();
        assert_eq!(hash.0.len(), 32);
    }

    #[test]
    fn test_transaction_rlp_roundtrip() {
        let tx = Transaction::Legacy(LegacyTransaction {
            nonce: U256::from(42),
            gas_price: U256::from(1_000_000_000u64),
            gas_limit: U256::from(50_000),
            to: None,
            value: U256::zero(),
            data: vec![0x60, 0x80, 0x60, 0x40].into(),
            v: 27,
            r: U256::from(1),
            s: U256::from(2),
        });

        let encoded = ethereum_rlp::encode(&tx);
        let decoded: Transaction = ethereum_rlp::decode(&encoded).unwrap();

        assert_eq!(tx, decoded);
    }

    #[test]
    fn test_eip1559_transaction() {
        let tx = Eip1559Transaction {
            chain_id: 1,
            nonce: U256::from(0),
            max_priority_fee_per_gas: U256::from(1_000_000_000u64),
            max_fee_per_gas: U256::from(2_000_000_000u64),
            gas_limit: U256::from(21_000),
            to: Some("0x3535353535353535353535353535353535353535".parse().unwrap()),
            value: U256::from(1_000_000_000_000_000_000u64),
            data: Bytes::new(),
            access_list: vec![],
            y_parity: false,
            r: U256::from(1),
            s: U256::from(2),
        };

        let hash = tx.hash();
        assert_eq!(hash.0.len(), 32);
        
        let signing_hash = tx.signing_hash();
        assert_eq!(signing_hash.0.len(), 32);
    }
}