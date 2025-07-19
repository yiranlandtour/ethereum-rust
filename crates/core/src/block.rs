use ethereum_types::{Address, Bloom, H256, U256};
use ethereum_rlp::{Decode, Decoder, Encode, Encoder, RlpError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub parent_hash: H256,
    pub ommers_hash: H256,
    pub beneficiary: Address,
    pub state_root: H256,
    pub transactions_root: H256,
    pub receipts_root: H256,
    pub logs_bloom: Bloom,
    pub difficulty: U256,
    pub number: U256,
    pub gas_limit: U256,
    pub gas_used: U256,
    pub timestamp: u64,
    pub extra_data: Vec<u8>,
    pub mix_hash: H256,
    pub nonce: u64,
    
    // EIP-1559 fields (post-London)
    pub base_fee_per_gas: Option<U256>,
    
    // EIP-4844 fields (post-Cancun)
    pub blob_gas_used: Option<u64>,
    pub excess_blob_gas: Option<u64>,
    
    // EIP-4788 fields (post-Cancun)
    pub parent_beacon_block_root: Option<H256>,
    
    // EIP-4895 fields (post-Shanghai)
    pub withdrawals_root: Option<H256>,
}

impl Header {
    pub fn new() -> Self {
        Self {
            parent_hash: H256::zero(),
            ommers_hash: H256::zero(),
            beneficiary: Address::ZERO,
            state_root: H256::zero(),
            transactions_root: H256::zero(),
            receipts_root: H256::zero(),
            logs_bloom: Bloom::ZERO,
            difficulty: U256::zero(),
            number: U256::zero(),
            gas_limit: U256::zero(),
            gas_used: U256::zero(),
            timestamp: 0,
            extra_data: Vec::new(),
            mix_hash: H256::zero(),
            nonce: 0,
            base_fee_per_gas: None,
            blob_gas_used: None,
            excess_blob_gas: None,
            parent_beacon_block_root: None,
            withdrawals_root: None,
        }
    }
    
    pub fn hash(&self) -> H256 {
        use ethereum_crypto::keccak256;
        let mut encoder = Encoder::new();
        self.encode(&mut encoder);
        H256::from_slice(keccak256(&encoder.finish()).as_bytes())
    }
    
    pub fn is_genesis(&self) -> bool {
        self.parent_hash == H256::zero() && self.number == U256::zero()
    }
    
    pub fn seal(&self) -> Vec<H256> {
        vec![
            H256::from_low_u64_be(self.nonce),
            self.mix_hash,
        ]
    }
}

impl Encode for Header {
    fn encode(&self, encoder: &mut Encoder) {
        // Manually encode as list by encoding each field
        let mut list_encoder = Encoder::new();
        
        self.parent_hash.encode(&mut list_encoder);
        self.ommers_hash.encode(&mut list_encoder);
        self.beneficiary.encode(&mut list_encoder);
        self.state_root.encode(&mut list_encoder);
        self.transactions_root.encode(&mut list_encoder);
        self.receipts_root.encode(&mut list_encoder);
        self.logs_bloom.encode(&mut list_encoder);
        self.difficulty.encode(&mut list_encoder);
        self.number.encode(&mut list_encoder);
        self.gas_limit.encode(&mut list_encoder);
        self.gas_used.encode(&mut list_encoder);
        self.timestamp.encode(&mut list_encoder);
        self.extra_data.encode(&mut list_encoder);
        self.mix_hash.encode(&mut list_encoder);
        self.nonce.encode(&mut list_encoder);
        
        // Optional fields
        if let Some(base_fee) = &self.base_fee_per_gas {
            base_fee.encode(&mut list_encoder);
        }
        
        if let Some(withdrawals_root) = &self.withdrawals_root {
            withdrawals_root.encode(&mut list_encoder);
        }
        
        if let Some(blob_gas_used) = &self.blob_gas_used {
            blob_gas_used.encode(&mut list_encoder);
        }
        
        if let Some(excess_blob_gas) = &self.excess_blob_gas {
            excess_blob_gas.encode(&mut list_encoder);
        }
        
        if let Some(parent_beacon_block_root) = &self.parent_beacon_block_root {
            parent_beacon_block_root.encode(&mut list_encoder);
        }
        
        let list_bytes = list_encoder.finish();
        
        // Encode as RLP list
        match list_bytes.len() {
            len if len < 56 => {
                encoder.encode_bytes(&[0xc0 + len as u8]);
                encoder.encode_bytes(&list_bytes);
            }
            len => {
                let len_bytes = encode_length(len);
                encoder.encode_bytes(&[0xf7 + len_bytes.len() as u8]);
                encoder.encode_bytes(&len_bytes);
                encoder.encode_bytes(&list_bytes);
            }
        }
    }
}

fn encode_length(len: usize) -> Vec<u8> {
    if len < 256 {
        vec![len as u8]
    } else if len < 65536 {
        vec![(len >> 8) as u8, len as u8]
    } else if len < 16777216 {
        vec![(len >> 16) as u8, (len >> 8) as u8, len as u8]
    } else {
        vec![
            (len >> 24) as u8,
            (len >> 16) as u8,
            (len >> 8) as u8,
            len as u8,
        ]
    }
}

// Create a custom decoder that tracks position
struct ListDecoder<'a> {
    items: Vec<ethereum_rlp::RlpItem>,
    position: usize,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> ListDecoder<'a> {
    fn new(decoder: &mut Decoder<'a>) -> Result<Self, RlpError> {
        let items = decoder.decode_item()?.as_list()
            .ok_or_else(|| RlpError::Decoder(ethereum_rlp::DecoderError::InvalidData(
                "Expected list".to_string()
            )))?
            .to_vec();
        
        Ok(ListDecoder {
            items,
            position: 0,
            _phantom: std::marker::PhantomData,
        })
    }
    
    fn is_finished(&self) -> bool {
        self.position >= self.items.len()
    }
    
    fn decode<T: Decode>(&mut self) -> Result<T, RlpError> {
        if self.position >= self.items.len() {
            return Err(RlpError::Decoder(ethereum_rlp::DecoderError::UnexpectedEof));
        }
        
        let item = &self.items[self.position];
        self.position += 1;
        
        // Re-encode the item and decode it with the proper type
        let mut encoder = Encoder::new();
        encode_rlp_item(item, &mut encoder);
        let bytes = encoder.finish();
        let mut decoder = Decoder::new(&bytes)?;
        T::decode(&mut decoder)
    }
}

impl Decode for Header {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let mut list = ListDecoder::new(decoder)?;
        
        let parent_hash = list.decode()?;
        let ommers_hash = list.decode()?;
        let beneficiary = list.decode()?;
        let state_root = list.decode()?;
        let transactions_root = list.decode()?;
        let receipts_root = list.decode()?;
        let logs_bloom = list.decode()?;
        let difficulty = list.decode()?;
        let number = list.decode()?;
        let gas_limit = list.decode()?;
        let gas_used = list.decode()?;
        let timestamp = list.decode()?;
        let extra_data = list.decode()?;
        let mix_hash = list.decode()?;
        let nonce = list.decode()?;
        
        let base_fee_per_gas = if !list.is_finished() {
            Some(list.decode()?)
        } else {
            None
        };
        
        let withdrawals_root = if !list.is_finished() {
            Some(list.decode()?)
        } else {
            None
        };
        
        let blob_gas_used = if !list.is_finished() {
            Some(list.decode()?)
        } else {
            None
        };
        
        let excess_blob_gas = if !list.is_finished() {
            Some(list.decode()?)
        } else {
            None
        };
        
        let parent_beacon_block_root = if !list.is_finished() {
            Some(list.decode()?)
        } else {
            None
        };
        
        Ok(Header {
            parent_hash,
            ommers_hash,
            beneficiary,
            state_root,
            transactions_root,
            receipts_root,
            logs_bloom,
            difficulty,
            number,
            gas_limit,
            gas_used,
            timestamp,
            extra_data,
            mix_hash,
            nonce,
            base_fee_per_gas,
            withdrawals_root,
            blob_gas_used,
            excess_blob_gas,
            parent_beacon_block_root,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub header: Header,
    pub transactions: Vec<Vec<u8>>, // Will be replaced with proper Transaction type
    pub ommers: Vec<Header>,
    pub withdrawals: Option<Vec<Withdrawal>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Withdrawal {
    pub index: u64,
    pub validator_index: u64,
    pub address: Address,
    pub amount: u64, // Amount in Gwei
}

impl Encode for Withdrawal {
    fn encode(&self, encoder: &mut Encoder) {
        let mut list_encoder = Encoder::new();
        
        self.index.encode(&mut list_encoder);
        self.validator_index.encode(&mut list_encoder);
        self.address.encode(&mut list_encoder);
        self.amount.encode(&mut list_encoder);
        
        let list_bytes = list_encoder.finish();
        
        // Encode as RLP list
        match list_bytes.len() {
            len if len < 56 => {
                encoder.encode_bytes(&[0xc0 + len as u8]);
                encoder.encode_bytes(&list_bytes);
            }
            len => {
                let len_bytes = encode_length(len);
                encoder.encode_bytes(&[0xf7 + len_bytes.len() as u8]);
                encoder.encode_bytes(&len_bytes);
                encoder.encode_bytes(&list_bytes);
            }
        }
    }
}

impl Decode for Withdrawal {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let mut list = ListDecoder::new(decoder)?;
        
        Ok(Withdrawal {
            index: list.decode()?,
            validator_index: list.decode()?,
            address: list.decode()?,
            amount: list.decode()?,
        })
    }
}

impl Block {
    pub fn new(header: Header) -> Self {
        Self {
            header,
            transactions: Vec::new(),
            ommers: Vec::new(),
            withdrawals: None,
        }
    }
    
    pub fn hash(&self) -> H256 {
        self.header.hash()
    }
    
    pub fn number(&self) -> U256 {
        self.header.number
    }
    
    pub fn gas_limit(&self) -> U256 {
        self.header.gas_limit
    }
    
    pub fn gas_used(&self) -> U256 {
        self.header.gas_used
    }
    
    pub fn timestamp(&self) -> u64 {
        self.header.timestamp
    }
    
    pub fn difficulty(&self) -> U256 {
        self.header.difficulty
    }
}

impl Encode for Block {
    fn encode(&self, encoder: &mut Encoder) {
        let mut list_encoder = Encoder::new();
        
        // Encode header
        self.header.encode(&mut list_encoder);
        
        // Encode transactions list
        encode_vec(&self.transactions, &mut list_encoder);
        
        // Encode ommers list
        encode_vec(&self.ommers, &mut list_encoder);
        
        // Encode withdrawals if present
        if let Some(withdrawals) = &self.withdrawals {
            encode_vec(withdrawals, &mut list_encoder);
        }
        
        let list_bytes = list_encoder.finish();
        
        // Encode as RLP list
        match list_bytes.len() {
            len if len < 56 => {
                encoder.encode_bytes(&[0xc0 + len as u8]);
                encoder.encode_bytes(&list_bytes);
            }
            len => {
                let len_bytes = encode_length(len);
                encoder.encode_bytes(&[0xf7 + len_bytes.len() as u8]);
                encoder.encode_bytes(&len_bytes);
                encoder.encode_bytes(&list_bytes);
            }
        }
    }
}

// Helper function to encode a vector
fn encode_vec<T: Encode>(items: &[T], encoder: &mut Encoder) {
    let mut list_encoder = Encoder::new();
    for item in items {
        item.encode(&mut list_encoder);
    }
    let list_bytes = list_encoder.finish();
    
    match list_bytes.len() {
        len if len < 56 => {
            encoder.encode_bytes(&[0xc0 + len as u8]);
            encoder.encode_bytes(&list_bytes);
        }
        len => {
            let len_bytes = encode_length(len);
            encoder.encode_bytes(&[0xf7 + len_bytes.len() as u8]);
            encoder.encode_bytes(&len_bytes);
            encoder.encode_bytes(&list_bytes);
        }
    }
}

impl Decode for Block {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let mut list = ListDecoder::new(decoder)?;
        
        let header: Header = list.decode()?;
        
        // Decode transactions list - need special handling for Vec<Vec<u8>>
        let transactions = {
            let tx_item = &list.items[list.position];
            list.position += 1;
            
            let tx_list = tx_item.as_list()
                .ok_or_else(|| RlpError::Decoder(ethereum_rlp::DecoderError::InvalidData(
                    "Expected list for transactions".to_string()
                )))?;
            
            let mut txs = Vec::new();
            for item in tx_list {
                txs.push(item.as_bytes()
                    .ok_or_else(|| RlpError::Decoder(ethereum_rlp::DecoderError::InvalidData(
                        "Invalid transaction data".to_string()
                    )))?
                    .to_vec());
            }
            txs
        };
        
        // Decode ommers list
        let ommers = {
            let ommer_item = &list.items[list.position];
            list.position += 1;
            
            let ommer_list = ommer_item.as_list()
                .ok_or_else(|| RlpError::Decoder(ethereum_rlp::DecoderError::InvalidData(
                    "Expected list for ommers".to_string()
                )))?;
            
            let mut ommers = Vec::new();
            for item in ommer_list {
                // Re-encode and decode each header
                let mut encoder = Encoder::new();
                encode_rlp_item(item, &mut encoder);
                let bytes = encoder.finish();
                let mut decoder = Decoder::new(&bytes)?;
                ommers.push(Header::decode(&mut decoder)?);
            }
            ommers
        };
        
        // Decode withdrawals if present
        let withdrawals = if !list.is_finished() {
            let withdrawal_item = &list.items[list.position];
            list.position += 1;
            
            let withdrawal_list = withdrawal_item.as_list()
                .ok_or_else(|| RlpError::Decoder(ethereum_rlp::DecoderError::InvalidData(
                    "Expected list for withdrawals".to_string()
                )))?;
            
            let mut withdrawals = Vec::new();
            for item in withdrawal_list {
                // Re-encode and decode each withdrawal
                let mut encoder = Encoder::new();
                encode_rlp_item(item, &mut encoder);
                let bytes = encoder.finish();
                let mut decoder = Decoder::new(&bytes)?;
                withdrawals.push(Withdrawal::decode(&mut decoder)?);
            }
            Some(withdrawals)
        } else {
            None
        };
        
        Ok(Block {
            header,
            transactions,
            ommers,
            withdrawals,
        })
    }
}

// Helper function to encode RlpItem back to bytes
fn encode_rlp_item(item: &ethereum_rlp::RlpItem, encoder: &mut Encoder) {
    match item {
        ethereum_rlp::RlpItem::String(bytes) => {
            encoder.encode_bytes(bytes);
        }
        ethereum_rlp::RlpItem::List(items) => {
            let mut list_encoder = Encoder::new();
            for sub_item in items {
                encode_rlp_item(sub_item, &mut list_encoder);
            }
            let list_bytes = list_encoder.finish();
            
            match list_bytes.len() {
                len if len < 56 => {
                    encoder.encode_bytes(&[0xc0 + len as u8]);
                    encoder.encode_bytes(&list_bytes);
                }
                len => {
                    let len_bytes = encode_length(len);
                    encoder.encode_bytes(&[0xf7 + len_bytes.len() as u8]);
                    encoder.encode_bytes(&len_bytes);
                    encoder.encode_bytes(&list_bytes);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_header_creation() {
        let header = Header::new();
        assert_eq!(header.parent_hash, H256::zero());
        assert_eq!(header.number, U256::zero());
        assert!(header.is_genesis());
    }
    
    #[test]
    fn test_header_hash() {
        let header = Header::new();
        let hash = header.hash();
        assert_ne!(hash, H256::zero());
    }
    
    #[test]
    fn test_header_rlp_roundtrip() {
        let mut header = Header::new();
        header.number = U256::from(1);
        header.timestamp = 1234567890;
        header.gas_limit = U256::from(8_000_000);
        
        let mut encoder = Encoder::new();
        header.encode(&mut encoder);
        let encoded = encoder.finish();
        
        let mut decoder = Decoder::new(&encoded).unwrap();
        let decoded = Header::decode(&mut decoder).unwrap();
        
        assert_eq!(header, decoded);
    }
    
    #[test]
    fn test_header_with_eip1559() {
        let mut header = Header::new();
        header.base_fee_per_gas = Some(U256::from(1_000_000_000));
        
        let mut encoder = Encoder::new();
        header.encode(&mut encoder);
        let encoded = encoder.finish();
        
        let mut decoder = Decoder::new(&encoded).unwrap();
        let decoded = Header::decode(&mut decoder).unwrap();
        
        assert_eq!(header, decoded);
        assert_eq!(decoded.base_fee_per_gas, Some(U256::from(1_000_000_000)));
    }
    
    #[test]
    fn test_block_creation() {
        let header = Header::new();
        let block = Block::new(header.clone());
        
        assert_eq!(block.header, header);
        assert_eq!(block.transactions.len(), 0);
        assert_eq!(block.ommers.len(), 0);
    }
    
    #[test]
    fn test_block_rlp_roundtrip() {
        let header = Header::new();
        let block = Block::new(header);
        
        let mut encoder = Encoder::new();
        block.encode(&mut encoder);
        let encoded = encoder.finish();
        
        let mut decoder = Decoder::new(&encoded).unwrap();
        let decoded = Block::decode(&mut decoder).unwrap();
        
        assert_eq!(block, decoded);
    }
    
    #[test]
    fn test_withdrawal_rlp_roundtrip() {
        let withdrawal = Withdrawal {
            index: 42,
            validator_index: 1337,
            address: Address::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xde, 0xad, 0xbe, 0xef]),
            amount: 1_000_000_000, // 1 ETH in Gwei
        };
        
        let mut encoder = Encoder::new();
        withdrawal.encode(&mut encoder);
        let encoded = encoder.finish();
        
        let mut decoder = Decoder::new(&encoded).unwrap();
        let decoded = Withdrawal::decode(&mut decoder).unwrap();
        
        assert_eq!(withdrawal, decoded);
    }
}