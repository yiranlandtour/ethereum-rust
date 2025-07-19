use crate::traits::Encode;
use bytes::BytesMut;

pub struct Encoder {
    buffer: BytesMut,
}

impl Encoder {
    pub fn new() -> Self {
        Encoder {
            buffer: BytesMut::new(),
        }
    }
    
    pub fn with_capacity(capacity: usize) -> Self {
        Encoder {
            buffer: BytesMut::with_capacity(capacity),
        }
    }
    
    pub fn finish(self) -> Vec<u8> {
        self.buffer.to_vec()
    }
    
    pub fn encode_bytes(&mut self, bytes: &[u8]) {
        match bytes.len() {
            0 => self.buffer.extend_from_slice(&[0x80]),
            1 if bytes[0] < 0x80 => self.buffer.extend_from_slice(bytes),
            len if len < 56 => {
                self.buffer.extend_from_slice(&[0x80 + len as u8]);
                self.buffer.extend_from_slice(bytes);
            }
            len => {
                let len_bytes = encode_length(len);
                self.buffer.extend_from_slice(&[0xb7 + len_bytes.len() as u8]);
                self.buffer.extend_from_slice(&len_bytes);
                self.buffer.extend_from_slice(bytes);
            }
        }
    }
    
    pub fn encode_list<T: Encode>(&mut self, items: &[T]) {
        let mut list_encoder = Encoder::new();
        for item in items {
            item.encode(&mut list_encoder);
        }
        let list_bytes = list_encoder.finish();
        
        match list_bytes.len() {
            len if len < 56 => {
                self.buffer.extend_from_slice(&[0xc0 + len as u8]);
                self.buffer.extend_from_slice(&list_bytes);
            }
            len => {
                let len_bytes = encode_length(len);
                self.buffer.extend_from_slice(&[0xf7 + len_bytes.len() as u8]);
                self.buffer.extend_from_slice(&len_bytes);
                self.buffer.extend_from_slice(&list_bytes);
            }
        }
    }
    
    pub fn encode_u8(&mut self, value: u8) {
        if value == 0 {
            self.encode_bytes(&[]);
        } else {
            self.encode_bytes(&[value]);
        }
    }
    
    pub fn encode_u16(&mut self, value: u16) {
        if value == 0 {
            self.encode_bytes(&[]);
        } else if value < 256 {
            self.encode_bytes(&[value as u8]);
        } else {
            self.encode_bytes(&value.to_be_bytes());
        }
    }
    
    pub fn encode_u32(&mut self, value: u32) {
        if value == 0 {
            self.encode_bytes(&[]);
        } else {
            let bytes = value.to_be_bytes();
            let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap();
            self.encode_bytes(&bytes[first_non_zero..]);
        }
    }
    
    pub fn encode_u64(&mut self, value: u64) {
        if value == 0 {
            self.encode_bytes(&[]);
        } else {
            let bytes = value.to_be_bytes();
            let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap();
            self.encode_bytes(&bytes[first_non_zero..]);
        }
    }
    
    pub fn encode_bool(&mut self, value: bool) {
        self.encode_u8(if value { 1 } else { 0 });
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

pub trait Encodable {
    fn rlp_append(&self, encoder: &mut Encoder);
}

impl<T: Encode> Encodable for T {
    fn rlp_append(&self, encoder: &mut Encoder) {
        self.encode(encoder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_encode_single_byte() {
        let mut encoder = Encoder::new();
        encoder.encode_bytes(&[0x00]);
        assert_eq!(encoder.finish(), vec![0x00]);
        
        let mut encoder = Encoder::new();
        encoder.encode_bytes(&[0x7f]);
        assert_eq!(encoder.finish(), vec![0x7f]);
    }
    
    #[test]
    fn test_encode_string() {
        let mut encoder = Encoder::new();
        encoder.encode_bytes(b"dog");
        assert_eq!(encoder.finish(), vec![0x83, b'd', b'o', b'g']);
    }
    
    #[test]
    fn test_encode_list() {
        let mut encoder = Encoder::new();
        encoder.encode_list(&["cat", "dog"]);
        assert_eq!(
            encoder.finish(),
            vec![0xc8, 0x83, b'c', b'a', b't', 0x83, b'd', b'o', b'g']
        );
    }
    
    #[test]
    fn test_encode_empty() {
        let mut encoder = Encoder::new();
        encoder.encode_bytes(&[]);
        assert_eq!(encoder.finish(), vec![0x80]);
    }
    
    #[test]
    fn test_encode_empty_list() {
        let mut encoder = Encoder::new();
        encoder.encode_list::<Vec<u8>>(&[]);
        assert_eq!(encoder.finish(), vec![0xc0]);
    }
}