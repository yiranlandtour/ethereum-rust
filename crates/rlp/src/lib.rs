pub mod decode;
pub mod encode;
pub mod error;
pub mod traits;

pub use decode::{Decodable, Decoder};
pub use encode::{Encodable, Encoder};
pub use error::{DecoderError, EncoderError, RlpError};
pub use traits::{Decode, Encode};

use ethereum_types::Bytes;

pub fn encode<T: Encode>(value: &T) -> Bytes {
    let mut encoder = Encoder::new();
    value.encode(&mut encoder);
    Bytes::from_vec(encoder.finish())
}

pub fn decode<T: Decode>(data: &[u8]) -> Result<T, RlpError> {
    let mut decoder = Decoder::new(data)?;
    T::decode(&mut decoder)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RlpItem {
    String(Vec<u8>),
    List(Vec<RlpItem>),
}

impl RlpItem {
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            RlpItem::String(bytes) => Some(bytes),
            RlpItem::List(_) => None,
        }
    }
    
    pub fn as_list(&self) -> Option<&[RlpItem]> {
        match self {
            RlpItem::String(_) => None,
            RlpItem::List(items) => Some(items),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encode_decode_string() {
        let data = b"hello world";
        let encoded = encode(&data.as_slice());
        let decoded: Vec<u8> = decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
    
    #[test]
    fn test_encode_decode_empty() {
        let data: &[u8] = &[];
        let encoded = encode(&data);
        let decoded: Vec<u8> = decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}