use crate::{Decoder, Encoder, RlpError};
use ethereum_types::{Address, Bloom, Bytes, H160, H256, H512, U128, U256, U512};

pub trait Encode {
    fn encode(&self, encoder: &mut Encoder);
}

pub trait Decode: Sized {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError>;
}

impl Encode for u8 {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_u8(*self);
    }
}

impl Decode for u8 {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_u8()
    }
}

impl Encode for u16 {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_u16(*self);
    }
}

impl Decode for u16 {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_u16()
    }
}

impl Encode for u32 {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_u32(*self);
    }
}

impl Decode for u32 {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_u32()
    }
}

impl Encode for u64 {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_u64(*self);
    }
}

impl Decode for u64 {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_u64()
    }
}

impl Encode for bool {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bool(*self);
    }
}

impl Decode for bool {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_bool()
    }
}

impl Encode for &[u8] {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self);
    }
}

impl Encode for Vec<u8> {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self);
    }
}

impl Decode for Vec<u8> {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_bytes()
    }
}

impl Encode for &str {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self.as_bytes());
    }
}

impl Encode for String {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self.as_bytes());
    }
}

impl Decode for String {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let bytes = decoder.decode_bytes()?;
        String::from_utf8(bytes).map_err(|_| {
            RlpError::Decoder(crate::DecoderError::InvalidData(
                "Invalid UTF-8 string".to_string(),
            ))
        })
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_list(self);
    }
}

impl<T: Decode> Decode for Vec<T> {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        decoder.decode_list()
    }
}

impl<T: Encode> Encode for Option<T> {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            Some(value) => value.encode(encoder),
            None => encoder.encode_bytes(&[]),
        }
    }
}

impl<T: Decode> Decode for Option<T> {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        if decoder.is_empty_string()? {
            decoder.decode_bytes()?;
            Ok(None)
        } else {
            Ok(Some(T::decode(decoder)?))
        }
    }
}

impl Encode for Bytes {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self.as_slice());
    }
}

impl Decode for Bytes {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        Ok(Bytes::from_vec(decoder.decode_bytes()?))
    }
}

impl Encode for Address {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self.as_bytes());
    }
}

impl Decode for Address {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let bytes = decoder.decode_bytes()?;
        if bytes.len() != 20 {
            return Err(RlpError::Decoder(crate::DecoderError::InvalidData(
                format!("Invalid address length: {}", bytes.len()),
            )));
        }
        let mut array = [0u8; 20];
        array.copy_from_slice(&bytes);
        Ok(Address::from_bytes(array))
    }
}

impl Encode for H256 {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self.as_bytes());
    }
}

impl Decode for H256 {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let bytes = decoder.decode_bytes()?;
        if bytes.len() != 32 {
            return Err(RlpError::Decoder(crate::DecoderError::InvalidData(
                format!("Invalid H256 length: {}", bytes.len()),
            )));
        }
        Ok(H256::from_slice(&bytes))
    }
}

impl Encode for U256 {
    fn encode(&self, encoder: &mut Encoder) {
        if self.is_zero() {
            encoder.encode_bytes(&[]);
        } else {
            let mut bytes = [0u8; 32];
            self.to_big_endian(&mut bytes);
            let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap();
            encoder.encode_bytes(&bytes[first_non_zero..]);
        }
    }
}

impl Decode for U256 {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let bytes = decoder.decode_bytes()?;
        if bytes.is_empty() {
            Ok(U256::zero())
        } else {
            if bytes.len() > 32 {
                return Err(RlpError::Decoder(crate::DecoderError::IntegerOverflow));
            }
            if bytes.len() > 1 && bytes[0] == 0 {
                return Err(RlpError::Decoder(crate::DecoderError::LeadingZeros));
            }
            Ok(U256::from_big_endian(&bytes))
        }
    }
}

impl Encode for Bloom {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.encode_bytes(self.as_bytes());
    }
}

impl Decode for Bloom {
    fn decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        let bytes = decoder.decode_bytes()?;
        if bytes.len() != 256 {
            return Err(RlpError::Decoder(crate::DecoderError::InvalidData(
                format!("Invalid Bloom filter length: {}", bytes.len()),
            )));
        }
        let mut array = [0u8; 256];
        array.copy_from_slice(&bytes);
        Ok(Bloom::from(array))
    }
}