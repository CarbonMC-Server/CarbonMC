//! Bounded Minecraft compression envelopes. Each packet is its own zlib stream.
use carbon_protocol::{decode_varint, frame_packet, MAX_PACKET_SIZE};
use flate2::{write::ZlibEncoder, Compression, Decompress, FlushDecompress, Status};
use std::io::{self, Write};

pub(super) const THRESHOLD: usize = 256;
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

// Validate and charge the declared decoded size before allocating/inflating.
pub(super) fn decoded_length(packet: &[u8]) -> io::Result<usize> {
    let (declared, prefix) =
        decode_varint(packet).map_err(|_| invalid("invalid compression length"))?;
    if declared < 0 || declared as usize > MAX_PACKET_SIZE {
        return Err(invalid("decompressed packet exceeds size limit"));
    }
    if declared == 0 {
        let length = packet.len() - prefix;
        if length == 0 || length >= THRESHOLD {
            return Err(invalid("invalid uncompressed packet size"));
        }
        Ok(length)
    } else if (declared as usize) < THRESHOLD {
        Err(invalid("compressed packet below threshold"))
    } else {
        Ok(declared as usize)
    }
}

pub(super) fn decode(packet: Vec<u8>) -> io::Result<Vec<u8>> {
    let expected = decoded_length(&packet)?;
    let (declared, prefix) =
        decode_varint(&packet).map_err(|_| invalid("invalid compression length"))?;
    if declared == 0 {
        return Ok(packet[prefix..].to_vec());
    }
    // One sentinel byte detects expansion past the declared length. Never grow
    // based on the compressed stream's contents.
    let mut output = vec![0; expected + 1];
    let mut inflater = Decompress::new(true);
    let status = inflater
        .decompress(&packet[prefix..], &mut output, FlushDecompress::Finish)
        .map_err(|_| invalid("invalid zlib packet"))?;
    if status != Status::StreamEnd
        || inflater.total_out() != expected as u64
        || inflater.total_in() != (packet.len() - prefix) as u64
    {
        return Err(invalid(
            "incomplete, trailing, or incorrectly sized zlib packet",
        ));
    }
    output.truncate(expected);
    Ok(output)
}

// Existing server encoders supply complete ordinary frames; some configuration
// writes concatenate several frames. Re-envelope each one independently.
pub(super) fn encode_frames(mut frames: &[u8]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    while !frames.is_empty() {
        let (length, prefix) =
            decode_varint(frames).map_err(|_| invalid("invalid outgoing frame"))?;
        if length <= 0 || length as usize > MAX_PACKET_SIZE {
            return Err(invalid("invalid outgoing packet size"));
        }
        let end = prefix + length as usize;
        let body = frames
            .get(prefix..end)
            .ok_or_else(|| invalid("incomplete outgoing frame"))?;
        let envelope = if body.len() < THRESHOLD {
            frame_packet(0, body)
        } else {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(body)?;
            frame_packet(length, &encoder.finish()?)
        };
        let (wire_length, _) =
            decode_varint(&envelope).map_err(|_| invalid("invalid outgoing envelope"))?;
        if wire_length as usize > MAX_PACKET_SIZE {
            return Err(invalid("compressed outgoing frame exceeds wire limit"));
        }
        output.extend(envelope);
        frames = &frames[end..];
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn envelope(bytes: &[u8]) -> Vec<u8> {
        let (_, prefix) = decode_varint(bytes).unwrap();
        bytes[prefix..].to_vec()
    }
    fn compressed(declared: i32, payload: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(payload).unwrap();
        envelope(&frame_packet(declared, &encoder.finish().unwrap()))
    }
    #[test]
    fn decodes_independent_python_zlib_fixture() {
        // Python zlib.compress(bytes([7]) + bytes([42]) * 255).
        let packet = vec![
            0x80, 0x02, 0x78, 0x9c, 0x63, 0xd7, 0x1a, 0xd9, 0x00, 0x00, 0xf4, 0x2c, 0x29, 0xde,
        ];
        assert_eq!(decode(packet).unwrap(), [vec![7], vec![42; 255]].concat());
    }

    #[test]
    fn threshold_boundaries_and_maximum_packet_round_trip() {
        for length in [1, THRESHOLD - 1, THRESHOLD, THRESHOLD + 1, MAX_PACKET_SIZE] {
            let payload = vec![42; length];
            let framed = frame_packet(42, &payload[1..]);
            let encoded = encode_frames(&framed).unwrap();
            let body = envelope(&encoded);
            assert_eq!(decoded_length(&body).unwrap(), length);
            assert_eq!(decode(body).unwrap(), payload);
        }
    }
    #[test]
    fn rejects_bombs_size_lies_trailing_streams_and_corruption() {
        assert!(decoded_length(&envelope(&frame_packet(i32::MAX, &[]))).is_err());
        assert!(decoded_length(&envelope(&frame_packet(-1, &[]))).is_err());
        assert!(decoded_length(&envelope(&frame_packet(0, &vec![0; THRESHOLD]))).is_err());
        assert!(decode(compressed(255, &[0; 255])).is_err());
        for declared in [256, 512, 1024] {
            assert!(decode(compressed(declared, &[0; 513])).is_err());
        }
        let valid = compressed(512, &[0; 512]);
        for end in 0..valid.len() {
            assert!(decode(valid[..end].to_vec()).is_err());
        }
        let mut trailing = valid.clone();
        trailing.push(0);
        assert!(decode(trailing).is_err());
        let mut corrupted = valid;
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(decode(corrupted).is_err());
    }
    #[test]
    fn concatenated_configuration_frames_remain_separate() {
        let mut input = frame_packet(1, &[9]);
        input.extend(frame_packet(2, &[8; 512]));
        let encoded = encode_frames(&input).unwrap();
        let mut remaining = encoded.as_slice();
        for expected in [vec![1, 9], [vec![2], vec![8; 512]].concat()] {
            let (length, prefix) = decode_varint(remaining).unwrap();
            let end = prefix + length as usize;
            assert_eq!(decode(remaining[prefix..end].to_vec()).unwrap(), expected);
            remaining = &remaining[end..];
        }
        assert!(remaining.is_empty());
        assert!(encode_frames(&input[..input.len() - 1]).is_err());
    }
}
