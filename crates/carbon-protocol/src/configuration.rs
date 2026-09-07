//! Minecraft 26.2 configuration-state packets.

use crate::PacketError;

const SERVERBOUND_FINISH_CONFIGURATION_ID: i32 = 3;

/// Version-pinned registry identifiers and tag relationships generated from
/// the official 26.2 data pack. Every frame is already length-prefixed.
pub const CONFIGURATION_SNAPSHOT: &[u8] = include_bytes!("../assets/configuration-26.2.bin");

pub fn decode_finish_configuration(packet: &[u8]) -> Result<(), PacketError> {
    if packet == [SERVERBOUND_FINISH_CONFIGURATION_ID as u8] {
        Ok(())
    } else {
        Err(PacketError::UnexpectedPacket(
            packet.first().copied().map_or(-1, i32::from),
        ))
    }
}
