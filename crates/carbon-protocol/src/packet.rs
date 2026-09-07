use bytes::{BufMut, BytesMut};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};

const CLIENTBOUND_LOGIN_FINISHED_ID: i32 = 2;
const CLIENTBOUND_CONFIGURATION_CUSTOM_PAYLOAD_ID: i32 = 1;
const CLIENTBOUND_UPDATE_ENABLED_FEATURES_ID: i32 = 12;
const CLIENTBOUND_SELECT_KNOWN_PACKS_ID: i32 = 14;
const SERVERBOUND_LOGIN_ACKNOWLEDGED_ID: i32 = 3;
const SERVERBOUND_SELECT_KNOWN_PACKS_ID: i32 = 7;
const MAX_KNOWN_PACKS: usize = 64;

use crate::{decode_varint, encode_varint, VarIntError, MAX_PACKET_SIZE};

/// Protocol phase assigned to a connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
    Configuration,
    Play,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NextState {
    Status,
    Login,
    Transfer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Handshake {
    pub protocol_version: i32,
    pub server_address: String,
    pub server_port: u16,
    pub next_state: NextState,
}

/// First packet sent by a 26.2 client after entering the login state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginStart {
    pub username: String,
    pub player_id: [u8; 16],
}

/// Data-pack identity negotiated before registry synchronization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnownPack {
    pub namespace: String,
    pub id: String,
    pub version: String,
}

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid VarInt: {0}")]
    VarInt(#[from] VarIntError),
    #[error("packet length {0} is invalid")]
    InvalidLength(i32),
    #[error("packet {actual} exceeds limit of {limit} bytes")]
    TooLarge { actual: usize, limit: usize },
    #[error("packet ended unexpectedly")]
    UnexpectedEnd,
    #[error("string is not valid UTF-8")]
    InvalidUtf8,
    #[error("string length {0} is invalid")]
    InvalidStringLength(i32),
    #[error("unexpected packet id {0}")]
    UnexpectedPacket(i32),
    #[error("unsupported handshake next state {0}")]
    UnsupportedNextState(i32),
    #[error("trailing bytes in packet")]
    TrailingBytes,
    #[error("login username cannot be empty")]
    EmptyUsername,
}

/// Reads one length-prefixed Minecraft packet body.
pub async fn read_frame<R>(reader: &mut R) -> Result<Vec<u8>, PacketError>
where
    R: AsyncRead + Unpin,
{
    let length = read_varint_async(reader).await?;
    if length < 0 {
        return Err(PacketError::InvalidLength(length));
    }
    let length = usize::try_from(length).map_err(|_| PacketError::InvalidLength(length))?;
    if length > MAX_PACKET_SIZE {
        return Err(PacketError::TooLarge {
            actual: length,
            limit: MAX_PACKET_SIZE,
        });
    }
    let mut payload = vec![0; length];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

/// Frames a packet ID and payload using Minecraft's outer length prefix.
#[must_use]
pub fn frame_packet(packet_id: i32, payload: &[u8]) -> Vec<u8> {
    let mut body = BytesMut::new();
    encode_varint(packet_id, &mut body);
    body.extend_from_slice(payload);

    let mut framed = BytesMut::new();
    encode_varint(
        i32::try_from(body.len()).expect("packet body fits in i32"),
        &mut framed,
    );
    framed.extend_from_slice(&body);
    framed.to_vec()
}

pub fn decode_handshake(packet: &[u8]) -> Result<Handshake, PacketError> {
    let mut cursor = Cursor::new(packet);
    let packet_id = cursor.varint()?;
    if packet_id != 0 {
        return Err(PacketError::UnexpectedPacket(packet_id));
    }
    let protocol_version = cursor.varint()?;
    let server_address = cursor.string(255)?;
    let server_port = cursor.u16()?;
    let state = cursor.varint()?;
    let next_state = match state {
        1 => NextState::Status,
        2 => NextState::Login,
        3 => NextState::Transfer,
        other => return Err(PacketError::UnsupportedNextState(other)),
    };
    if !cursor.is_empty() {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Handshake {
        protocol_version,
        server_address,
        server_port,
        next_state,
    })
}

/// Decodes the serverbound Login Start packet used by Minecraft Java 26.2.
pub fn decode_login_start(packet: &[u8]) -> Result<LoginStart, PacketError> {
    let mut cursor = Cursor::new(packet);
    let packet_id = cursor.varint()?;
    if packet_id != 0 {
        return Err(PacketError::UnexpectedPacket(packet_id));
    }
    let username = cursor.string(16)?;
    if username.is_empty() {
        return Err(PacketError::EmptyUsername);
    }
    let player_id = cursor.bytes::<16>()?;
    if !cursor.is_empty() {
        return Err(PacketError::TrailingBytes);
    }
    Ok(LoginStart {
        username,
        player_id,
    })
}
/// Encodes a framed 26.2 Login Finished packet for an offline-mode connection.
#[must_use]
pub fn encode_login_finished(
    username: &str,
    profile_id: [u8; 16],
    session_id: [u8; 16],
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    // ByteBufCodecs.GAME_PROFILE: UUID, player name, property map.
    payload.extend_from_slice(&profile_id);
    payload.extend_from_slice(&encode_string(username));
    encode_varint(0, &mut payload); // Empty profile-property map in offline mode.
    payload.extend_from_slice(&session_id);
    frame_packet(CLIENTBOUND_LOGIN_FINISHED_ID, &payload)
}

/// Validates the terminal empty packet that moves login into configuration.
pub fn decode_login_acknowledged(packet: &[u8]) -> Result<(), PacketError> {
    let mut cursor = Cursor::new(packet);
    let packet_id = cursor.varint()?;
    if packet_id != SERVERBOUND_LOGIN_ACKNOWLEDGED_ID {
        return Err(PacketError::UnexpectedPacket(packet_id));
    }
    if !cursor.is_empty() {
        return Err(PacketError::TrailingBytes);
    }
    Ok(())
}

/// Encodes Carbon's brand as the first clientbound configuration packet.
#[must_use]
pub fn encode_configuration_brand(brand: &str) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.extend_from_slice(&encode_string("minecraft:brand"));
    payload.extend_from_slice(&encode_string(brand));
    frame_packet(CLIENTBOUND_CONFIGURATION_CUSTOM_PAYLOAD_ID, &payload)
}

/// Encodes the feature flags enabled by the current world configuration.
#[must_use]
pub fn encode_update_enabled_features(features: &[&str]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(
        i32::try_from(features.len()).expect("feature count fits in i32"),
        &mut payload,
    );
    for feature in features {
        payload.extend_from_slice(&encode_string(feature));
    }
    frame_packet(CLIENTBOUND_UPDATE_ENABLED_FEATURES_ID, &payload)
}

/// Encodes the server's known data packs for registry negotiation.
#[must_use]
pub fn encode_select_known_packs(packs: &[KnownPack]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(
        i32::try_from(packs.len()).expect("known-pack count fits in i32"),
        &mut payload,
    );
    for pack in packs {
        payload.extend_from_slice(&encode_string(&pack.namespace));
        payload.extend_from_slice(&encode_string(&pack.id));
        payload.extend_from_slice(&encode_string(&pack.version));
    }
    frame_packet(CLIENTBOUND_SELECT_KNOWN_PACKS_ID, &payload)
}

/// Decodes and validates the client's known-data-pack selection response.
pub fn decode_select_known_packs(packet: &[u8]) -> Result<Vec<KnownPack>, PacketError> {
    let mut cursor = Cursor::new(packet);
    let packet_id = cursor.varint()?;
    if packet_id != SERVERBOUND_SELECT_KNOWN_PACKS_ID {
        return Err(PacketError::UnexpectedPacket(packet_id));
    }
    let count = cursor.varint()?;
    if count < 0 {
        return Err(PacketError::InvalidLength(count));
    }
    let count = usize::try_from(count).map_err(|_| PacketError::InvalidLength(count))?;
    if count > MAX_KNOWN_PACKS {
        return Err(PacketError::TooLarge {
            actual: count,
            limit: MAX_KNOWN_PACKS,
        });
    }

    let mut packs = Vec::with_capacity(count);
    for _ in 0..count {
        packs.push(KnownPack {
            namespace: cursor.string(32_767)?,
            id: cursor.string(32_767)?,
            version: cursor.string(32_767)?,
        });
    }
    if !cursor.is_empty() {
        return Err(PacketError::TrailingBytes);
    }
    Ok(packs)
}

async fn read_varint_async<R>(reader: &mut R) -> Result<i32, PacketError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = [0_u8; 5];
    for index in 0..5 {
        bytes[index] = reader.read_u8().await?;
        if bytes[index] & 0x80 == 0 {
            return Ok(decode_varint(&bytes[..=index])?.0);
        }
    }
    Err(PacketError::VarInt(VarIntError::TooLarge))
}

struct Cursor<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn varint(&mut self) -> Result<i32, PacketError> {
        let (value, consumed) = decode_varint(&self.data[self.offset..])?;
        self.offset += consumed;
        Ok(value)
    }

    fn string(&mut self, max_chars: usize) -> Result<String, PacketError> {
        let length = self.varint()?;
        if length < 0 {
            return Err(PacketError::InvalidStringLength(length));
        }
        let length =
            usize::try_from(length).map_err(|_| PacketError::InvalidStringLength(length))?;
        let end = self
            .offset
            .checked_add(length)
            .filter(|end| *end <= self.data.len())
            .ok_or(PacketError::UnexpectedEnd)?;
        let value = std::str::from_utf8(&self.data[self.offset..end])
            .map_err(|_| PacketError::InvalidUtf8)?;
        if value.chars().count() > max_chars {
            return Err(PacketError::TooLarge {
                actual: value.chars().count(),
                limit: max_chars,
            });
        }
        self.offset = end;
        Ok(value.to_owned())
    }

    fn bytes<const LENGTH: usize>(&mut self) -> Result<[u8; LENGTH], PacketError> {
        let end = self
            .offset
            .checked_add(LENGTH)
            .ok_or(PacketError::UnexpectedEnd)?;
        let bytes = self
            .data
            .get(self.offset..end)
            .ok_or(PacketError::UnexpectedEnd)?
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?;
        self.offset = end;
        Ok(bytes)
    }

    fn u16(&mut self) -> Result<u16, PacketError> {
        let end = self
            .offset
            .checked_add(2)
            .ok_or(PacketError::UnexpectedEnd)?;
        let bytes: [u8; 2] = self
            .data
            .get(self.offset..end)
            .ok_or(PacketError::UnexpectedEnd)?
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?;
        self.offset = end;
        Ok(u16::from_be_bytes(bytes))
    }

    fn is_empty(&self) -> bool {
        self.offset == self.data.len()
    }
}

/// Encodes a protocol string payload without a packet ID.
#[must_use]
pub fn encode_string(value: &str) -> Vec<u8> {
    let mut bytes = BytesMut::new();
    encode_varint(
        i32::try_from(value.len()).expect("protocol string fits in i32"),
        &mut bytes,
    );
    bytes.put_slice(value.as_bytes());
    bytes.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_a_status_handshake() {
        let mut packet = BytesMut::new();
        encode_varint(0, &mut packet);
        encode_varint(769, &mut packet);
        encode_varint(9, &mut packet);
        packet.extend_from_slice(b"localhost");
        packet.put_u16(25565);
        encode_varint(1, &mut packet);

        let result = decode_handshake(&packet).expect("valid handshake");
        assert_eq!(result.protocol_version, 769);
        assert_eq!(result.server_address, "localhost");
        assert_eq!(result.next_state, NextState::Status);
    }

    #[test]
    fn decodes_a_26_2_login_start() {
        let player_id = [0x2a; 16];
        let mut packet = BytesMut::new();
        encode_varint(0, &mut packet);
        packet.extend_from_slice(&encode_string("CarbonPlayer"));
        packet.extend_from_slice(&player_id);

        let result = decode_login_start(&packet).expect("valid login start");
        assert_eq!(result.username, "CarbonPlayer");
        assert_eq!(result.player_id, player_id);
    }

    #[test]
    fn login_start_rejects_an_empty_username() {
        let mut packet = BytesMut::new();
        encode_varint(0, &mut packet);
        encode_varint(0, &mut packet);
        packet.extend_from_slice(&[0; 16]);
        assert!(matches!(
            decode_login_start(&packet),
            Err(PacketError::EmptyUsername)
        ));
    }

    #[test]
    fn frames_packet_id_and_body() {
        assert_eq!(frame_packet(0, &[1, 2]), vec![3, 0, 1, 2]);
    }
    #[test]
    fn encodes_a_26_2_login_finished_packet() {
        let login = LoginStart {
            username: "CarbonPlayer".into(),
            player_id: [0x11; 16],
        };
        let session_id = [0x22; 16];
        let framed = encode_login_finished(&login.username, login.player_id, session_id);
        let (frame_length, outer_bytes) = decode_varint(&framed).expect("frame length");
        assert_eq!(
            usize::try_from(frame_length).unwrap(),
            framed.len() - outer_bytes
        );
        let body = &framed[outer_bytes..];
        let (packet_id, id_bytes) = decode_varint(body).expect("packet id");
        assert_eq!(packet_id, CLIENTBOUND_LOGIN_FINISHED_ID);
        assert_eq!(&body[id_bytes..id_bytes + 16], &[0x11; 16]);
        let name_offset = id_bytes + 16;
        assert_eq!(body[name_offset], 12);
        assert_eq!(&body[name_offset + 1..name_offset + 13], b"CarbonPlayer");
        assert_eq!(body[name_offset + 13], 0);
        assert_eq!(&body[name_offset + 14..], &[0x22; 16]);
    }

    #[test]
    fn validates_login_acknowledgement() {
        assert!(decode_login_acknowledged(&[3]).is_ok());
        assert!(matches!(
            decode_login_acknowledged(&[3, 0]),
            Err(PacketError::TrailingBytes)
        ));
        assert!(matches!(
            decode_login_acknowledged(&[2]),
            Err(PacketError::UnexpectedPacket(2))
        ));
    }

    #[test]
    fn encodes_configuration_brand() {
        let framed = encode_configuration_brand("Carbon");
        assert_eq!(
            framed,
            [
                vec![24, CLIENTBOUND_CONFIGURATION_CUSTOM_PAYLOAD_ID as u8, 15],
                b"minecraft:brand".to_vec(),
                vec![6],
                b"Carbon".to_vec(),
            ]
            .concat()
        );
    }

    #[test]
    fn encodes_vanilla_feature_flag() {
        let framed = encode_update_enabled_features(&["minecraft:vanilla"]);
        assert_eq!(framed[0], 20);
        assert_eq!(framed[1], CLIENTBOUND_UPDATE_ENABLED_FEATURES_ID as u8);
        assert_eq!(framed[2], 1);
        assert_eq!(framed[3], 17);
        assert_eq!(&framed[4..], b"minecraft:vanilla");
    }

    #[test]
    fn round_trips_known_pack_payload_layout() {
        let pack = KnownPack {
            namespace: "minecraft".into(),
            id: "core".into(),
            version: "26.2".into(),
        };
        let framed = encode_select_known_packs(std::slice::from_ref(&pack));
        let (_, outer_bytes) = decode_varint(&framed).expect("frame length");
        let body = &framed[outer_bytes..];
        let (packet_id, id_bytes) = decode_varint(body).expect("packet id");
        assert_eq!(packet_id, CLIENTBOUND_SELECT_KNOWN_PACKS_ID);

        let mut response = BytesMut::new();
        encode_varint(SERVERBOUND_SELECT_KNOWN_PACKS_ID, &mut response);
        response.extend_from_slice(&body[id_bytes..]);
        assert_eq!(decode_select_known_packs(&response).unwrap(), vec![pack]);
    }

    #[test]
    fn rejects_too_many_known_packs() {
        let mut response = BytesMut::new();
        encode_varint(SERVERBOUND_SELECT_KNOWN_PACKS_ID, &mut response);
        encode_varint(65, &mut response);
        assert!(matches!(
            decode_select_known_packs(&response),
            Err(PacketError::TooLarge {
                actual: 65,
                limit: 64
            })
        ));
    }
}
