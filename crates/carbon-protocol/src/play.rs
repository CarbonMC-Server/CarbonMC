//! Minimal Minecraft 26.2 play-state encoders used by Carbon's first join path.

use bytes::{BufMut, BytesMut};

use crate::{decode_varint, encode_string, encode_varint, frame_packet, PacketError};

const CLIENTBOUND_ADD_ENTITY_ID: i32 = 1;
const CLIENTBOUND_ANIMATE_ID: i32 = 2;
const CLIENTBOUND_CHANGE_DIFFICULTY_ID: i32 = 10;
const CLIENTBOUND_CONTAINER_SET_SLOT_ID: i32 = 20;
const CLIENTBOUND_CONTAINER_SET_DATA_ID: i32 = 19;
const CLIENTBOUND_BLOCK_CHANGED_ACK_ID: i32 = 4;
const CLIENTBOUND_BLOCK_UPDATE_ID: i32 = 8;
const CLIENTBOUND_CHUNK_BATCH_FINISHED_ID: i32 = 11;
const CLIENTBOUND_CHUNK_BATCH_START_ID: i32 = 12;
const CLIENTBOUND_COMMANDS_ID: i32 = 16;
const CLIENTBOUND_ENTITY_EVENT_ID: i32 = 34;
const CLIENTBOUND_DISCONNECT_ID: i32 = 32;
const CLIENTBOUND_FORGET_LEVEL_CHUNK_ID: i32 = 37;
const CLIENTBOUND_GAME_EVENT_ID: i32 = 38;
const CLIENTBOUND_KEEP_ALIVE_ID: i32 = 44;
const CLIENTBOUND_LOGIN_ID: i32 = 49;
const CLIENTBOUND_OPEN_SCREEN_ID: i32 = 59;
const CLIENTBOUND_PLAYER_ABILITIES_ID: i32 = 64;
const CLIENTBOUND_PLAYER_INFO_REMOVE_ID: i32 = 69;
const CLIENTBOUND_PLAYER_INFO_UPDATE_ID: i32 = 70;
const CLIENTBOUND_PLAYER_POSITION_ID: i32 = 72;
const CLIENTBOUND_MOVE_ENTITY_POS_ROT_ID: i32 = 54;
const CLIENTBOUND_REMOVE_ENTITIES_ID: i32 = 77;
const CLIENTBOUND_REMOVE_MOB_EFFECT_ID: i32 = 78;
const CLIENTBOUND_RESPAWN_ID: i32 = 82;
const CLIENTBOUND_ROTATE_HEAD_ID: i32 = 83;
const CLIENTBOUND_SET_CHUNK_CACHE_CENTER_ID: i32 = 94;
const CLIENTBOUND_SET_DEFAULT_SPAWN_POSITION_ID: i32 = 97;
const CLIENTBOUND_SET_ENTITY_DATA_ID: i32 = 99;
const CLIENTBOUND_SET_ENTITY_MOTION_ID: i32 = 101;
const CLIENTBOUND_SET_EQUIPMENT_ID: i32 = 102;
const CLIENTBOUND_SET_HEALTH_ID: i32 = 104;
const CLIENTBOUND_SET_CURSOR_ITEM_ID: i32 = 96;
const CLIENTBOUND_SET_PLAYER_INVENTORY_ID: i32 = 108;
const CLIENTBOUND_SYSTEM_CHAT_ID: i32 = 121;
const CLIENTBOUND_UPDATE_MOB_EFFECT_ID: i32 = 132;
const SERVERBOUND_CHAT_COMMAND_ID: i32 = 7;
const SERVERBOUND_CHAT_ID: i32 = 9;
const SERVERBOUND_CONTAINER_CLICK_ID: i32 = 18;
const SERVERBOUND_CONTAINER_CLOSE_ID: i32 = 19;
const SERVERBOUND_INTERACT_ID: i32 = 26;
const SERVERBOUND_MOVE_PLAYER_POS_ID: i32 = 30;
const SERVERBOUND_MOVE_PLAYER_POS_ROT_ID: i32 = 31;
const SERVERBOUND_MOVE_PLAYER_ROT_ID: i32 = 32;
const SERVERBOUND_PLAYER_ACTION_ID: i32 = 41;
const SERVERBOUND_PLAYER_COMMAND_ID: i32 = 42;
const SERVERBOUND_SET_CARRIED_ITEM_ID: i32 = 53;
const SERVERBOUND_SWING_ID: i32 = 63;
const SERVERBOUND_USE_ITEM_ON_ID: i32 = 66;
const SERVERBOUND_USE_ITEM_ID: i32 = 67;
const SERVERBOUND_ATTACK_ID: i32 = 1;
const SERVERBOUND_CLIENT_COMMAND_ID: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerMovement {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub yaw: Option<f32>,
    pub pitch: Option<f32>,
    pub on_ground: bool,
    pub horizontal_collision: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerRotation {
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerAction {
    pub action: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub sequence: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UseItemOn {
    pub hand: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: i32,
    pub sequence: i32,
}

/// Serverbound 26.2 container-click header. Client-predicted hashed stacks are
/// validated structurally but deliberately not trusted as inventory state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContainerClick {
    pub container_id: i32,
    pub state_id: i32,
    pub slot: i16,
    pub button: i8,
    pub input: i32,
}

pub const SPAWN_X: f64 = -7.5;
pub const SPAWN_Y: f64 = 65.0;
pub const SPAWN_Z: f64 = -8.5;
pub const STARTER_CHUNK_COUNT: i32 = 289;

#[must_use]
pub fn encode_play_login(
    entity_id: i32,
    max_players: u32,
    view_distance: u8,
    seed: i64,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_i32(entity_id);
    payload.put_u8(0); // Not hardcore.

    let levels = [
        "minecraft:overworld",
        "minecraft:the_nether",
        "minecraft:the_end",
    ];
    encode_varint(levels.len() as i32, &mut payload);
    for level in levels {
        payload.extend_from_slice(&encode_string(level));
    }

    encode_varint(i32::try_from(max_players).unwrap_or(i32::MAX), &mut payload);
    encode_varint(i32::from(view_distance), &mut payload);
    encode_varint(i32::from(view_distance), &mut payload); // Simulation distance.
    payload.put_u8(0); // Full debug information.
    payload.put_u8(1); // Show the death screen.
    payload.put_u8(0); // Normal recipe crafting.

    encode_varint(0, &mut payload); // minecraft:overworld dimension-type holder.
    payload.extend_from_slice(&encode_string("minecraft:overworld"));
    payload.put_i64(seed);
    payload.put_i8(0); // Survival.
    payload.put_i8(-1); // No previous game mode.
    payload.put_u8(0); // Not a debug world.
    payload.put_u8(0); // Procedurally generated prototype world.
    payload.put_u8(0); // No last death position.
    encode_varint(0, &mut payload); // Portal cooldown.
    encode_varint(63, &mut payload); // Sea level.
    payload.put_u8(0); // Offline mode.
    payload.put_u8(0); // Secure chat is not enforced.
    frame_packet(CLIENTBOUND_LOGIN_ID, &payload)
}

#[must_use]
pub fn encode_change_difficulty() -> Vec<u8> {
    frame_packet(CLIENTBOUND_CHANGE_DIFFICULTY_ID, &[1, 0])
}

#[must_use]
pub fn encode_animate(entity_id: i32, animation: u8) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    payload.put_u8(animation);
    frame_packet(CLIENTBOUND_ANIMATE_ID, &payload)
}

#[must_use]
pub fn encode_chunk_batch_start() -> Vec<u8> {
    frame_packet(CLIENTBOUND_CHUNK_BATCH_START_ID, &[])
}

#[must_use]
pub fn encode_chunk_batch_finished(chunk_count: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(chunk_count, &mut payload);
    frame_packet(CLIENTBOUND_CHUNK_BATCH_FINISHED_ID, &payload)
}

#[must_use]
pub fn encode_commands(commands: &[&str]) -> Vec<u8> {
    let has_say = commands.contains(&"say");
    let extra_nodes = usize::from(has_say);
    let mut payload = BytesMut::new();
    encode_varint(
        i32::try_from(commands.len() + 1 + extra_nodes).unwrap_or(i32::MAX),
        &mut payload,
    );
    payload.put_u8(0); // Root node.
    encode_varint(
        i32::try_from(commands.len()).unwrap_or(i32::MAX),
        &mut payload,
    );
    for index in 1..=commands.len() {
        encode_varint(i32::try_from(index).unwrap_or(i32::MAX), &mut payload);
    }
    for command in commands {
        if *command == "say" {
            payload.put_u8(0x01); // Literal with a message child.
            encode_varint(1, &mut payload);
            encode_varint(
                i32::try_from(commands.len() + 1).unwrap_or(i32::MAX),
                &mut payload,
            );
        } else {
            payload.put_u8(0x05); // Executable literal node.
            encode_varint(0, &mut payload); // No children.
        }
        payload.extend_from_slice(&encode_string(command));
    }
    if has_say {
        payload.put_u8(0x06); // Executable argument node.
        encode_varint(0, &mut payload);
        payload.extend_from_slice(&encode_string("message"));
        encode_varint(20, &mut payload); // minecraft:message, verified for 26.2.
    }
    encode_varint(0, &mut payload); // Root node index.
    frame_packet(CLIENTBOUND_COMMANDS_ID, &payload)
}

#[must_use]
pub fn encode_level_chunks_load_start() -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_u8(13); // LEVEL_CHUNKS_LOAD_START.
    payload.put_f32(0.0);
    frame_packet(CLIENTBOUND_GAME_EVENT_ID, &payload)
}

#[must_use]
pub fn encode_player_abilities() -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_u8(0);
    payload.put_f32(0.05);
    payload.put_f32(0.1);
    frame_packet(CLIENTBOUND_PLAYER_ABILITIES_ID, &payload)
}

#[must_use]
pub fn encode_player_info(username: &str, profile_id: [u8; 16]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_u8(0xff); // Fixed-width mask for all eight initialization actions.
    encode_varint(1, &mut payload);
    payload.extend_from_slice(&profile_id);
    payload.extend_from_slice(&encode_string(username));
    encode_varint(0, &mut payload); // No profile properties.
    payload.put_u8(0); // No signed chat session.
    encode_varint(0, &mut payload); // Survival.
    payload.put_u8(1); // Listed in the tab list.
    encode_varint(0, &mut payload); // Initial latency.
    payload.put_u8(0); // No display-name override.
    encode_varint(0, &mut payload); // List order.
    payload.put_u8(1); // Show the player's hat layer.
    frame_packet(CLIENTBOUND_PLAYER_INFO_UPDATE_ID, &payload)
}

#[must_use]
pub fn encode_player_info_remove(profile_ids: &[[u8; 16]]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(
        i32::try_from(profile_ids.len()).unwrap_or(i32::MAX),
        &mut payload,
    );
    for profile_id in profile_ids {
        payload.extend_from_slice(profile_id);
    }
    frame_packet(CLIENTBOUND_PLAYER_INFO_REMOVE_ID, &payload)
}

#[must_use]
pub fn encode_player_position(teleport_id: i32) -> Vec<u8> {
    encode_player_position_at(teleport_id, [SPAWN_X, SPAWN_Y, SPAWN_Z])
}

#[must_use]
pub fn encode_player_position_at(teleport_id: i32, position: [f64; 3]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(teleport_id, &mut payload);
    for coordinate in position {
        payload.put_f64(coordinate);
    }
    payload.put_f64(0.0); // Delta X.
    payload.put_f64(0.0); // Delta Y.
    payload.put_f64(0.0); // Delta Z.
    payload.put_f32(0.0); // Yaw.
    payload.put_f32(0.0); // Pitch.
    payload.put_i32(0); // No relative components.
    frame_packet(CLIENTBOUND_PLAYER_POSITION_ID, &payload)
}

#[must_use]
pub fn encode_chunk_cache_center(x: i32, z: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(x, &mut payload);
    encode_varint(z, &mut payload);
    frame_packet(CLIENTBOUND_SET_CHUNK_CACHE_CENTER_ID, &payload)
}

#[must_use]
pub fn encode_forget_level_chunk(x: i32, z: i32) -> Vec<u8> {
    let packed = (i64::from(z) << 32) | (i64::from(x) & 0xffff_ffff);
    frame_packet(CLIENTBOUND_FORGET_LEVEL_CHUNK_ID, &packed.to_be_bytes())
}

#[must_use]
pub fn encode_default_spawn_position() -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.extend_from_slice(&encode_string("minecraft:overworld"));
    payload.put_i64(80); // Packed block position (0, 80, 0).
    payload.put_f32(0.0);
    payload.put_f32(0.0);
    frame_packet(CLIENTBOUND_SET_DEFAULT_SPAWN_POSITION_ID, &payload)
}

#[must_use]
pub fn encode_keep_alive(id: i64) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_i64(id);
    frame_packet(CLIENTBOUND_KEEP_ALIVE_ID, &payload)
}

#[must_use]
pub fn encode_block_update(x: i32, y: i32, z: i32, state_id: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_i64(pack_block_position(x, y, z));
    encode_varint(state_id, &mut payload);
    frame_packet(CLIENTBOUND_BLOCK_UPDATE_ID, &payload)
}

#[must_use]
pub fn encode_block_changed_ack(sequence: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(sequence, &mut payload);
    frame_packet(CLIENTBOUND_BLOCK_CHANGED_ACK_ID, &payload)
}

#[must_use]
pub fn encode_set_health(health: f32, food: i32, saturation: f32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_f32(health);
    encode_varint(food, &mut payload);
    payload.put_f32(saturation);
    frame_packet(CLIENTBOUND_SET_HEALTH_ID, &payload)
}

#[must_use]
pub fn encode_respawn(seed: i64) -> Vec<u8> {
    encode_dimension_respawn(seed, 0, "minecraft:overworld", 63)
}

#[must_use]
pub fn encode_dimension_respawn(
    seed: i64,
    dimension_type_id: i32,
    dimension_name: &str,
    sea_level: i32,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(dimension_type_id, &mut payload);
    payload.extend_from_slice(&encode_string(dimension_name));
    payload.put_i64(seed);
    payload.put_i8(0);
    payload.put_i8(-1);
    payload.put_u8(0);
    payload.put_u8(0);
    payload.put_u8(0);
    encode_varint(0, &mut payload);
    encode_varint(sea_level, &mut payload);
    payload.put_u8(0);
    frame_packet(CLIENTBOUND_RESPAWN_ID, &payload)
}

/// Updates one slot in the 26.2 player's 36-slot inventory.
#[must_use]
pub fn encode_set_player_inventory(slot: i32, count: u8, item_id: i32, damage: u16) -> Vec<u8> {
    encode_set_player_inventory_with_glint(slot, count, item_id, damage, false)
}

#[must_use]
pub fn encode_set_player_inventory_with_glint(
    slot: i32,
    count: u8,
    item_id: i32,
    damage: u16,
    glint: bool,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(slot, &mut payload);
    if count == 0 {
        encode_varint(0, &mut payload);
    } else {
        encode_item_stack(count, item_id, damage, glint, &mut payload);
    }
    frame_packet(CLIENTBOUND_SET_PLAYER_INVENTORY_ID, &payload)
}

#[must_use]
pub fn encode_set_cursor_item_with_glint(
    count: u8,
    item_id: i32,
    damage: u16,
    glint: bool,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    if count == 0 {
        encode_varint(0, &mut payload);
    } else {
        encode_item_stack(count, item_id, damage, glint, &mut payload);
    }
    frame_packet(CLIENTBOUND_SET_CURSOR_ITEM_ID, &payload)
}

#[must_use]
pub fn encode_container_set_slot_with_glint(
    container_id: i32,
    state_id: i32,
    slot: i16,
    count: u8,
    item_id: i32,
    damage: u16,
    glint: bool,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(container_id, &mut payload);
    encode_varint(state_id, &mut payload);
    payload.put_i16(slot);
    if count == 0 {
        encode_varint(0, &mut payload);
    } else {
        encode_item_stack(count, item_id, damage, glint, &mut payload);
    }
    frame_packet(CLIENTBOUND_CONTAINER_SET_SLOT_ID, &payload)
}

/// Opens a crafting-table menu using the verified 26.2 menu registry ID.
#[must_use]
pub fn encode_open_crafting_screen(container_id: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(container_id, &mut payload);
    encode_varint(12, &mut payload); // minecraft:crafting menu type.
    payload.put_u8(8); // Network NBT string tag.
    let title = b"Crafting";
    payload.put_u16(title.len() as u16);
    payload.extend_from_slice(title);
    frame_packet(CLIENTBOUND_OPEN_SCREEN_ID, &payload)
}

/// Opens a furnace menu using the verified Minecraft 26.2 registry ID.
#[must_use]
pub fn encode_open_furnace_screen(container_id: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(container_id, &mut payload);
    encode_varint(14, &mut payload);
    payload.put_u8(8);
    payload.put_u16(7);
    payload.extend_from_slice(b"Furnace");
    frame_packet(CLIENTBOUND_OPEN_SCREEN_ID, &payload)
}

/// Opens a 27-slot chest using the verified Minecraft 26.2 menu registry ID.
#[must_use]
pub fn encode_open_chest_screen(container_id: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(container_id, &mut payload);
    encode_varint(2, &mut payload);
    payload.put_u8(8);
    payload.put_u16(5);
    payload.extend_from_slice(b"Chest");
    frame_packet(CLIENTBOUND_OPEN_SCREEN_ID, &payload)
}

#[must_use]
pub fn encode_container_set_data(container_id: i32, property: i16, value: i16) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_u8(u8::try_from(container_id).unwrap_or(0));
    payload.put_i16(property);
    payload.put_i16(value);
    frame_packet(CLIENTBOUND_CONTAINER_SET_DATA_ID, &payload)
}

#[must_use]
pub fn encode_item_entity_data(entity_id: i32, count: u8, item_id: i32, damage: u16) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    payload.put_u8(8); // ItemEntity.DATA_ITEM.
    encode_varint(8, &mut payload); // ITEM_STACK serializer.
    encode_item_stack(count, item_id, damage, false, &mut payload);
    payload.put_u8(0xff);
    frame_packet(CLIENTBOUND_SET_ENTITY_DATA_ID, &payload)
}

fn encode_item_stack(count: u8, item_id: i32, damage: u16, glint: bool, payload: &mut BytesMut) {
    encode_varint(i32::from(count), payload);
    encode_varint(item_id, payload);
    encode_varint(i32::from(damage != 0) + i32::from(glint), payload);
    if damage != 0 {
        encode_varint(3, payload); // minecraft:damage component.
        encode_varint(i32::from(damage), payload);
    }
    if glint {
        encode_varint(21, payload); // minecraft:enchantment_glint_override component.
        payload.put_u8(1);
    }
    encode_varint(0, payload); // No removed data components.
}

pub fn decode_chat_command(packet: &[u8]) -> Result<Option<String>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_CHAT_COMMAND_ID {
        return Ok(None);
    }
    let (length, length_bytes) = decode_varint(&packet[id_bytes..])?;
    let length = usize::try_from(length).map_err(|_| PacketError::InvalidLength(length))?;
    if length > 256 {
        return Err(PacketError::InvalidLength(
            i32::try_from(length).unwrap_or(i32::MAX),
        ));
    }
    let start = id_bytes + length_bytes;
    let end = start
        .checked_add(length)
        .ok_or(PacketError::UnexpectedEnd)?;
    if packet.len() < end {
        return Err(PacketError::UnexpectedEnd);
    }
    if packet.len() > end {
        return Err(PacketError::TrailingBytes);
    }
    let command = std::str::from_utf8(&packet[start..end])
        .map_err(|_| PacketError::InvalidUtf8)?
        .to_owned();
    Ok(Some(command))
}

/// Decodes Minecraft 26.2's serverbound chat packet. Carbon currently accepts
/// it only in offline-development mode and re-publishes it as system chat.
pub fn decode_chat_message(packet: &[u8]) -> Result<Option<String>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_CHAT_ID {
        return Ok(None);
    }
    let mut offset = id_bytes;
    let length = take_varint(packet, &mut offset)?;
    let length = usize::try_from(length).map_err(|_| PacketError::InvalidLength(length))?;
    if length > 1024 {
        return Err(PacketError::InvalidLength(
            i32::try_from(length).unwrap_or(i32::MAX),
        ));
    }
    let end = offset
        .checked_add(length)
        .ok_or(PacketError::UnexpectedEnd)?;
    let message = std::str::from_utf8(packet.get(offset..end).ok_or(PacketError::UnexpectedEnd)?)
        .map_err(|_| PacketError::InvalidUtf8)?
        .to_owned();
    if message.chars().count() > 256 {
        return Err(PacketError::InvalidLength(
            i32::try_from(message.chars().count()).unwrap_or(i32::MAX),
        ));
    }
    offset = end;
    skip_bytes(packet, &mut offset, 16)?; // Timestamp and salt.
    if take_u8(packet, &mut offset)? != 0 {
        skip_bytes(packet, &mut offset, 256)?; // Optional fixed-size signature.
    }
    let _acknowledged_offset = take_varint(packet, &mut offset)?;
    skip_bytes(packet, &mut offset, 3)?; // Fixed 20-bit last-seen mask.
    let _checksum = take_u8(packet, &mut offset)?;
    if offset != packet.len() {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(message))
}

/// Encodes a plain, server-authored Minecraft 26.2 system-chat component.
#[must_use]
pub fn encode_system_chat(text: &str, overlay: bool) -> Vec<u8> {
    let bytes = text.as_bytes();
    let length = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
    let mut payload = BytesMut::with_capacity(usize::from(length) + 4);
    payload.put_u8(8); // Network NBT string tag.
    payload.put_u16(length);
    payload.extend_from_slice(&bytes[..usize::from(length)]);
    payload.put_u8(u8::from(overlay));
    frame_packet(CLIENTBOUND_SYSTEM_CHAT_ID, &payload)
}

/// Encodes Minecraft 26.2's play-state disconnect packet with a plain reason.
#[must_use]
pub fn encode_play_disconnect(reason: &str) -> Vec<u8> {
    let bytes = reason.as_bytes();
    let length = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
    let mut payload = BytesMut::with_capacity(usize::from(length) + 3);
    payload.put_u8(8); // Network NBT string tag.
    payload.put_u16(length);
    payload.extend_from_slice(&bytes[..usize::from(length)]);
    frame_packet(CLIENTBOUND_DISCONNECT_ID, &payload)
}

#[must_use]
pub fn encode_update_mob_effect(
    entity_id: i32,
    effect_id: i32,
    amplifier: u8,
    duration_ticks: u32,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    encode_varint(effect_id, &mut payload);
    encode_varint(i32::from(amplifier), &mut payload);
    encode_varint(
        i32::try_from(duration_ticks).unwrap_or(i32::MAX),
        &mut payload,
    );
    payload.put_u8(0x06); // Visible particles and HUD icon; not ambient.
    frame_packet(CLIENTBOUND_UPDATE_MOB_EFFECT_ID, &payload)
}

#[must_use]
pub fn encode_remove_mob_effect(entity_id: i32, effect_id: i32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    encode_varint(effect_id, &mut payload);
    frame_packet(CLIENTBOUND_REMOVE_MOB_EFFECT_ID, &payload)
}

pub fn decode_container_click(packet: &[u8]) -> Result<Option<ContainerClick>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_CONTAINER_CLICK_ID {
        return Ok(None);
    }
    let mut offset = id_bytes;
    let container_id = take_varint(packet, &mut offset)?;
    let state_id = take_varint(packet, &mut offset)?;
    let slot = take_i16(packet, &mut offset)?;
    let button = take_u8(packet, &mut offset)? as i8;
    let input = take_varint(packet, &mut offset)?;
    let changed_count = bounded_count(take_varint(packet, &mut offset)?, 128)?;
    for _ in 0..changed_count {
        let _changed_slot = take_i16(packet, &mut offset)?;
        skip_hashed_stack(packet, &mut offset)?;
    }
    skip_hashed_stack(packet, &mut offset)?;
    if offset != packet.len() {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(ContainerClick {
        container_id,
        state_id,
        slot,
        button,
        input,
    }))
}

pub fn decode_container_close(packet: &[u8]) -> Result<Option<i32>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_CONTAINER_CLOSE_ID {
        return Ok(None);
    }
    let (container_id, consumed) = decode_varint(&packet[id_bytes..])?;
    if packet.len() != id_bytes + consumed {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(container_id))
}

fn take_varint(packet: &[u8], offset: &mut usize) -> Result<i32, PacketError> {
    let (value, consumed) =
        decode_varint(packet.get(*offset..).ok_or(PacketError::UnexpectedEnd)?)?;
    *offset = offset
        .checked_add(consumed)
        .ok_or(PacketError::UnexpectedEnd)?;
    Ok(value)
}

fn take_u8(packet: &[u8], offset: &mut usize) -> Result<u8, PacketError> {
    let value = *packet.get(*offset).ok_or(PacketError::UnexpectedEnd)?;
    *offset += 1;
    Ok(value)
}

fn take_i16(packet: &[u8], offset: &mut usize) -> Result<i16, PacketError> {
    let end = offset.checked_add(2).ok_or(PacketError::UnexpectedEnd)?;
    let bytes = packet.get(*offset..end).ok_or(PacketError::UnexpectedEnd)?;
    *offset = end;
    Ok(i16::from_be_bytes(
        bytes.try_into().map_err(|_| PacketError::UnexpectedEnd)?,
    ))
}

fn skip_bytes(packet: &[u8], offset: &mut usize, count: usize) -> Result<(), PacketError> {
    let end = offset
        .checked_add(count)
        .ok_or(PacketError::UnexpectedEnd)?;
    packet.get(*offset..end).ok_or(PacketError::UnexpectedEnd)?;
    *offset = end;
    Ok(())
}

fn bounded_count(value: i32, maximum: usize) -> Result<usize, PacketError> {
    let count = usize::try_from(value).map_err(|_| PacketError::InvalidLength(value))?;
    if count > maximum {
        return Err(PacketError::InvalidLength(value));
    }
    Ok(count)
}

fn skip_hashed_stack(packet: &[u8], offset: &mut usize) -> Result<(), PacketError> {
    if take_u8(packet, offset)? == 0 {
        return Ok(());
    }
    let _item_holder = take_varint(packet, offset)?;
    let _count = take_varint(packet, offset)?;
    let added = bounded_count(take_varint(packet, offset)?, 256)?;
    for _ in 0..added {
        let _component_type = take_varint(packet, offset)?;
        let end = offset.checked_add(4).ok_or(PacketError::UnexpectedEnd)?;
        packet.get(*offset..end).ok_or(PacketError::UnexpectedEnd)?;
        *offset = end;
    }
    let removed = bounded_count(take_varint(packet, offset)?, 256)?;
    for _ in 0..removed {
        let _component_type = take_varint(packet, offset)?;
    }
    Ok(())
}

/// Encodes Minecraft 26.2's generic entity-spawn packet.
#[must_use]
pub fn encode_add_entity(
    entity_id: i32,
    uuid: [u8; 16],
    entity_type: i32,
    position: [f64; 3],
    velocity: [f64; 3],
    yaw: f32,
) -> Vec<u8> {
    encode_add_entity_with_rotation(entity_id, uuid, entity_type, position, velocity, yaw, 0.0)
}

/// Encodes an entity spawn with independent body yaw and pitch.
#[must_use]
pub fn encode_add_entity_with_rotation(
    entity_id: i32,
    uuid: [u8; 16],
    entity_type: i32,
    position: [f64; 3],
    velocity: [f64; 3],
    yaw: f32,
    pitch: f32,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    payload.extend_from_slice(&uuid);
    encode_varint(entity_type, &mut payload);
    for coordinate in position {
        payload.put_f64(coordinate);
    }
    encode_lp_vec3(velocity, &mut payload);
    payload.put_u8(pack_rotation(pitch));
    payload.put_u8(pack_rotation(yaw));
    payload.put_u8(pack_rotation(yaw)); // Head yaw.
    encode_varint(0, &mut payload); // Entity-specific spawn data.
    frame_packet(CLIENTBOUND_ADD_ENTITY_ID, &payload)
}

/// Encodes a relative entity movement and rotation update.
#[must_use]
pub fn encode_move_entity_pos_rot(
    entity_id: i32,
    delta: [f64; 3],
    yaw: f32,
    pitch: f32,
    on_ground: bool,
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    for component in delta {
        payload.put_i16(pack_relative(component));
    }
    payload.put_u8(pack_rotation(yaw));
    payload.put_u8(pack_rotation(pitch));
    payload.put_u8(u8::from(on_ground));
    frame_packet(CLIENTBOUND_MOVE_ENTITY_POS_ROT_ID, &payload)
}

/// Rotates a living entity's head independently of its body.
#[must_use]
pub fn encode_rotate_head(entity_id: i32, yaw: f32) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    payload.put_u8(pack_rotation(yaw));
    frame_packet(CLIENTBOUND_ROTATE_HEAD_ID, &payload)
}

/// Updates the base entity flags; bit zero is Minecraft's visual on-fire flag.
#[must_use]
pub fn encode_entity_flags(entity_id: i32, on_fire: bool) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    payload.put_u8(0); // Shared entity flags metadata index.
    encode_varint(0, &mut payload); // BYTE entity-data serializer.
    payload.put_u8(u8::from(on_fire));
    payload.put_u8(0xff); // End of packed entity data.
    frame_packet(CLIENTBOUND_SET_ENTITY_DATA_ID, &payload)
}

/// Sets an entity's client-side velocity in blocks per tick.
#[must_use]
pub fn encode_set_entity_motion(entity_id: i32, velocity: [f64; 3]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    for component in velocity {
        payload.put_i16((component.clamp(-3.9, 3.9) * 8_000.0).round() as i16);
    }
    frame_packet(CLIENTBOUND_SET_ENTITY_MOTION_ID, &payload)
}

/// Synchronizes one or more living-entity equipment slots.
///
/// Each tuple is `(slot, count, item_id, damage)`. Slot values follow the
/// protocol order: main hand 0, offhand 1, feet 2, legs 3, chest 4, head 5.
#[must_use]
pub fn encode_set_equipment(entity_id: i32, equipment: &[(u8, u8, i32, u16)]) -> Vec<u8> {
    let equipment: Vec<_> = equipment
        .iter()
        .map(|&(slot, count, item_id, damage)| (slot, count, item_id, damage, false))
        .collect();
    encode_set_equipment_with_glint(entity_id, &equipment)
}

#[must_use]
pub fn encode_set_equipment_with_glint(
    entity_id: i32,
    equipment: &[(u8, u8, i32, u16, bool)],
) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(entity_id, &mut payload);
    for (index, &(slot, count, item_id, damage, glint)) in equipment.iter().enumerate() {
        let continuation = u8::from(index + 1 < equipment.len()) << 7;
        payload.put_u8((slot & 0x7f) | continuation);
        if count == 0 {
            encode_varint(0, &mut payload);
        } else {
            encode_item_stack(count, item_id, damage, glint, &mut payload);
        }
    }
    frame_packet(CLIENTBOUND_SET_EQUIPMENT_ID, &payload)
}

/// Plays a vanilla entity event, such as the living-entity hurt animation.
#[must_use]
pub fn encode_entity_event(entity_id: i32, event: u8) -> Vec<u8> {
    let mut payload = BytesMut::new();
    payload.put_i32(entity_id);
    payload.put_u8(event);
    frame_packet(CLIENTBOUND_ENTITY_EVENT_ID, &payload)
}

/// Removes one or more entities from the client world.
#[must_use]
pub fn encode_remove_entities(entity_ids: &[i32]) -> Vec<u8> {
    let mut payload = BytesMut::new();
    encode_varint(
        i32::try_from(entity_ids.len()).unwrap_or(i32::MAX),
        &mut payload,
    );
    for entity_id in entity_ids {
        encode_varint(*entity_id, &mut payload);
    }
    frame_packet(CLIENTBOUND_REMOVE_ENTITIES_ID, &payload)
}

/// Decodes the two play packets that carry a player's absolute position.
pub fn decode_player_movement(packet: &[u8]) -> Result<Option<PlayerMovement>, PacketError> {
    let (packet_id, consumed) = decode_varint(packet)?;
    let body = &packet[consumed..];
    let expected = match packet_id {
        SERVERBOUND_MOVE_PLAYER_POS_ID => 25,
        SERVERBOUND_MOVE_PLAYER_POS_ROT_ID => 33,
        _ => return Ok(None),
    };
    if body.len() < expected {
        return Err(PacketError::UnexpectedEnd);
    }
    if body.len() > expected {
        return Err(PacketError::TrailingBytes);
    }
    let x = f64::from_be_bytes(
        body[0..8]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    let y = f64::from_be_bytes(
        body[8..16]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    let z = f64::from_be_bytes(
        body[16..24]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return Err(PacketError::InvalidLength(-1));
    }
    let (yaw, pitch, flags) = if packet_id == SERVERBOUND_MOVE_PLAYER_POS_ROT_ID {
        (
            Some(f32::from_be_bytes(
                body[24..28]
                    .try_into()
                    .map_err(|_| PacketError::UnexpectedEnd)?,
            )),
            Some(f32::from_be_bytes(
                body[28..32]
                    .try_into()
                    .map_err(|_| PacketError::UnexpectedEnd)?,
            )),
            body[32],
        )
    } else {
        (None, None, body[24])
    };
    Ok(Some(PlayerMovement {
        x,
        y,
        z,
        yaw,
        pitch,
        on_ground: flags & 1 != 0,
        horizontal_collision: flags & 2 != 0,
    }))
}

pub fn decode_player_rotation(packet: &[u8]) -> Result<Option<PlayerRotation>, PacketError> {
    let (packet_id, consumed) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_MOVE_PLAYER_ROT_ID {
        return Ok(None);
    }
    let body = &packet[consumed..];
    if body.len() < 9 {
        return Err(PacketError::UnexpectedEnd);
    }
    if body.len() > 9 {
        return Err(PacketError::TrailingBytes);
    }
    let yaw = f32::from_be_bytes(
        body[0..4]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    let pitch = f32::from_be_bytes(
        body[4..8]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    if !yaw.is_finite() || !pitch.is_finite() {
        return Err(PacketError::InvalidLength(-1));
    }
    Ok(Some(PlayerRotation {
        yaw,
        pitch,
        on_ground: body[8] & 1 != 0,
        horizontal_collision: body[8] & 2 != 0,
    }))
}

pub fn decode_swing(packet: &[u8]) -> Result<Option<i32>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_SWING_ID {
        return Ok(None);
    }
    let (hand, hand_bytes) = decode_varint(&packet[id_bytes..])?;
    if packet.len() != id_bytes + hand_bytes {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(hand))
}

pub fn decode_player_action(packet: &[u8]) -> Result<Option<PlayerAction>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_PLAYER_ACTION_ID {
        return Ok(None);
    }
    let (action, action_bytes) = decode_varint(&packet[id_bytes..])?;
    let offset = id_bytes + action_bytes;
    if packet.len() < offset + 9 {
        return Err(PacketError::UnexpectedEnd);
    }
    let packed = i64::from_be_bytes(
        packet[offset..offset + 8]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    let sequence_offset = offset + 9;
    let (sequence, sequence_bytes) = decode_varint(&packet[sequence_offset..])?;
    if packet.len() != sequence_offset + sequence_bytes {
        return Err(PacketError::TrailingBytes);
    }
    let (x, y, z) = unpack_block_position(packed);
    Ok(Some(PlayerAction {
        action,
        x,
        y,
        z,
        sequence,
    }))
}

/// Decodes a player command and returns its action ID.
pub fn decode_player_command(packet: &[u8]) -> Result<Option<i32>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_PLAYER_COMMAND_ID {
        return Ok(None);
    }
    let (_, entity_bytes) = decode_varint(&packet[id_bytes..])?;
    let action_offset = id_bytes + entity_bytes;
    let (action, action_bytes) = decode_varint(&packet[action_offset..])?;
    let data_offset = action_offset + action_bytes;
    let (_, data_bytes) = decode_varint(&packet[data_offset..])?;
    if packet.len() != data_offset + data_bytes {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(action))
}

pub fn decode_set_carried_item(packet: &[u8]) -> Result<Option<u16>, PacketError> {
    let (packet_id, consumed) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_SET_CARRIED_ITEM_ID {
        return Ok(None);
    }
    if packet.len() != consumed + 2 {
        return Err(if packet.len() < consumed + 2 {
            PacketError::UnexpectedEnd
        } else {
            PacketError::TrailingBytes
        });
    }
    Ok(Some(u16::from_be_bytes(
        packet[consumed..]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    )))
}

pub fn decode_use_item_on(packet: &[u8]) -> Result<Option<UseItemOn>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_USE_ITEM_ON_ID {
        return Ok(None);
    }
    let (hand, hand_bytes) = decode_varint(&packet[id_bytes..])?;
    let position_offset = id_bytes + hand_bytes;
    if packet.len() < position_offset + 8 {
        return Err(PacketError::UnexpectedEnd);
    }
    let packed = i64::from_be_bytes(
        packet[position_offset..position_offset + 8]
            .try_into()
            .map_err(|_| PacketError::UnexpectedEnd)?,
    );
    let face_offset = position_offset + 8;
    let (face, face_bytes) = decode_varint(&packet[face_offset..])?;
    let sequence_offset = face_offset + face_bytes + 14;
    if packet.len() < sequence_offset {
        return Err(PacketError::UnexpectedEnd);
    }
    let (sequence, sequence_bytes) = decode_varint(&packet[sequence_offset..])?;
    if packet.len() != sequence_offset + sequence_bytes {
        return Err(PacketError::TrailingBytes);
    }
    let (x, y, z) = unpack_block_position(packed);
    Ok(Some(UseItemOn {
        hand,
        x,
        y,
        z,
        face,
        sequence,
    }))
}

pub fn decode_use_item(packet: &[u8]) -> Result<Option<(i32, i32)>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_USE_ITEM_ID {
        return Ok(None);
    }
    let (hand, hand_bytes) = decode_varint(&packet[id_bytes..])?;
    let sequence_offset = id_bytes + hand_bytes;
    let (sequence, sequence_bytes) = decode_varint(&packet[sequence_offset..])?;
    let rotations_offset = sequence_offset + sequence_bytes;
    if packet.len() != rotations_offset + 8 {
        return Err(if packet.len() < rotations_offset + 8 {
            PacketError::UnexpectedEnd
        } else {
            PacketError::TrailingBytes
        });
    }
    Ok(Some((hand, sequence)))
}

pub fn decode_attack(packet: &[u8]) -> Result<Option<i32>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_ATTACK_ID {
        return Ok(None);
    }
    let (entity_id, entity_bytes) = decode_varint(&packet[id_bytes..])?;
    if packet.len() != id_bytes + entity_bytes {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(entity_id))
}

/// Decodes the normal entity-interaction branch of Minecraft 26.2's interact packet.
/// Attack has its own packet in this protocol version; interact-at is intentionally ignored.
pub fn decode_interact_entity(packet: &[u8]) -> Result<Option<(i32, i32)>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_INTERACT_ID {
        return Ok(None);
    }
    let (entity_id, entity_bytes) = decode_varint(&packet[id_bytes..])?;
    let action_offset = id_bytes + entity_bytes;
    let (action, action_bytes) = decode_varint(&packet[action_offset..])?;
    if action != 0 {
        return Ok(None);
    }
    let hand_offset = action_offset + action_bytes;
    let (hand, hand_bytes) = decode_varint(&packet[hand_offset..])?;
    let sneaking_offset = hand_offset + hand_bytes;
    if packet.len() != sneaking_offset + 1 {
        return Err(if packet.len() < sneaking_offset + 1 {
            PacketError::UnexpectedEnd
        } else {
            PacketError::TrailingBytes
        });
    }
    if packet[sneaking_offset] > 1 {
        return Err(PacketError::InvalidLength(i32::from(
            packet[sneaking_offset],
        )));
    }
    Ok(Some((entity_id, hand)))
}

pub fn decode_client_command(packet: &[u8]) -> Result<Option<i32>, PacketError> {
    let (packet_id, id_bytes) = decode_varint(packet)?;
    if packet_id != SERVERBOUND_CLIENT_COMMAND_ID {
        return Ok(None);
    }
    let (action, action_bytes) = decode_varint(&packet[id_bytes..])?;
    if packet.len() != id_bytes + action_bytes {
        return Err(PacketError::TrailingBytes);
    }
    Ok(Some(action))
}

fn pack_block_position(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3ff_ffff) << 38)
        | ((i64::from(z) & 0x3ff_ffff) << 12)
        | (i64::from(y) & 0xfff)
}

fn unpack_block_position(value: i64) -> (i32, i32, i32) {
    let x = (value >> 38) as i32;
    let y = (value << 52 >> 52) as i32;
    let z = (value << 26 >> 38) as i32;
    (x, y, z)
}

fn pack_rotation(degrees: f32) -> u8 {
    (degrees.rem_euclid(360.0) * (256.0 / 360.0)) as u8
}

fn encode_lp_vec3(vector: [f64; 3], output: &mut BytesMut) {
    let vector = vector.map(|value| {
        if value.is_nan() {
            0.0
        } else {
            value.clamp(-17_179_869_183.0, 17_179_869_183.0)
        }
    });
    let absolute_max = vector.into_iter().map(f64::abs).fold(0.0, f64::max);
    if absolute_max < 3.051_944_088_384_301e-5 {
        output.put_u8(0);
        return;
    }

    let scale = absolute_max.ceil() as u64;
    let continued = scale & 3 != scale;
    let scale_header = if continued { (scale & 3) | 4 } else { scale };
    let pack = |value: f64| (((value / scale as f64) * 0.5 + 0.5) * 32_766.0).round() as u64;
    let packed =
        scale_header | (pack(vector[0]) << 3) | (pack(vector[1]) << 18) | (pack(vector[2]) << 33);
    output.put_u8(packed as u8);
    output.put_u8((packed >> 8) as u8);
    output.put_u32((packed >> 16) as u32);
    if continued {
        encode_varint(i32::try_from(scale >> 2).unwrap_or(i32::MAX), output);
    }
}

fn pack_relative(value: f64) -> i16 {
    (value.clamp(-7.999, 7.999) * 4_096.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode_varint;

    #[test]
    fn play_login_targets_the_verified_packet_id() {
        let framed = encode_play_login(21, 20, 10, 42);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (id, _) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_LOGIN_ID);
    }

    #[test]
    fn position_packet_has_the_26_2_layout() {
        let framed = encode_player_position(1);
        let (length, outer) = decode_varint(&framed).unwrap();
        assert_eq!(length, 62);
        assert_eq!(framed[outer], CLIENTBOUND_PLAYER_POSITION_ID as u8);
    }

    #[test]
    fn player_info_uses_a_fixed_width_action_mask() {
        let framed = encode_player_info("CarbonCapture", [0x11; 16]);
        let (_, outer) = decode_varint(&framed).unwrap();
        assert_eq!(
            &framed[outer..outer + 3],
            &[CLIENTBOUND_PLAYER_INFO_UPDATE_ID as u8, 0xff, 1]
        );
    }

    #[test]
    fn frames_a_single_chunk_batch() {
        assert_eq!(encode_chunk_batch_start(), vec![1, 12]);
        assert_eq!(encode_chunk_batch_finished(1), vec![2, 11, 1]);
    }

    #[test]
    fn forget_chunk_uses_the_verified_id_and_packed_chunk_position() {
        let framed = encode_forget_level_chunk(-2, 3);
        assert_eq!(framed[0], 9);
        assert_eq!(framed[1], CLIENTBOUND_FORGET_LEVEL_CHUNK_ID as u8);
        assert_eq!(
            i64::from_be_bytes(framed[2..10].try_into().unwrap()),
            (i64::from(3_i32) << 32) | (i64::from(-2_i32) & 0xffff_ffff),
        );
    }

    #[test]
    fn announces_the_initial_level_chunk_load() {
        assert_eq!(
            encode_level_chunks_load_start(),
            vec![6, CLIENTBOUND_GAME_EVENT_ID as u8, 13, 0, 0, 0, 0]
        );
    }

    #[test]
    fn mob_packets_use_verified_26_2_ids() {
        let add = encode_add_entity(1000, [1; 16], 30, [-8.0, 65.0, -8.0], [0.0; 3], 90.0);
        let (add_length, add_outer) = decode_varint(&add).unwrap();
        assert_eq!(add_length, 49);
        assert_eq!(decode_varint(&add[add_outer..]).unwrap().0, 1);

        let movement = encode_move_entity_pos_rot(1000, [0.25, 0.0, -0.25], 45.0, 0.0, true);
        let (_, movement_outer) = decode_varint(&movement).unwrap();
        assert_eq!(decode_varint(&movement[movement_outer..]).unwrap().0, 54);

        assert_eq!(
            decode_varint(&encode_rotate_head(1000, 90.0)[1..])
                .unwrap()
                .0,
            83
        );
        assert_eq!(
            decode_varint(&encode_entity_flags(1000, true)[1..])
                .unwrap()
                .0,
            99
        );
        assert_eq!(
            decode_varint(&encode_entity_event(1000, 2)[1..]).unwrap().0,
            34
        );
        assert_eq!(
            decode_varint(&encode_remove_entities(&[1000])[1..])
                .unwrap()
                .0,
            77
        );
    }

    #[test]
    fn motion_and_equipment_packets_use_verified_ids_and_scaling() {
        let motion = encode_set_entity_motion(42, [0.4, 0.4, -0.2]);
        let (_, outer) = decode_varint(&motion).unwrap();
        let (packet_id, id_bytes) = decode_varint(&motion[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_SET_ENTITY_MOTION_ID);
        let (entity_id, entity_bytes) = decode_varint(&motion[outer + id_bytes..]).unwrap();
        assert_eq!(entity_id, 42);
        let velocity = &motion[outer + id_bytes + entity_bytes..];
        assert_eq!(i16::from_be_bytes([velocity[0], velocity[1]]), 3_200);
        assert_eq!(i16::from_be_bytes([velocity[4], velocity[5]]), -1_600);

        let equipment = encode_set_equipment(42, &[(0, 1, 941, 0), (5, 0, 0, 0)]);
        let (_, outer) = decode_varint(&equipment).unwrap();
        let (packet_id, id_bytes) = decode_varint(&equipment[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_SET_EQUIPMENT_ID);
        let (_, entity_bytes) = decode_varint(&equipment[outer + id_bytes..]).unwrap();
        let slots = &equipment[outer + id_bytes + entity_bytes..];
        assert_eq!(slots[0], 0x80);
        assert!(slots.contains(&5));
    }

    #[test]
    fn entity_spawn_preserves_independent_pitch() {
        let framed = encode_add_entity_with_rotation(
            7,
            [2; 16],
            156,
            [1.25, 65.0, -3.5],
            [0.0; 3],
            90.0,
            -45.0,
        );
        assert_eq!(framed[framed.len() - 4], pack_rotation(-45.0));
        assert_eq!(framed[framed.len() - 3], pack_rotation(90.0));
    }

    #[test]
    fn decodes_player_position_and_flags() {
        let mut packet = BytesMut::new();
        encode_varint(SERVERBOUND_MOVE_PLAYER_POS_ID, &mut packet);
        packet.put_f64(1.25);
        packet.put_f64(65.0);
        packet.put_f64(-2.5);
        packet.put_u8(3);
        let movement = decode_player_movement(&packet).unwrap().unwrap();
        assert_eq!((movement.x, movement.y, movement.z), (1.25, 65.0, -2.5));
        assert!(movement.on_ground);
        assert!(movement.horizontal_collision);
    }

    #[test]
    fn decodes_rotation_only_and_swing_packets() {
        let mut rotation = BytesMut::new();
        encode_varint(SERVERBOUND_MOVE_PLAYER_ROT_ID, &mut rotation);
        rotation.put_f32(135.0);
        rotation.put_f32(-20.0);
        rotation.put_u8(3);
        let decoded = decode_player_rotation(&rotation).unwrap().unwrap();
        assert_eq!((decoded.yaw, decoded.pitch), (135.0, -20.0));
        assert!(decoded.on_ground);
        assert!(decoded.horizontal_collision);

        let mut swing = BytesMut::new();
        encode_varint(SERVERBOUND_SWING_ID, &mut swing);
        encode_varint(1, &mut swing);
        assert_eq!(decode_swing(&swing).unwrap(), Some(1));
        swing.put_u8(0);
        assert!(matches!(
            decode_swing(&swing),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn decodes_sprint_player_commands() {
        let mut packet = BytesMut::new();
        encode_varint(SERVERBOUND_PLAYER_COMMAND_ID, &mut packet);
        encode_varint(42, &mut packet);
        encode_varint(3, &mut packet);
        encode_varint(0, &mut packet);
        assert_eq!(decode_player_command(&packet).unwrap(), Some(3));
        packet.put_u8(0);
        assert!(matches!(
            decode_player_command(&packet),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn animate_packet_uses_the_verified_id_and_action() {
        let framed = encode_animate(42, 3);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (packet_id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_ANIMATE_ID);
        let (entity_id, entity_bytes) = decode_varint(&framed[outer + id_bytes..]).unwrap();
        assert_eq!(entity_id, 42);
        assert_eq!(framed[outer + id_bytes + entity_bytes], 3);
    }

    #[test]
    fn decodes_use_item_hand_sequence_and_rotations() {
        let mut packet = BytesMut::new();
        encode_varint(SERVERBOUND_USE_ITEM_ID, &mut packet);
        encode_varint(0, &mut packet);
        encode_varint(42, &mut packet);
        packet.put_f32(90.0);
        packet.put_f32(-15.0);
        assert_eq!(decode_use_item(&packet).unwrap(), Some((0, 42)));
        packet.put_u8(0);
        assert!(matches!(
            decode_use_item(&packet),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn decodes_normal_26_2_entity_interaction() {
        let mut packet = BytesMut::new();
        encode_varint(SERVERBOUND_INTERACT_ID, &mut packet);
        encode_varint(123, &mut packet);
        encode_varint(0, &mut packet); // Interact rather than interact-at.
        encode_varint(0, &mut packet); // Main hand.
        packet.put_u8(0); // Not sneaking.
        assert_eq!(decode_interact_entity(&packet).unwrap(), Some((123, 0)));

        packet.put_u8(0);
        assert!(matches!(
            decode_interact_entity(&packet),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn inventory_item_stack_encodes_count_once_and_preserves_damage() {
        let framed = encode_set_player_inventory(0, 1, 941, 7);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (_, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        let body = &framed[outer + id_bytes..];
        let (slot, slot_bytes) = decode_varint(body).unwrap();
        let (count, count_bytes) = decode_varint(&body[slot_bytes..]).unwrap();
        let (item_id, _) = decode_varint(&body[slot_bytes + count_bytes..]).unwrap();
        assert_eq!((slot, count, item_id), (0, 1, 941));
        assert!(body.ends_with(&[1, 3, 7, 0]));
    }

    #[test]
    fn enchanted_inventory_stack_uses_verified_glint_component() {
        let framed = encode_set_player_inventory_with_glint(0, 1, 939, 0, true);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (_, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        let body = &framed[outer + id_bytes..];
        let (_, slot_bytes) = decode_varint(body).unwrap();
        let (_, count_bytes) = decode_varint(&body[slot_bytes..]).unwrap();
        let (_, item_bytes) = decode_varint(&body[slot_bytes + count_bytes..]).unwrap();
        let components = &body[slot_bytes + count_bytes + item_bytes..];
        assert_eq!(components, &[1, 21, 1, 0]);
    }

    #[test]
    fn decodes_26_2_container_click_and_consumes_hashed_stacks() {
        let mut packet = BytesMut::new();
        encode_varint(SERVERBOUND_CONTAINER_CLICK_ID, &mut packet);
        encode_varint(0, &mut packet);
        encode_varint(7, &mut packet);
        packet.put_i16(36);
        packet.put_i8(0);
        encode_varint(0, &mut packet);
        encode_varint(1, &mut packet);
        packet.put_i16(36);
        packet.put_u8(1);
        encode_varint(5, &mut packet);
        encode_varint(2, &mut packet);
        encode_varint(1, &mut packet);
        encode_varint(21, &mut packet);
        packet.put_i32(0x1234_5678);
        encode_varint(0, &mut packet);
        packet.put_u8(0);

        assert_eq!(
            decode_container_click(&packet).unwrap(),
            Some(ContainerClick {
                container_id: 0,
                state_id: 7,
                slot: 36,
                button: 0,
                input: 0,
            })
        );
        packet.put_u8(0);
        assert!(matches!(
            decode_container_click(&packet),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn decodes_26_2_container_close_without_trailing_bytes() {
        let mut packet = BytesMut::new();
        encode_varint(SERVERBOUND_CONTAINER_CLOSE_ID, &mut packet);
        encode_varint(0, &mut packet);
        assert_eq!(decode_container_close(&packet).unwrap(), Some(0));
        packet.put_u8(0);
        assert!(matches!(
            decode_container_close(&packet),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn cursor_item_uses_verified_packet_id_and_optional_stack_layout() {
        let empty = encode_set_cursor_item_with_glint(0, 0, 0, false);
        assert_eq!(empty, vec![2, CLIENTBOUND_SET_CURSOR_ITEM_ID as u8, 0]);
        let stack = encode_set_cursor_item_with_glint(1, 939, 0, true);
        let (_, outer) = decode_varint(&stack).unwrap();
        let (id, id_bytes) = decode_varint(&stack[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_SET_CURSOR_ITEM_ID);
        let body = &stack[outer + id_bytes..];
        assert_eq!(decode_varint(body).unwrap().0, 1);
        assert!(body.ends_with(&[1, 21, 1, 0]));
    }

    #[test]
    fn container_slot_uses_verified_26_2_layout() {
        let framed = encode_container_set_slot_with_glint(0, 7, 4, 2, 126, 0, false);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_CONTAINER_SET_SLOT_ID);
        let body = &framed[outer + id_bytes..];
        let (container, container_bytes) = decode_varint(body).unwrap();
        let (state, state_bytes) = decode_varint(&body[container_bytes..]).unwrap();
        assert_eq!((container, state), (0, 7));
        let slot_offset = container_bytes + state_bytes;
        assert_eq!(&body[slot_offset..slot_offset + 2], &[0, 4]);
        assert_eq!(decode_varint(&body[slot_offset + 2..]).unwrap().0, 2);
    }

    #[test]
    fn crafting_screen_uses_verified_packet_and_menu_ids() {
        let framed = encode_open_crafting_screen(1);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_OPEN_SCREEN_ID);
        let body = &framed[outer + id_bytes..];
        let (container, container_bytes) = decode_varint(body).unwrap();
        let (menu, menu_bytes) = decode_varint(&body[container_bytes..]).unwrap();
        assert_eq!((container, menu), (1, 12));
        assert_eq!(
            &body[container_bytes + menu_bytes..],
            &[8, 0, 8, b'C', b'r', b'a', b'f', b't', b'i', b'n', b'g']
        );
    }

    #[test]
    fn furnace_screen_and_progress_use_verified_26_2_layouts() {
        let framed = encode_open_furnace_screen(2);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_OPEN_SCREEN_ID);
        let body = &framed[outer + id_bytes..];
        let (container, used) = decode_varint(body).unwrap();
        assert_eq!(container, 2);
        assert_eq!(decode_varint(&body[used..]).unwrap().0, 14);

        let data = encode_container_set_data(2, 3, 200);
        let (_, outer) = decode_varint(&data).unwrap();
        let (id, used) = decode_varint(&data[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_CONTAINER_SET_DATA_ID);
        assert_eq!(&data[outer + used..], &[2, 0, 3, 0, 200]);
    }

    #[test]
    fn chest_screen_uses_verified_26_2_menu_id() {
        let framed = encode_open_chest_screen(3);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(id, CLIENTBOUND_OPEN_SCREEN_ID);
        let body = &framed[outer + id_bytes..];
        let (container, used) = decode_varint(body).unwrap();
        assert_eq!(container, 3);
        assert_eq!(decode_varint(&body[used..]).unwrap().0, 2);
    }

    #[test]
    fn command_tree_contains_executable_literals() {
        let framed = encode_commands(&["craft_planks", "craft_sticks"]);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (packet_id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_COMMANDS_ID);
        let (nodes, _) = decode_varint(&framed[outer + id_bytes..]).unwrap();
        assert_eq!(nodes, 3);
        assert!(framed.windows(12).any(|window| window == b"craft_planks"));
        assert!(framed.windows(12).any(|window| window == b"craft_sticks"));
    }

    #[test]
    fn say_command_tree_has_verified_message_argument() {
        let framed = encode_commands(&["say"]);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (_, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        let payload = &framed[outer + id_bytes..];
        assert_eq!(decode_varint(payload).unwrap().0, 3);
        assert!(framed.windows(7).any(|window| window == b"message"));
        assert!(framed.ends_with(&[20, 0])); // Parser ID then root index.
    }

    #[test]
    fn decodes_unsigned_and_signed_26_2_chat_packets() {
        let build = |signed: bool| {
            let mut packet = BytesMut::new();
            encode_varint(SERVERBOUND_CHAT_ID, &mut packet);
            packet.extend_from_slice(&encode_string("Hello Carbon"));
            packet.put_i64(1_788_000_000_000);
            packet.put_i64(42);
            packet.put_u8(u8::from(signed));
            if signed {
                packet.extend_from_slice(&[0x5a; 256]);
            }
            encode_varint(0, &mut packet);
            packet.extend_from_slice(&[0, 0, 0]);
            packet.put_u8(7);
            packet
        };
        assert_eq!(
            decode_chat_message(&build(false)).unwrap().as_deref(),
            Some("Hello Carbon")
        );
        assert_eq!(
            decode_chat_message(&build(true)).unwrap().as_deref(),
            Some("Hello Carbon")
        );
        let mut trailing = build(false);
        trailing.put_u8(0);
        assert!(matches!(
            decode_chat_message(&trailing),
            Err(PacketError::TrailingBytes)
        ));
    }

    #[test]
    fn system_chat_uses_verified_packet_and_plain_component_layout() {
        let framed = encode_system_chat("Carbon online", false);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (packet_id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_SYSTEM_CHAT_ID);
        assert_eq!(
            &framed[outer + id_bytes..],
            &[
                8, 0, 13, b'C', b'a', b'r', b'b', b'o', b'n', b' ', b'o', b'n', b'l', b'i', b'n',
                b'e', 0
            ]
        );
    }

    #[test]
    fn play_disconnect_uses_verified_packet_and_component_layout() {
        let framed = encode_play_disconnect("Kicked");
        let (_, outer) = decode_varint(&framed).unwrap();
        let (packet_id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_DISCONNECT_ID);
        assert_eq!(
            &framed[outer + id_bytes..],
            &[8, 0, 6, b'K', b'i', b'c', b'k', b'e', b'd']
        );
    }

    #[test]
    fn respawn_packet_selects_the_requested_dimension_holder_and_name() {
        let framed = encode_dimension_respawn(42, 3, "minecraft:the_nether", 32);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (_, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        let body = &framed[outer + id_bytes..];
        assert_eq!(decode_varint(body).unwrap().0, 3);
        assert!(framed
            .windows("minecraft:the_nether".len())
            .any(|window| window == b"minecraft:the_nether"));
    }

    #[test]
    fn player_info_remove_encodes_uuid_collection() {
        let first = [0x11; 16];
        let second = [0x22; 16];
        let framed = encode_player_info_remove(&[first, second]);
        let (_, outer) = decode_varint(&framed).unwrap();
        let (packet_id, id_bytes) = decode_varint(&framed[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_PLAYER_INFO_REMOVE_ID);
        let payload = &framed[outer + id_bytes..];
        let (count, count_bytes) = decode_varint(payload).unwrap();
        assert_eq!(count, 2);
        assert_eq!(&payload[count_bytes..count_bytes + 16], &first);
        assert_eq!(&payload[count_bytes + 16..], &second);
    }

    #[test]
    fn mob_effect_packets_use_verified_26_2_layouts() {
        let update = encode_update_mob_effect(42, 9, 1, 600);
        let (_, outer) = decode_varint(&update).unwrap();
        let (packet_id, packet_bytes) = decode_varint(&update[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_UPDATE_MOB_EFFECT_ID);
        let payload = &update[outer + packet_bytes..];
        let (entity_id, used_entity) = decode_varint(payload).unwrap();
        let (effect_id, used_effect) = decode_varint(&payload[used_entity..]).unwrap();
        let (amplifier, used_amplifier) =
            decode_varint(&payload[used_entity + used_effect..]).unwrap();
        let duration_offset = used_entity + used_effect + used_amplifier;
        let (duration, used_duration) = decode_varint(&payload[duration_offset..]).unwrap();
        assert_eq!((entity_id, effect_id, amplifier, duration), (42, 9, 1, 600));
        assert_eq!(payload[duration_offset + used_duration], 0x06);

        let remove = encode_remove_mob_effect(42, 9);
        let (_, outer) = decode_varint(&remove).unwrap();
        let (packet_id, packet_bytes) = decode_varint(&remove[outer..]).unwrap();
        assert_eq!(packet_id, CLIENTBOUND_REMOVE_MOB_EFFECT_ID);
        let payload = &remove[outer + packet_bytes..];
        let (entity_id, used) = decode_varint(payload).unwrap();
        assert_eq!(
            (entity_id, decode_varint(&payload[used..]).unwrap().0),
            (42, 9)
        );
    }
}
