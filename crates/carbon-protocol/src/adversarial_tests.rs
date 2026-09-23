//! Deterministic malformed-input regression coverage, not a substitute for fuzzing.
use crate::*;

fn exercise(packet: &[u8]) {
    let _ = decode_varint(packet);
    let _ = decode_encryption_response(packet);
    let _ = decode_handshake(packet);
    let _ = decode_login_start(packet);
    let _ = decode_login_acknowledged(packet);
    let _ = decode_select_known_packs(packet);
    let _ = decode_finish_configuration(packet);
    let _ = decode_keep_alive(packet);
    let _ = decode_attack(packet);
    let _ = decode_chat_command(packet);
    let _ = decode_chat_message(packet);
    let _ = decode_client_command(packet);
    let _ = decode_container_click(packet);
    let _ = decode_container_close(packet);
    let _ = decode_interact_entity(packet);
    let _ = decode_player_action(packet);
    let _ = decode_player_command(packet);
    let _ = decode_player_movement(packet);
    let _ = decode_player_rotation(packet);
    let _ = decode_set_carried_item(packet);
    let _ = decode_swing(packet);
    let _ = decode_use_item(packet);
    let _ = decode_use_item_on(packet);
}

#[test]
fn decoders_survive_short_inputs_and_boundary_payloads() {
    exercise(&[]);
    for first in 0..=255 {
        exercise(&[first]);
        for second in 0..=255 {
            exercise(&[first, second]);
        }
    }
    // Target every one-byte packet ID, so invalid random IDs cannot hide a decoder.
    let mut random = 0x9e3779b9_u32;
    for id in 0..=127 {
        for length in [3, 8, 9, 16, 25, 33, 128, 256, 1024, 4096] {
            for fill in [0, 0x7f, 0x80, 0xff] {
                let mut packet = vec![fill; length];
                packet[0] = id;
                exercise(&packet);
                for byte in &mut packet[1..] {
                    random ^= random << 13;
                    random ^= random >> 17;
                    random ^= random << 5;
                    *byte = random as u8;
                }
                exercise(&packet);
            }
        }
    }
    for fill in [0, 0xff] {
        let mut packet = vec![fill; MAX_PACKET_SIZE];
        for id in 0..=127 {
            packet[0] = id;
            exercise(&packet);
        }
    }
}

#[test]
fn keepalive_requires_exact_signed_long_payload() {
    for id in [i64::MIN, -1, 0, i64::MAX] {
        let mut packet = vec![28];
        packet.extend(id.to_be_bytes());
        assert_eq!(decode_keep_alive(&packet).unwrap(), Some(id));
        assert!(decode_keep_alive(&packet[..8]).is_err());
        packet.push(0);
        assert!(decode_keep_alive(&packet).is_err());
    }
    assert_eq!(decode_keep_alive(&[27]).unwrap(), None);
}
