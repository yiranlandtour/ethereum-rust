use crate::{DecoderError, RlpError, RlpItem};
use crate::traits::Decode;

pub struct Decoder<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, RlpError> {
        Ok(Decoder { data, position: 0 })
    }
    
    pub fn decode_bytes(&mut self) -> Result<Vec<u8>, RlpError> {
        let (offset, len, is_data) = self.decode_header()?;
        
        if !is_data {
            return Err(DecoderError::InvalidData("Expected data, got list".to_string()).into());
        }
        
        self.position += offset;
        
        if self.position + len > self.data.len() {
            return Err(DecoderError::UnexpectedEof.into());
        }
        
        let bytes = self.data[self.position..self.position + len].to_vec();
        self.position += len;
        
        Ok(bytes)
    }
    
    pub fn decode_list<T: Decode>(&mut self) -> Result<Vec<T>, RlpError> {
        let (offset, len, is_data) = self.decode_header()?;
        
        if is_data {
            return Err(DecoderError::InvalidData("Expected list, got data".to_string()).into());
        }
        
        self.position += offset;
        let end_position = self.position + len;
        
        if end_position > self.data.len() {
            return Err(DecoderError::UnexpectedEof.into());
        }
        
        let mut items = Vec::new();
        
        while self.position < end_position {
            items.push(T::decode(self)?);
        }
        
        if self.position != end_position {
            return Err(DecoderError::ListLengthMismatch {
                expected: len,
                actual: self.position - (end_position - len),
            }.into());
        }
        
        Ok(items)
    }
    
    pub fn decode_item(&mut self) -> Result<RlpItem, RlpError> {
        let (offset, len, is_data) = self.decode_header()?;
        
        self.position += offset;
        
        if self.position + len > self.data.len() {
            return Err(DecoderError::UnexpectedEof.into());
        }
        
        if is_data {
            let bytes = self.data[self.position..self.position + len].to_vec();
            self.position += len;
            Ok(RlpItem::String(bytes))
        } else {
            let end_position = self.position + len;
            let mut items = Vec::new();
            
            while self.position < end_position {
                items.push(self.decode_item()?);
            }
            
            Ok(RlpItem::List(items))
        }
    }
    
    pub fn decode_u8(&mut self) -> Result<u8, RlpError> {
        let bytes = self.decode_bytes()?;
        match bytes.len() {
            0 => Ok(0),
            1 => Ok(bytes[0]),
            _ => Err(DecoderError::IntegerOverflow.into()),
        }
    }
    
    pub fn decode_u16(&mut self) -> Result<u16, RlpError> {
        let bytes = self.decode_bytes()?;
        match bytes.len() {
            0 => Ok(0),
            1 => Ok(bytes[0] as u16),
            2 => {
                if bytes[0] == 0 {
                    return Err(DecoderError::LeadingZeros.into());
                }
                Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
            }
            _ => Err(DecoderError::IntegerOverflow.into()),
        }
    }
    
    pub fn decode_u32(&mut self) -> Result<u32, RlpError> {
        let bytes = self.decode_bytes()?;
        if bytes.is_empty() {
            return Ok(0);
        }
        if bytes.len() > 4 {
            return Err(DecoderError::IntegerOverflow.into());
        }
        if bytes.len() > 1 && bytes[0] == 0 {
            return Err(DecoderError::LeadingZeros.into());
        }
        
        let mut array = [0u8; 4];
        array[4 - bytes.len()..].copy_from_slice(&bytes);
        Ok(u32::from_be_bytes(array))
    }
    
    pub fn decode_u64(&mut self) -> Result<u64, RlpError> {
        let bytes = self.decode_bytes()?;
        if bytes.is_empty() {
            return Ok(0);
        }
        if bytes.len() > 8 {
            return Err(DecoderError::IntegerOverflow.into());
        }
        if bytes.len() > 1 && bytes[0] == 0 {
            return Err(DecoderError::LeadingZeros.into());
        }
        
        let mut array = [0u8; 8];
        array[8 - bytes.len()..].copy_from_slice(&bytes);
        Ok(u64::from_be_bytes(array))
    }
    
    pub fn decode_bool(&mut self) -> Result<bool, RlpError> {
        match self.decode_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecoderError::InvalidData("Invalid boolean value".to_string()).into()),
        }
    }
    
    pub fn is_empty_string(&self) -> Result<bool, RlpError> {
        if self.position >= self.data.len() {
            return Err(DecoderError::UnexpectedEof.into());
        }
        Ok(self.data[self.position] == 0x80)
    }
    
    pub fn is_list(&self) -> Result<bool, RlpError> {
        if self.position >= self.data.len() {
            return Err(DecoderError::UnexpectedEof.into());
        }
        Ok(self.data[self.position] >= 0xc0)
    }
    
    pub fn is_finished(&self) -> bool {
        self.position >= self.data.len()
    }
    
    fn decode_header(&mut self) -> Result<(usize, usize, bool), RlpError> {
        if self.position >= self.data.len() {
            return Err(DecoderError::UnexpectedEof.into());
        }
        
        let prefix = self.data[self.position];
        
        match prefix {
            0x00..=0x7f => Ok((0, 1, true)),
            0x80 => Ok((1, 0, true)),
            0x81..=0xb7 => {
                let len = (prefix - 0x80) as usize;
                Ok((1, len, true))
            }
            0xb8..=0xbf => {
                let len_of_len = (prefix - 0xb7) as usize;
                if self.position + 1 + len_of_len > self.data.len() {
                    return Err(DecoderError::UnexpectedEof.into());
                }
                
                let len = decode_length(&self.data[self.position + 1..self.position + 1 + len_of_len])?;
                Ok((1 + len_of_len, len, true))
            }
            0xc0 => Ok((1, 0, false)),
            0xc1..=0xf7 => {
                let len = (prefix - 0xc0) as usize;
                Ok((1, len, false))
            }
            0xf8..=0xff => {
                let len_of_len = (prefix - 0xf7) as usize;
                if self.position + 1 + len_of_len > self.data.len() {
                    return Err(DecoderError::UnexpectedEof.into());
                }
                
                let len = decode_length(&self.data[self.position + 1..self.position + 1 + len_of_len])?;
                Ok((1 + len_of_len, len, false))
            }
        }
    }
}

fn decode_length(bytes: &[u8]) -> Result<usize, RlpError> {
    if bytes.is_empty() {
        return Err(DecoderError::InvalidData("Empty length bytes".to_string()).into());
    }
    
    if bytes[0] == 0 {
        return Err(DecoderError::LeadingZeros.into());
    }
    
    let mut len = 0usize;
    for &byte in bytes {
        len = len.checked_shl(8)
            .and_then(|l| l.checked_add(byte as usize))
            .ok_or(DecoderError::IntegerOverflow)?;
    }
    
    Ok(len)
}

pub trait Decodable: Sized {
    fn rlp_decode(decoder: &mut Decoder) -> Result<Self, RlpError>;
}

impl<T: Decode> Decodable for T {
    fn rlp_decode(decoder: &mut Decoder) -> Result<Self, RlpError> {
        T::decode(decoder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decode_single_byte() {
        let mut decoder = Decoder::new(&[0x00]).unwrap();
        assert_eq!(decoder.decode_bytes().unwrap(), vec![0x00]);
        
        let mut decoder = Decoder::new(&[0x7f]).unwrap();
        assert_eq!(decoder.decode_bytes().unwrap(), vec![0x7f]);
    }
    
    #[test]
    fn test_decode_string() {
        let mut decoder = Decoder::new(&[0x83, b'd', b'o', b'g']).unwrap();
        assert_eq!(decoder.decode_bytes().unwrap(), b"dog");
    }
    
    #[test]
    fn test_decode_list() {
        let mut decoder = Decoder::new(&[0xc8, 0x83, b'c', b'a', b't', 0x83, b'd', b'o', b'g']).unwrap();
        let items: Vec<Vec<u8>> = decoder.decode_list().unwrap();
        assert_eq!(items, vec![b"cat".to_vec(), b"dog".to_vec()]);
    }
    
    #[test]
    fn test_decode_empty() {
        let mut decoder = Decoder::new(&[0x80]).unwrap();
        assert_eq!(decoder.decode_bytes().unwrap(), vec![]);
    }
    
    #[test]
    fn test_decode_empty_list() {
        let mut decoder = Decoder::new(&[0xc0]).unwrap();
        let items: Vec<Vec<u8>> = decoder.decode_list().unwrap();
        assert!(items.is_empty());
    }
}