use bytes::{BufMut, BytesMut};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum VarIntError {
    #[error("incomplete VarInt")]
    Incomplete,
    #[error("VarInt exceeds five bytes")]
    TooLarge,
}

pub fn encode_varint(value: i32, target: &mut BytesMut) {
    let mut value = value as u32;
    loop {
        if value & !0x7f == 0 {
            target.put_u8(value as u8);
            return;
        }
        target.put_u8(((value & 0x7f) | 0x80) as u8);
        value >>= 7;
    }
}

pub fn decode_varint(source: &[u8]) -> Result<(i32, usize), VarIntError> {
    let mut result = 0_u32;
    for index in 0..5 {
        let Some(&byte) = source.get(index) else {
            return Err(VarIntError::Incomplete);
        };
        result |= u32::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Ok((result as i32, index + 1));
        }
    }
    Err(VarIntError::TooLarge)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_examples() {
        for value in [0, 1, 127, 128, 255, 2_097_151, i32::MAX, -1] {
            let mut bytes = BytesMut::new();
            encode_varint(value, &mut bytes);
            assert_eq!(decode_varint(&bytes), Ok((value, bytes.len())));
        }
    }

    #[test]
    fn rejects_oversized_values() {
        assert_eq!(
            decode_varint(&[0x80, 0x80, 0x80, 0x80, 0x80]),
            Err(VarIntError::TooLarge)
        );
    }
}
