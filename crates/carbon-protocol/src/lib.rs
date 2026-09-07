//! Minecraft Java Edition protocol primitives implemented independently by Carbon.

mod chunk;
mod configuration;
mod packet;
mod play;
mod status;
mod varint;

pub use chunk::{
    encode_generated_chunk, encode_generated_chunk_with_blocks,
    encode_generated_chunk_with_blocks_and_biome, encode_generated_chunk_with_terrain,
    encode_generated_chunk_with_terrain_and_light_sources, BlockLightSource, ChunkBlockState,
    ChunkLighting, ChunkTerrainStates,
};
pub use configuration::{decode_finish_configuration, CONFIGURATION_SNAPSHOT};
pub use packet::{
    decode_handshake, decode_login_acknowledged, decode_login_start, decode_select_known_packs,
    encode_configuration_brand, encode_login_finished, encode_select_known_packs, encode_string,
    encode_update_enabled_features, frame_packet, read_frame, ConnectionState, Handshake,
    KnownPack, LoginStart, NextState, PacketError,
};
pub use play::{
    decode_attack, decode_chat_command, decode_chat_message, decode_client_command,
    decode_container_click, decode_container_close, decode_interact_entity, decode_player_action,
    decode_player_command, decode_player_movement, decode_player_rotation, decode_set_carried_item,
    decode_swing, decode_use_item, decode_use_item_on, encode_add_entity,
    encode_add_entity_with_rotation, encode_animate, encode_block_changed_ack, encode_block_update,
    encode_change_difficulty, encode_chunk_batch_finished, encode_chunk_batch_start,
    encode_chunk_cache_center, encode_commands, encode_container_set_data,
    encode_container_set_slot_with_glint, encode_default_spawn_position, encode_dimension_respawn,
    encode_entity_event, encode_entity_flags, encode_forget_level_chunk, encode_item_entity_data,
    encode_keep_alive, encode_level_chunks_load_start, encode_move_entity_pos_rot,
    encode_open_chest_screen, encode_open_crafting_screen, encode_open_furnace_screen,
    encode_play_disconnect, encode_play_login, encode_player_abilities, encode_player_info,
    encode_player_info_remove, encode_player_position, encode_player_position_at,
    encode_remove_entities, encode_remove_mob_effect, encode_respawn, encode_rotate_head,
    encode_set_cursor_item_with_glint, encode_set_entity_motion, encode_set_equipment,
    encode_set_equipment_with_glint, encode_set_health, encode_set_player_inventory,
    encode_set_player_inventory_with_glint, encode_system_chat, encode_update_mob_effect,
    ContainerClick, PlayerAction, PlayerMovement, PlayerRotation, UseItemOn, SPAWN_X, SPAWN_Y,
    SPAWN_Z, STARTER_CHUNK_COUNT,
};
pub use status::{StatusDescription, StatusPlayers, StatusResponse, StatusVersion};
pub use varint::{decode_varint, encode_varint, VarIntError};

/// Upper bound for a packet accepted during the pre-alpha handshake.
pub const MAX_PACKET_SIZE: usize = 2 * 1024 * 1024;

/// Minecraft Java Edition release targeted by this protocol crate.
pub const MINECRAFT_VERSION: &str = "26.2";
/// Protocol number embedded in Mojang's official 26.2 server metadata.
pub const PROTOCOL_VERSION: i32 = 776;
