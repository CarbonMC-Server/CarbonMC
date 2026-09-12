#[cfg(test)]
mod adversarial_tests;
mod transport;
use transport::{Admissions, Budget, Connection};

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use anyhow::{bail, Context};
use bytes::{BufMut, BytesMut};
use carbon_api::{
    BiomeKind, BlockKind, BlockPosition, ChestSnapshot, ChunkPosition, DimensionKind,
    EntityPosition, FurnaceSlot, FurnaceSnapshot, GameMode, InventoryCursor, ItemEntitySnapshot,
    ItemKind, ItemStack, MobKind, MobSnapshot, PlayerEquipment, PlayerEventKind,
    PlayerInventorySlot, PlayerSnapshot, PlayerTransform, ServerApi, StatusEffectKind,
};
use carbon_config::ServerConfig;
use carbon_protocol::{
    decode_attack, decode_chat_command, decode_chat_message, decode_client_command,
    decode_container_click, decode_container_close, decode_finish_configuration, decode_handshake,
    decode_interact_entity, decode_login_acknowledged, decode_login_start, decode_player_action,
    decode_player_command, decode_player_movement, decode_player_rotation,
    decode_select_known_packs, decode_set_carried_item, decode_swing, decode_use_item,
    decode_use_item_on, decode_varint, encode_add_entity, encode_add_entity_with_rotation,
    encode_animate, encode_block_changed_ack, encode_block_update, encode_change_difficulty,
    encode_chunk_batch_finished, encode_chunk_batch_start, encode_chunk_cache_center,
    encode_commands, encode_configuration_brand, encode_container_set_data,
    encode_container_set_slot_with_glint, encode_default_spawn_position, encode_dimension_respawn,
    encode_entity_event, encode_entity_flags, encode_forget_level_chunk,
    encode_generated_chunk_with_terrain_and_light_sources, encode_item_entity_data,
    encode_keep_alive, encode_level_chunks_load_start, encode_login_finished,
    encode_move_entity_pos_rot, encode_open_chest_screen, encode_open_crafting_screen,
    encode_open_furnace_screen, encode_play_disconnect, encode_play_login, encode_player_abilities,
    encode_player_info, encode_player_info_remove, encode_player_position,
    encode_player_position_at, encode_remove_entities, encode_remove_mob_effect,
    encode_rotate_head, encode_select_known_packs, encode_set_cursor_item_with_glint,
    encode_set_entity_motion, encode_set_equipment_with_glint, encode_set_health,
    encode_set_player_inventory_with_glint, encode_string, encode_system_chat,
    encode_update_enabled_features, encode_update_mob_effect, frame_packet, BlockLightSource,
    ChunkBlockState, ChunkLighting, ChunkTerrainStates, ConnectionState, KnownPack, NextState,
    StatusDescription, StatusPlayers, StatusResponse, StatusVersion, CONFIGURATION_SNAPSHOT,
    MINECRAFT_VERSION, PROTOCOL_VERSION,
};
use md5::{Digest, Md5};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    task::JoinSet,
    time::{self, timeout},
};
use tracing::{debug, info};
use uuid::Uuid;

pub async fn serve(
    listener: TcpListener,
    config: ServerConfig,
    state: Arc<dyn ServerApi>,
    mut shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    info!(address = %config.bind, "network listener ready");

    let admissions = Admissions::default();
    let mut attempts = Budget::new(128, 64);
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            result = listener.accept() => {
                let (stream, remote) = result.context("failed to accept connection")?;
                // Completed tasks count until reaped, bounding task bookkeeping too.
                if tasks.len() >= transport::MAX_CONNECTIONS || !attempts.take(1) { continue; }
                let Some(admission) = admissions.acquire(remote.ip()) else { continue; };
                let config = config.clone();
                let state = Arc::clone(&state);
                let connection_shutdown = shutdown.clone();
                tasks.spawn(async move {
                    let _admission = admission;
                    if let Err(error) = handle_connection(stream, remote, config, state, connection_shutdown).await {
                        debug!(%remote, %error, "connection closed");
                    }
                });
            }
            _ = tasks.join_next(), if !tasks.is_empty() => {}
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
    while tasks.join_next().await.is_some() {}
    info!("network listener stopped");
    Ok(())
}

pub async fn bind(config: &ServerConfig) -> anyhow::Result<TcpListener> {
    TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("could not bind {}; is Carbon already running?", config.bind))
}

async fn handle_connection(
    stream: TcpStream,
    remote: SocketAddr,
    config: ServerConfig,
    state: Arc<dyn ServerApi>,
    shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let mut stream = Connection::new(stream);
    stream.observe_shutdown(shutdown);
    let first_packet = timeout(Duration::from_secs(10), stream.read_frame())
        .await
        .context("handshake timed out")??;
    let handshake = decode_handshake(&first_packet)?;
    debug!(%remote, protocol = handshake.protocol_version, host = %handshake.server_address, "handshake");

    match handshake.next_state {
        NextState::Status => {
            handle_status(
                &mut stream,
                &config,
                state.as_ref(),
                handshake.protocol_version,
            )
            .await
        }
        NextState::Login => {
            handle_login(&mut stream, handshake.protocol_version, &config, state).await
        }
        NextState::Transfer => {
            disconnect_login(&mut stream, "Server transfer is not implemented yet.").await
        }
    }
}

async fn handle_status(
    stream: &mut Connection,
    config: &ServerConfig,
    state: &dyn ServerApi,
    _requested_protocol: i32,
) -> anyhow::Result<()> {
    let request = stream.read_frame().await?;
    if request.as_slice() != [0] {
        bail!("expected status request packet");
    }

    let response = StatusResponse {
        version: StatusVersion {
            name: format!("Minecraft {MINECRAFT_VERSION} / Carbon {}", state.version()),
            protocol: PROTOCOL_VERSION,
        },
        players: StatusPlayers {
            max: config.max_players,
            online: u32::try_from(state.players().len()).unwrap_or(u32::MAX),
            sample: Vec::new(),
        },
        description: StatusDescription {
            text: config.motd.clone(),
        },
    };
    let json = serde_json::to_string(&response)?;
    stream
        .write_all(&frame_packet(0, &encode_string(&json)))
        .await?;

    let ping = stream.read_frame().await?;
    let (packet_id, consumed) = decode_varint(&ping)?;
    if packet_id != 1 || ping.len() != consumed + 8 {
        bail!("expected ping packet");
    }
    let timestamp = i64::from_be_bytes(ping[consumed..].try_into()?);
    let mut payload = BytesMut::with_capacity(8);
    payload.put_i64(timestamp);
    stream.write_all(&frame_packet(1, &payload)).await?;
    Ok(())
}

async fn handle_login(
    stream: &mut Connection,
    client_protocol: i32,
    config: &ServerConfig,
    state: Arc<dyn ServerApi>,
) -> anyhow::Result<()> {
    if client_protocol != PROTOCOL_VERSION {
        return disconnect_login(
            stream,
            &format!(
                "Carbon currently targets Minecraft {MINECRAFT_VERSION} (protocol {PROTOCOL_VERSION}); your client uses protocol {client_protocol}."
            ),
        )
        .await;
    }

    let packet = stream.read_frame().await?;
    let login = decode_login_start(&packet)?;
    debug!(
        username = %login.username,
        player_id = %format_player_id(login.player_id),
        "validated Minecraft 26.2 login start"
    );

    if config.online_mode {
        return disconnect_login(
            stream,
            "Carbon parsed your Minecraft 26.2 login. Secure online-mode authentication is not implemented yet; use offline mode only on a trusted development network.",
        )
        .await;
    }

    if let Some(reason) = login_denial(config, state.as_ref(), &login.username) {
        return disconnect_login(stream, &reason).await;
    }

    let profile_id = offline_player_id(&login.username);
    let mut connection_state = ConnectionState::Login;
    let session_id = *uuid::Uuid::new_v4().as_bytes();
    debug!(
        username = %login.username,
        state = ?connection_state,
        "sending offline-mode Login Finished"
    );
    stream
        .write_all(&encode_login_finished(
            &login.username,
            profile_id,
            session_id,
        ))
        .await?;

    let acknowledgement = stream.read_frame().await?;
    decode_login_acknowledged(&acknowledgement)?;
    connection_state = ConnectionState::Configuration;
    debug!(username = %login.username, state = ?connection_state, "entering configuration state");
    handle_configuration(stream, &login.username).await?;
    connection_state = ConnectionState::Play;
    info!(
        username = %login.username,
        state = ?connection_state,
        "configuration completed; entering play state"
    );
    stream.enter_play();
    handle_play(stream, &login.username, profile_id, config, state).await
}

async fn handle_configuration(stream: &mut Connection, username: &str) -> anyhow::Result<()> {
    stream
        .write_all(&encode_configuration_brand("Carbon"))
        .await?;
    stream
        .write_all(&encode_update_enabled_features(&["minecraft:vanilla"]))
        .await?;

    let core_pack = KnownPack {
        namespace: "minecraft".into(),
        id: "core".into(),
        version: MINECRAFT_VERSION.into(),
    };
    stream
        .write_all(&encode_select_known_packs(std::slice::from_ref(&core_pack)))
        .await?;

    let selected_packs = loop {
        let packet = stream.read_frame().await?;
        let (packet_id, _) = decode_varint(&packet)?;
        match packet_id {
            // Vanilla sends Client Information and its brand as configuration
            // begins. Both frames have already passed Carbon's size bound.
            0 => debug!(%username, "received configuration client information"),
            2 => debug!(%username, "received configuration client brand"),
            7 => break decode_select_known_packs(&packet)?,
            unexpected => {
                bail!("unexpected configuration packet {unexpected} before known-pack selection");
            }
        }
    };
    if selected_packs != [core_pack] {
        bail!("Minecraft 26.2 core data pack was not accepted by the client");
    }
    debug!(
        %username,
        selected_pack_count = selected_packs.len(),
        "validated configuration data-pack selection"
    );

    stream.write_all(CONFIGURATION_SNAPSHOT).await?;
    let acknowledgement = stream.read_frame().await?;
    decode_finish_configuration(&acknowledgement)?;
    Ok(())
}

async fn handle_play(
    stream: &mut Connection,
    username: &str,
    profile_id: [u8; 16],
    config: &ServerConfig,
    state: Arc<dyn ServerApi>,
) -> anyhow::Result<()> {
    let world = state
        .worlds()
        .into_iter()
        .next()
        .context("Carbon has no world available for login")?;
    let entity_id = (i32::from_be_bytes(profile_id[..4].try_into()?) & i32::MAX).max(1);

    stream
        .write_all(&encode_play_login(
            entity_id,
            config.max_players,
            config.view_distance,
            world.seed,
        ))
        .await?;
    stream.write_all(&encode_change_difficulty()).await?;
    stream.write_all(&encode_player_abilities()).await?;
    stream
        .write_all(&encode_player_info(username, profile_id))
        .await?;
    let advertised_commands = visible_play_commands(state.as_ref(), username);
    stream
        .write_all(&encode_commands(&advertised_commands))
        .await?;
    stream.write_all(&encode_player_position(1)).await?;
    stream.write_all(&encode_default_spawn_position()).await?;
    stream.write_all(&encode_level_chunks_load_start()).await?;
    let initial_block_revision = state
        .block_changes_since(0)
        .last()
        .map_or(0, |change| change.revision);
    let mut initial_center = ChunkPosition { x: -1, z: -1 };
    stream
        .write_all(&encode_chunk_cache_center(
            initial_center.x,
            initial_center.z,
        ))
        .await?;
    let mut loaded_chunks = HashSet::new();
    stream_visible_chunks(
        stream,
        state.as_ref(),
        DimensionKind::Overworld,
        initial_center,
        i32::from(config.view_distance),
        &mut loaded_chunks,
    )
    .await?;
    for mob in state.dimension_mobs(DimensionKind::Overworld) {
        stream.write_all(&encode_mob_spawn(&mob)).await?;
        stream
            .write_all(&encode_entity_flags(mob.entity_id, mob.on_fire))
            .await?;
        stream
            .write_all(&encode_rotate_head(mob.entity_id, mob.yaw))
            .await?;
    }
    for item in state.dimension_items(DimensionKind::Overworld) {
        stream.write_all(&encode_item_spawn(&item)).await?;
        stream.write_all(&encode_item_data(&item)).await?;
    }

    let player_id = Uuid::from_bytes(profile_id);
    let added = state.add_player(PlayerSnapshot {
        id: player_id,
        name: username.to_owned(),
        world: world.name,
        position: BlockPosition {
            x: -8,
            y: 65,
            z: -9,
        },
        game_mode: GameMode::Survival,
    });
    if !added {
        bail!("a player with UUID {player_id} is already connected");
    }
    let _registration = PlayerRegistration {
        state: Arc::clone(&state),
        id: player_id,
    };
    let joined_player = state
        .players()
        .into_iter()
        .find(|player| player.id == player_id)
        .context("joined player disappeared")?;
    let joined_dimension = player_dimension(&joined_player);
    let joined_center = ChunkPosition {
        x: joined_player.position.x.div_euclid(16),
        z: joined_player.position.z.div_euclid(16),
    };
    if joined_dimension != DimensionKind::Overworld {
        stream
            .write_all(&encode_dimension_respawn(
                dimension_seed(world.seed, joined_dimension),
                joined_dimension.type_id(),
                joined_dimension.name(),
                dimension_sea_level(joined_dimension),
            ))
            .await?;
        loaded_chunks.clear();
    }
    if joined_dimension != DimensionKind::Overworld
        || joined_player.position
            != (BlockPosition {
                x: -8,
                y: 65,
                z: -9,
            })
    {
        stream
            .write_all(&encode_player_position_at(
                2,
                [
                    f64::from(joined_player.position.x) + 0.5,
                    f64::from(joined_player.position.y),
                    f64::from(joined_player.position.z) + 0.5,
                ],
            ))
            .await?;
        stream
            .write_all(&encode_chunk_cache_center(joined_center.x, joined_center.z))
            .await?;
        stream.write_all(&encode_level_chunks_load_start()).await?;
        stream_visible_chunks(
            stream,
            state.as_ref(),
            joined_dimension,
            joined_center,
            i32::from(config.view_distance),
            &mut loaded_chunks,
        )
        .await?;
        initial_center = joined_center;
    }
    if joined_dimension != DimensionKind::Overworld {
        for mob in state.dimension_mobs(joined_dimension) {
            stream.write_all(&encode_mob_spawn(&mob)).await?;
            stream
                .write_all(&encode_entity_flags(mob.entity_id, mob.on_fire))
                .await?;
            stream
                .write_all(&encode_rotate_head(mob.entity_id, mob.yaw))
                .await?;
        }
        for item in state.dimension_items(joined_dimension) {
            stream.write_all(&encode_item_spawn(&item)).await?;
            stream.write_all(&encode_item_data(&item)).await?;
        }
    }
    if state
        .inventory(player_id)
        .is_some_and(|inventory| inventory.slots.iter().all(Option::is_none))
    {
        state.give_item(player_id, ItemKind::OakLog, 8);
        state.give_item(player_id, ItemKind::OakPlanks, 16);
        state.give_item(player_id, ItemKind::Apple, 3);
    }
    let joined_vitals = state.vitals(player_id).unwrap_or_default();
    stream
        .write_all(&encode_set_health(
            joined_vitals.health,
            i32::from(joined_vitals.food),
            joined_vitals.saturation,
        ))
        .await?;
    info!(%username, %player_id, "player joined Carbon's starter world");

    let result = run_play_session(
        stream,
        username,
        player_id,
        state.as_ref(),
        PlaySessionState {
            world_seed: world.seed,
            dimension: joined_dimension,
            last_block_revision: initial_block_revision,
            view_distance: i32::from(config.view_distance),
            chunk_center: initial_center,
            loaded_chunks,
            advertised_commands,
        },
    )
    .await;
    drop(_registration);
    info!(%username, %player_id, "player left Carbon's starter world");
    result
}

struct PlayerRegistration {
    state: Arc<dyn ServerApi>,
    id: Uuid,
}
impl Drop for PlayerRegistration {
    fn drop(&mut self) {
        self.state.remove_player(self.id);
    }
}

struct PlaySessionState {
    world_seed: i64,
    dimension: DimensionKind,
    last_block_revision: u64,
    view_distance: i32,
    chunk_center: ChunkPosition,
    loaded_chunks: HashSet<ChunkPosition>,
    advertised_commands: Vec<&'static str>,
}

// Only advertise commands that the play dispatcher implements. Administrative
// console commands need argument schemas and routing before appearing here.
const PLAY_COMMANDS: &[(&str, Option<&str>)] = &[
    ("craft_planks", None),
    ("craft_sticks", None),
    ("craft_table", None),
    ("craft_pickaxe", None),
    ("craft_axe", None),
    ("craft_shovel", None),
    ("craft_sword", None),
    ("craft_shield", None),
    ("equip_iron", Some("carbon.command.equip_iron")),
    ("equip_shield", Some("carbon.command.equip_shield")),
    (
        "enchant_sharpness",
        Some("carbon.command.enchant_sharpness"),
    ),
    ("dimension_overworld", None),
    ("dimension_nether", None),
    ("dimension_end", None),
    ("say", Some("carbon.command.say")),
];

fn play_command_allowed(state: &dyn ServerApi, username: &str, label: &str) -> bool {
    // Preserve recipe aliases and their existing validation by ServerApi::craft.
    if label.starts_with("craft_") {
        return true;
    }
    PLAY_COMMANDS.iter().any(|(name, permission)| {
        *name == label
            && permission.map_or(true, |permission| {
                state.has_permission(username, permission)
            })
    })
}

fn visible_play_commands(state: &dyn ServerApi, username: &str) -> Vec<&'static str> {
    PLAY_COMMANDS
        .iter()
        .filter_map(|(name, _)| play_command_allowed(state, username, name).then_some(*name))
        .collect()
}

async fn refresh_play_commands(
    stream: &mut Connection,
    state: &dyn ServerApi,
    username: &str,
    advertised: &mut Vec<&'static str>,
) -> anyhow::Result<bool> {
    let current = visible_play_commands(state, username);
    if current == *advertised {
        return Ok(false);
    }
    stream.write_all(&encode_commands(&current)).await?;
    *advertised = current;
    Ok(true)
}

async fn transition_dimension(
    stream: &mut Connection,
    state: &dyn ServerApi,
    player_id: Uuid,
    session: &mut PlaySessionState,
    teleport_id: &mut i32,
    dimension: DimensionKind,
) -> anyhow::Result<Option<i32>> {
    if dimension == session.dimension || !state.change_player_dimension(player_id, dimension) {
        return Ok(None);
    }
    session.dimension = dimension;
    session.loaded_chunks.clear();
    session.last_block_revision = state
        .block_changes_since(0)
        .last()
        .map_or(0, |change| change.revision);
    stream
        .write_all(&encode_dimension_respawn(
            dimension_seed(session.world_seed, dimension),
            dimension.type_id(),
            dimension.name(),
            dimension_sea_level(dimension),
        ))
        .await?;
    let player = state
        .players()
        .into_iter()
        .find(|player| player.id == player_id)
        .context("dimension traveler disappeared")?;
    stream
        .write_all(&encode_player_position_at(
            *teleport_id,
            [
                f64::from(player.position.x) + 0.5,
                f64::from(player.position.y),
                f64::from(player.position.z) + 0.5,
            ],
        ))
        .await?;
    *teleport_id = teleport_id.saturating_add(1);
    let center = ChunkPosition {
        x: player.position.x.div_euclid(16),
        z: player.position.z.div_euclid(16),
    };
    stream
        .write_all(&encode_chunk_cache_center(center.x, center.z))
        .await?;
    stream.write_all(&encode_level_chunks_load_start()).await?;
    stream_visible_chunks(
        stream,
        state,
        dimension,
        center,
        session.view_distance,
        &mut session.loaded_chunks,
    )
    .await?;
    session.chunk_center = center;
    Ok(Some(player.position.y))
}

async fn run_play_session(
    stream: &mut Connection,
    username: &str,
    player_id: Uuid,
    state: &dyn ServerApi,
    mut session: PlaySessionState,
) -> anyhow::Result<()> {
    let mut keep_alive = time::interval(Duration::from_secs(10));
    keep_alive.tick().await;
    let mut entity_updates = time::interval(Duration::from_millis(50));
    entity_updates.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
    entity_updates.tick().await;
    let initial_mobs = state.dimension_mobs(session.dimension);
    let mut last_sent: HashMap<i32, [f64; 3]> = initial_mobs
        .iter()
        .map(|mob| (mob.entity_id, mob_position(mob)))
        .collect();
    let mut last_health: HashMap<i32, f32> = initial_mobs
        .iter()
        .map(|mob| (mob.entity_id, mob.health))
        .collect();
    let mut last_fire: HashMap<i32, bool> = initial_mobs
        .iter()
        .map(|mob| (mob.entity_id, mob.on_fire))
        .collect();
    let mut last_items: HashMap<i32, [f64; 3]> = state
        .dimension_items(session.dimension)
        .iter()
        .map(|item| (item.entity_id, item_position(item)))
        .collect();
    let mut last_players = HashMap::<Uuid, (i32, PlayerTransform)>::new();
    let mut last_player_equipment = HashMap::<Uuid, PlayerEquipment>::new();
    let mut last_local_equipment = None;
    let mut last_player_event_revision = state
        .player_events_since(0)
        .last()
        .map_or(0, |event| event.revision);
    let mut last_player_impulse_revision = state
        .player_impulses_since(0)
        .last()
        .map_or(0, |impulse| impulse.revision);
    let mut last_chat_revision = state
        .chat_messages_since(0)
        .last()
        .map_or(0, |message| message.revision);
    let mut last_disconnect_revision = state
        .disconnects_since(0)
        .last()
        .map_or(0, |disconnect| disconnect.revision);
    let mut mining_started = HashMap::<BlockPosition, u64>::new();
    let mut teleport_id: i32 = 2;
    let mut awaiting_safe_position = false;
    let mut inventory_revision = u64::MAX;
    let mut vitals_revision = u64::MAX;
    let mut last_effect_revisions = HashMap::<StatusEffectKind, u64>::new();
    let mut selected_slot = 0_usize;
    let mut inventory_cursor = InventoryCursor::default();
    let mut crafting_grid = [InventoryCursor::default(); 4];
    let mut crafting_table_grid: Option<[InventoryCursor; 9]> = None;
    let mut open_furnace: Option<BlockPosition> = None;
    let mut furnace_revision = u64::MAX;
    let mut open_chest: Option<BlockPosition> = None;
    let mut chest_revision = u64::MAX;
    let mut menu_state_id = 0_i32;
    let mut drag_slots = HashSet::<PlayerInventorySlot>::new();
    let mut drag_button = None;
    let mut highest_fall_y = 65.0_f64;
    let mut airborne = false;
    let mut last_attack_tick = state.current_tick().saturating_sub(25);
    let mut last_chat_tick = None;
    let mut portal_cooldown_until = 0_u64;
    let result: anyhow::Result<()> = async {
        loop {
        tokio::select! {
            packet = stream.read_frame() => {
                let packet = packet?;
                let (packet_id, _) = decode_varint(&packet)?;
                if let Some(message) = decode_chat_message(&packet)? {
                    let tick = state.current_tick();
                    let rate_limit_ready = match last_chat_tick {
                        Some(last) => tick.saturating_sub(last) >= 10,
                        None => true,
                    };
                    if rate_limit_ready
                        && state.publish_chat(username, &message)
                    {
                        last_chat_tick = Some(tick);
                    }
                }
                if let Some(command) = decode_chat_command(&packet)? {
                    let label = command.split_whitespace().next().unwrap_or("");
                    if !play_command_allowed(state, username, label) {
                        if PLAY_COMMANDS.iter().any(|(name, _)| *name == label) {
                            if let Err(error) = state.record_moderation(
                                username, &format!("denied:{label}"), None, "Permission denied.",
                            ) {
                                tracing::warn!(%error, "could not write moderation audit record");
                            }
                            stream.write_all(&encode_system_chat(
                                "You do not have permission to use this command.", false,
                            )).await?;
                        }
                        continue;
                    }
                    if let Some(recipe) = command.strip_prefix("craft_") {
                        state.craft(player_id, recipe);
                    } else if command == "equip_iron" {
                        state.set_player_armor(player_id, iron_armor());
                    } else if command == "equip_shield" {
                        state.set_offhand(
                            player_id,
                            Some(ItemStack {
                                kind: ItemKind::Shield,
                                count: 1,
                                damage: 0,
                            }),
                        );
                    } else if command == "enchant_sharpness" {
                        state.enchant_selected_weapon(player_id, 1);
                    } else if let Some(message) = command.strip_prefix("say ") {
                        if !message.is_empty()
                            && message.chars().count() <= 256
                            && !message.chars().any(char::is_control)
                        {
                            state.broadcast(&format!("[Server] {message}"));
                        }
                    } else if let Some(dimension) = command_dimension(&command) {
                        if let Some(y) = transition_dimension(
                            stream,
                            state,
                            player_id,
                            &mut session,
                            &mut teleport_id,
                            dimension,
                        )
                        .await?
                        {
                            last_sent.clear();
                            last_health.clear();
                            last_fire.clear();
                            last_items.clear();
                            if !last_players.is_empty() {
                                let profile_ids: Vec<_> =
                                    last_players.keys().map(|id| *id.as_bytes()).collect();
                                stream
                                    .write_all(&encode_player_info_remove(&profile_ids))
                                    .await?;
                                last_players.clear();
                                last_player_equipment.clear();
                            }
                            airborne = false;
                            highest_fall_y = f64::from(y);
                        }
                    }
                }
                if let Some(click) = decode_container_click(&packet)? {
                    if click.container_id == 0 {
                        if click.input == 0
                            && matches!(click.button, 0 | 1)
                            && (1..=4).contains(&click.slot)
                        {
                            let index = usize::try_from(click.slot - 1).unwrap_or(0);
                            inventory_cursor = click_local_slot(
                                &mut crafting_grid[index],
                                inventory_cursor,
                                click.button == 1,
                            );
                            menu_state_id = menu_state_id.wrapping_add(1);
                        } else if click.input == 0
                            && click.slot == 0
                            && matches!(click.button, 0 | 1)
                        {
                            if craft_from_grid(&mut crafting_grid, &mut inventory_cursor) {
                                menu_state_id = menu_state_id.wrapping_add(1);
                            }
                        } else {
                            inventory_cursor = handle_inventory_gesture(
                                state,
                                player_id,
                                session.dimension,
                                InventoryGesture {
                                    menu_slot: click.slot,
                                    button: click.button,
                                    input: click.input,
                                    crafting_table: false,
                                },
                                inventory_cursor,
                                &mut drag_slots,
                                &mut drag_button,
                            );
                        }
                    } else if click.container_id == 1 {
                        if let Some(grid) = crafting_table_grid.as_mut() {
                            if click.input == 0
                                && matches!(click.button, 0 | 1)
                                && (1..=9).contains(&click.slot)
                            {
                                let index = usize::try_from(click.slot - 1).unwrap_or(0);
                                inventory_cursor = click_local_slot(
                                    &mut grid[index],
                                    inventory_cursor,
                                    click.button == 1,
                                );
                                menu_state_id = menu_state_id.wrapping_add(1);
                            } else if click.slot == 0 && click.input == 0 {
                                if craft_from_table(grid, &mut inventory_cursor) {
                                    menu_state_id = menu_state_id.wrapping_add(1);
                                }
                            } else if click.slot == 0 && click.input == 1 {
                                if shift_craft_from_table(state, player_id, grid) {
                                    menu_state_id = menu_state_id.wrapping_add(1);
                                }
                            } else if click.input == 1 && (1..=9).contains(&click.slot) {
                                let index = usize::try_from(click.slot - 1).unwrap_or(0);
                                if grid[index].stack.is_some_and(|stack| {
                                    inventory_capacity(state, player_id, stack)
                                        >= u16::from(stack.count)
                                }) {
                                    let carried = click_local_slot(
                                        &mut grid[index],
                                        InventoryCursor::default(),
                                        false,
                                    );
                                    let _ = return_inventory_cursor(
                                        state,
                                        player_id,
                                        session.dimension,
                                        carried,
                                    );
                                    menu_state_id = menu_state_id.wrapping_add(1);
                                }
                            } else {
                                inventory_cursor = handle_inventory_gesture(
                                    state,
                                    player_id,
                                    session.dimension,
                                    InventoryGesture {
                                        menu_slot: click.slot,
                                        button: click.button,
                                        input: click.input,
                                        crafting_table: true,
                                    },
                                    inventory_cursor,
                                    &mut drag_slots,
                                    &mut drag_button,
                                );
                            }
                        }
                    } else if click.container_id == 2 {
                        if let Some(position) = open_furnace {
                            if click.input == 0 && matches!(click.button, 0 | 1) {
                                let furnace_slot = match click.slot {
                                    0 => Some(FurnaceSlot::Input),
                                    1 => Some(FurnaceSlot::Fuel),
                                    2 => Some(FurnaceSlot::Output),
                                    _ => None,
                                };
                                if let Some(slot) = furnace_slot {
                                    inventory_cursor = state.click_furnace_slot(
                                        session.dimension, position, slot, inventory_cursor, click.button == 1,
                                    ).unwrap_or(inventory_cursor);
                                } else if let Some(slot) = furnace_menu_slot(click.slot) {
                                    inventory_cursor = state.click_player_inventory_slot(
                                        player_id, slot, inventory_cursor, click.button == 1,
                                    ).unwrap_or(inventory_cursor);
                                }
                            }
                            menu_state_id = menu_state_id.wrapping_add(1);
                        }
                    } else if click.container_id == 3 {
                        if let Some(position) = open_chest {
                            if click.input == 0 && matches!(click.button, 0 | 1) {
                                if (0..=26).contains(&click.slot) {
                                    inventory_cursor = state
                                        .click_chest_slot(
                                            session.dimension,
                                            position,
                                            u8::try_from(click.slot).unwrap_or(0),
                                            inventory_cursor,
                                            click.button == 1,
                                        )
                                        .unwrap_or(inventory_cursor);
                                } else if let Some(slot) = chest_menu_slot(click.slot) {
                                    inventory_cursor = state
                                        .click_player_inventory_slot(
                                            player_id,
                                            slot,
                                            inventory_cursor,
                                            click.button == 1,
                                        )
                                        .unwrap_or(inventory_cursor);
                                }
                            } else if click.input == 1 && inventory_cursor.stack.is_none() {
                                let _ = shift_chest_slot(
                                    state,
                                    player_id,
                                    session.dimension,
                                    position,
                                    click.slot,
                                );
                            }
                            menu_state_id = menu_state_id.wrapping_add(1);
                        }
                    }
                    // The client's hashes are predictions, never authority. Sending every
                    // raw slot and the cursor also rejects unsupported click modes safely.
                    sync_player_inventory(stream, state, player_id, inventory_cursor).await?;
                    if let Some(position) = open_chest {
                        if let Some(chest) = state.chest(session.dimension, position) {
                            sync_chest(stream, chest, menu_state_id).await?;
                            chest_revision = chest.revision;
                        }
                    } else if let Some(position) = open_furnace {
                        if let Some(furnace) = state.furnace(session.dimension, position) {
                            sync_furnace(stream, furnace, menu_state_id).await?;
                            furnace_revision = furnace.revision;
                        }
                    } else if let Some(grid) = &crafting_table_grid {
                        sync_crafting_table_grid(stream, grid, menu_state_id).await?;
                    } else {
                        sync_crafting_grid(stream, &crafting_grid, menu_state_id).await?;
                    }
                    inventory_revision = state.inventory(player_id).map_or(u64::MAX, |value| value.revision);
                    last_local_equipment = state.player_equipment(player_id);
                }
                if let Some(container_id) = decode_container_close(&packet)? {
                    if container_id == 0 {
                        inventory_cursor = return_crafting_grid(
                            state,
                            player_id,
                            session.dimension,
                            &mut crafting_grid,
                            inventory_cursor,
                        );
                    } else if container_id == 1 {
                        if let Some(mut grid) = crafting_table_grid.take() {
                            inventory_cursor = return_crafting_table_grid(
                                state,
                                player_id,
                                session.dimension,
                                &mut grid,
                                inventory_cursor,
                            );
                        }
                    } else if container_id == 2 {
                        open_furnace = None;
                    } else if container_id == 3 {
                        open_chest = None;
                    } else {
                        continue;
                    }
                    inventory_cursor = return_inventory_cursor(
                        state,
                        player_id,
                        session.dimension,
                        inventory_cursor,
                    );
                    sync_player_inventory(stream, state, player_id, inventory_cursor).await?;
                    inventory_revision = state
                        .inventory(player_id)
                        .map_or(u64::MAX, |value| value.revision);
                    last_local_equipment = state.player_equipment(player_id);
                }
                if let Some(movement) = decode_player_movement(&packet)? {
                    if requires_spawn_rescue(movement.x, movement.y, movement.z) {
                        let rescue_x: i32 = -8;
                        let rescue_z: i32 = -9;
                        let surface = state.ensure_dimension_chunk_surface(
                            session.dimension,
                            ChunkPosition { x: -1, z: -1 },
                        );
                        let rescue_index = usize::try_from(
                            rescue_z.rem_euclid(16) * 16 + rescue_x.rem_euclid(16),
                        )
                        .unwrap_or(0);
                        let rescue_y = i32::from(surface[rescue_index]) + 1;
                        if !awaiting_safe_position {
                            stream
                                .write_all(&encode_player_position_at(
                                    teleport_id,
                                    [
                                        f64::from(rescue_x) + 0.5,
                                        f64::from(rescue_y),
                                        f64::from(rescue_z) + 0.5,
                                    ],
                                ))
                                .await?;
                            teleport_id = teleport_id.saturating_add(1);
                            awaiting_safe_position = true;
                            info!(%username, "rescued player from outside the loaded starter area");
                        }
                        state.update_player_position(
                            player_id,
                            BlockPosition { x: rescue_x, y: rescue_y, z: rescue_z },
                        );
                    } else {
                        awaiting_safe_position = false;
                        if movement.on_ground {
                            if airborne {
                                let fall_distance = highest_fall_y - movement.y;
                                if fall_distance > 3.0 {
                                    state.damage_player(
                                        player_id,
                                        (fall_distance - 3.0).floor() as f32,
                                    );
                                }
                            }
                            airborne = false;
                            highest_fall_y = movement.y;
                        } else {
                            if !airborne {
                                highest_fall_y = movement.y;
                            } else {
                                highest_fall_y = highest_fall_y.max(movement.y);
                            }
                            airborne = true;
                        }
                        let position = BlockPosition {
                            x: floor_to_i32(movement.x),
                            y: floor_to_i32(movement.y),
                            z: floor_to_i32(movement.z),
                        };
                        let previous = state
                            .player_transforms()
                            .into_iter()
                            .find(|transform| transform.id == player_id);
                        state.set_player_falling(
                            player_id,
                            !movement.on_ground
                                && previous.is_some_and(|transform| {
                                    movement.y < transform.position.y - 0.001
                                }),
                        );
                        state.update_player_transform(PlayerTransform {
                            id: player_id,
                            position: EntityPosition {
                                x: movement.x,
                                y: movement.y,
                                z: movement.z,
                            },
                            yaw: movement
                                .yaw
                                .or(previous.map(|transform| transform.yaw))
                                .unwrap_or(0.0),
                            pitch: movement
                                .pitch
                                .or(previous.map(|transform| transform.pitch))
                                .unwrap_or(0.0),
                            on_ground: movement.on_ground,
                            revision: previous.map_or(0, |transform| transform.revision),
                        });
                        let destination = portal_destination(state, session.dimension, position);
                        let tick = state.current_tick();
                        if destination.is_some() && tick >= portal_cooldown_until {
                            if let Some(destination) = destination {
                                if let Some(y) = transition_dimension(
                                    stream,
                                    state,
                                    player_id,
                                    &mut session,
                                    &mut teleport_id,
                                    destination,
                                )
                                .await?
                                {
                                    last_sent.clear();
                                    last_health.clear();
                                    last_fire.clear();
                                    last_items.clear();
                                    if !last_players.is_empty() {
                                        let profile_ids: Vec<_> = last_players
                                            .keys()
                                            .map(|id| *id.as_bytes())
                                            .collect();
                                        stream
                                            .write_all(&encode_player_info_remove(&profile_ids))
                                            .await?;
                                        last_players.clear();
                                        last_player_equipment.clear();
                                    }
                                    airborne = false;
                                    highest_fall_y = f64::from(y);
                                    portal_cooldown_until = tick.saturating_add(100);
                                    continue;
                                }
                            }
                        }
                        let new_center = ChunkPosition {
                            x: position.x.div_euclid(16),
                            z: position.z.div_euclid(16),
                        };
                        if new_center != session.chunk_center {
                            stream
                                .write_all(&encode_chunk_cache_center(new_center.x, new_center.z))
                                .await?;
                            let sent = stream_visible_chunks(
                                stream,
                                state,
                                session.dimension,
                                new_center,
                                session.view_distance,
                                &mut session.loaded_chunks,
                            )
                            .await?;
                            session.chunk_center = new_center;
                            debug!(%username, x = new_center.x, z = new_center.z, sent, "streamed player-centered chunks");
                        }
                    }
                }
                if let Some(rotation) = decode_player_rotation(&packet)? {
                    if let Some(mut transform) = state
                        .player_transforms()
                        .into_iter()
                        .find(|transform| transform.id == player_id)
                    {
                        transform.yaw = rotation.yaw;
                        transform.pitch = rotation.pitch;
                        transform.on_ground = rotation.on_ground;
                        state.update_player_transform(transform);
                    }
                }
                if let Some(hand) = decode_swing(&packet)? {
                    if hand == 0 || hand == 1 {
                        state.swing_player(player_id, hand == 1);
                    }
                }
                if let Some(action) = decode_player_action(&packet)? {
                    if action.action == 5 {
                        state.set_player_blocking(player_id, false);
                    }
                    let position = BlockPosition { x: action.x, y: action.y, z: action.z };
                    match action.action {
                        0 if player_can_reach(state, player_id, position) => {
                            if state.dimension_block_at(session.dimension, position).is_surface_plant() {
                                // Zero-hardness plants break on START; clients need not
                                // send a later FINISH action for these blocks.
                                state.set_dimension_block(session.dimension, position, BlockKind::Air);
                                stream.write_all(&encode_block_update(action.x, action.y, action.z, 0)).await?;
                            } else {
                                mining_started.insert(position, state.current_tick());
                            }
                        }
                        1 => {
                            mining_started.remove(&position);
                        }
                        2 if player_can_reach(state, player_id, position) => {
                            let previous = state.dimension_block_at(session.dimension, position);
                            let held = state
                                .inventory(player_id)
                                .and_then(|inventory| inventory.slots[selected_slot]);
                            let elapsed = mining_started
                                .remove(&position)
                                .map_or(u64::MAX, |started| state.current_tick().saturating_sub(started));
                            if previous != BlockKind::Air
                                && previous != BlockKind::Bedrock
                                && elapsed >= mining_ticks(previous, held.map(|stack| stack.kind))
                            {
                                if previous == BlockKind::Furnace {
                                    for stack in state.take_furnace_contents(session.dimension, position) {
                                        state.drop_dimension_item(session.dimension, stack, position);
                                    }
                                }
                                if previous == BlockKind::Chest {
                                    for stack in state.take_chest_contents(session.dimension, position) {
                                        state.drop_dimension_item(session.dimension, stack, position);
                                    }
                                }
                                state.set_dimension_block(session.dimension, position, BlockKind::Air);
                                if let Some(stack) = block_drop_at(
                                    previous,
                                    held.map(|stack| stack.kind),
                                    position,
                                    state.current_tick(),
                                ) {
                                    state.drop_dimension_item(session.dimension, stack, position);
                                }
                                if held.is_some_and(|stack| is_tool(stack.kind)) {
                                    state.damage_item(player_id, selected_slot, 1);
                                }
                                stream
                                    .write_all(&encode_block_update(action.x, action.y, action.z, 0))
                                    .await?;
                            } else {
                                stream
                                    .write_all(&encode_block_update(
                                        action.x,
                                        action.y,
                                        action.z,
                                        block_state_id(previous),
                                    ))
                                    .await?;
                            }
                        }
                        _ => {}
                    }
                    stream.write_all(&encode_block_changed_ack(action.sequence)).await?;
                }
                if let Some(action) = decode_player_command(&packet)? {
                    match action {
                        3 => {
                            state.set_player_sprinting(player_id, true);
                        }
                        4 => {
                            state.set_player_sprinting(player_id, false);
                        }
                        _ => {}
                    }
                }
                if let Some(slot) = decode_set_carried_item(&packet)? {
                    if slot < 9 {
                        selected_slot = usize::from(slot);
                        state.set_selected_slot(player_id, u8::try_from(slot).unwrap_or(0));
                    }
                }
                if let Some((hand, sequence)) = decode_use_item(&packet)? {
                    if hand == 0 {
                        if !state.consume_milk(player_id, selected_slot) {
                            state.consume_food(player_id, selected_slot);
                        }
                    } else if hand == 1
                        && state.player_equipment(player_id).is_some_and(|equipment| {
                            equipment
                                .off_hand
                                .is_some_and(|stack| stack.kind == ItemKind::Shield)
                        })
                    {
                        state.set_player_blocking(player_id, true);
                    }
                    stream.write_all(&encode_block_changed_ack(sequence)).await?;
                }
                if let Some((entity_id, hand)) = decode_interact_entity(&packet)? {
                    if hand == 0 {
                        if let Some(cow) = state
                            .dimension_mobs(session.dimension)
                            .into_iter()
                            .find(|mob| mob.entity_id == entity_id && mob.kind == MobKind::Cow)
                        {
                            if player_can_reach_entity(state, player_id, &cow) {
                                state.fill_milk_bucket(player_id, selected_slot);
                            }
                        }
                    }
                }
                if let Some(use_item) = decode_use_item_on(&packet)? {
                    if use_item.hand == 0 {
                        let clicked = BlockPosition {
                            x: use_item.x,
                            y: use_item.y,
                            z: use_item.z,
                        };
                        if player_can_reach(state, player_id, clicked) {
                            if state.dimension_block_at(session.dimension, clicked) == BlockKind::Chest {
                                crafting_table_grid = None;
                                open_furnace = None;
                                open_chest = Some(clicked);
                                menu_state_id = 0;
                                chest_revision = u64::MAX;
                                drag_slots.clear();
                                drag_button = None;
                                stream.write_all(&encode_open_chest_screen(3)).await?;
                                if let Some(chest) = state.chest(session.dimension, clicked) {
                                    sync_chest(stream, chest, menu_state_id).await?;
                                    chest_revision = chest.revision;
                                }
                            } else if state.dimension_block_at(session.dimension, clicked) == BlockKind::Furnace {
                                crafting_table_grid = None;
                                open_chest = None;
                                open_furnace = Some(clicked);
                                menu_state_id = 0;
                                furnace_revision = u64::MAX;
                                drag_slots.clear();
                                drag_button = None;
                                stream.write_all(&encode_open_furnace_screen(2)).await?;
                                if let Some(furnace) = state.furnace(session.dimension, clicked) {
                                    sync_furnace(stream, furnace, menu_state_id).await?;
                                    furnace_revision = furnace.revision;
                                }
                            } else if state.dimension_block_at(session.dimension, clicked)
                                == BlockKind::CraftingTable
                            {
                                open_furnace = None;
                                open_chest = None;
                                if let Some(mut previous) = crafting_table_grid.take() {
                                    inventory_cursor = return_crafting_table_grid(
                                        state,
                                        player_id,
                                        session.dimension,
                                        &mut previous,
                                        inventory_cursor,
                                    );
                                }
                                crafting_table_grid = Some([InventoryCursor::default(); 9]);
                                menu_state_id = 0;
                                drag_slots.clear();
                                drag_button = None;
                                stream.write_all(&encode_open_crafting_screen(1)).await?;
                                if let Some(grid) = &crafting_table_grid {
                                    sync_crafting_table_grid(stream, grid, menu_state_id).await?;
                                }
                            } else {
                                let target = if state.dimension_block_at(session.dimension, clicked).is_surface_plant() {
                                    clicked
                                } else {
                                    adjacent_position(clicked, use_item.face)
                                };
                                let held = state
                                    .inventory(player_id)
                                    .and_then(|inventory| inventory.slots[selected_slot]);
                                if held.is_some_and(|stack| stack.kind == ItemKind::FlintAndSteel)
                                    && state.ignite_nether_portal(session.dimension, target)
                                {
                                    state.damage_item(player_id, selected_slot, 1);
                                } else if state
                                    .dimension_block_at(session.dimension, target)
                                    .is_replaceable()
                                {
                                    if let Some(kind) = held.and_then(|stack| item_block(stack.kind)) {
                                        if state.set_dimension_block(session.dimension, target, kind)
                                            && state.take_item(player_id, selected_slot, 1)
                                        {
                                            stream
                                                .write_all(&encode_block_update(
                                                    target.x,
                                                    target.y,
                                                    target.z,
                                                    block_state_id(kind),
                                                ))
                                                .await?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    stream
                        .write_all(&encode_block_changed_ack(use_item.sequence))
                        .await?;
                }
                if let Some(entity_id) = decode_attack(&packet)? {
                    let tick = state.current_tick();
                    let equipment = state.player_equipment(player_id);
                    let charge = attack_charge(tick.saturating_sub(last_attack_tick), equipment);
                    let strength_bonus = state
                        .status_effects(player_id)
                        .into_iter()
                        .find(|effect| effect.kind == StatusEffectKind::Strength)
                        .map_or(0.0, |effect| 3.0 * f32::from(effect.amplifier + 1));
                    let base_damage = equipment.map_or(1.0, attack_damage) + strength_bonus;
                    let combat = state.player_combat_state(player_id).unwrap_or_default();
                    let sprinting = combat.sprinting;
                    let critical = charge >= 0.9 && combat.falling && !sprinting;
                    let damage = base_damage
                        * attack_damage_scale(charge)
                        * if critical { 1.5 } else { 1.0 };
                    let mut connected = false;
                    let mut landed_damage = false;
                    let mut sweep_center = None;
                    let mob = state
                        .dimension_mobs(session.dimension)
                        .into_iter()
                        .find(|mob| mob.entity_id == entity_id);
                    if let Some(mob) = mob {
                        if player_can_reach_entity(state, player_id, &mob) {
                            if let Some(damaged) = state.damage_dimension_mob(
                                session.dimension,
                                entity_id,
                                damage,
                            ) {
                                connected = true;
                                landed_damage = true;
                                sweep_center = Some((damaged.position, None, Some(entity_id)));
                                if damaged.health <= 0.0 {
                                    if let Some(stack) = mob_drop(&damaged, tick) {
                                        state.drop_dimension_item(
                                            session.dimension,
                                            stack,
                                            BlockPosition {
                                                x: floor_to_i32(damaged.position.x),
                                                y: floor_to_i32(damaged.position.y),
                                                z: floor_to_i32(damaged.position.z),
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    } else if let Some(target) = state.players().into_iter().find(|target| {
                        target.id != player_id
                            && player_entity_id(target.id) == entity_id
                            && player_dimension(target) == session.dimension
                    }) {
                        if player_can_reach_player(state, player_id, target.id) {
                            connected = true;
                            let damaged = state.attack_player(player_id, target.id, damage);
                            landed_damage = damaged;
                            sweep_center = state
                                .player_transforms()
                                .into_iter()
                                .find(|transform| transform.id == target.id)
                                .map(|transform| (transform.position, Some(target.id), None));
                            let fully_charged = charge >= 0.9;
                            let axe = equipment.is_some_and(|equipment| {
                                equipment
                                    .main_hand
                                    .is_some_and(|stack| is_axe(stack.kind))
                            });
                            if damaged {
                                let normal = 0.4 * f64::from(charge.max(0.2));
                                state.knockback_player(
                                    player_id,
                                    target.id,
                                    if sprinting && fully_charged { 0.8 } else { normal },
                                );
                            } else if axe && fully_charged {
                                state.disable_player_shield(target.id, 100);
                            }
                        }
                    }
                    if connected {
                        if landed_damage && critical {
                            state.critical_hit_player(player_id);
                            stream
                                .write_all(&encode_animate(player_entity_id(player_id), 4))
                                .await?;
                        }
                        let sweeping = landed_damage
                            && charge >= 0.9
                            && !sprinting
                            && !combat.falling
                            && equipment.is_some_and(|equipment| {
                                equipment
                                    .main_hand
                                    .is_some_and(|stack| is_sword(stack.kind))
                            });
                        if sweeping {
                            if let Some((center, excluded_player, excluded_mob)) = sweep_center {
                                apply_sweeping_attack(
                                    state,
                                    session.dimension,
                                    player_id,
                                    center,
                                    excluded_player,
                                    excluded_mob,
                                    tick,
                                );
                            }
                        }
                        if equipment.is_some_and(|equipment| {
                            equipment.main_hand.is_some_and(|stack| {
                                is_tool(stack.kind) || is_sword(stack.kind)
                            })
                        }) {
                            state.damage_item(player_id, selected_slot, 1);
                        }
                        if sprinting && charge >= 0.9 {
                            state.set_player_sprinting(player_id, false);
                        }
                        last_attack_tick = tick;
                    }
                }
                if decode_client_command(&packet)? == Some(0)
                    && state.vitals(player_id).is_some_and(|vitals| vitals.health <= 0.0)
                {
                    state.respawn_player(player_id);
                    session.dimension = DimensionKind::Overworld;
                    session.loaded_chunks.clear();
                    session.last_block_revision = state
                        .block_changes_since(0)
                        .last()
                        .map_or(0, |change| change.revision);
                    stream
                        .write_all(&encode_dimension_respawn(
                            dimension_seed(session.world_seed, DimensionKind::Overworld),
                            DimensionKind::Overworld.type_id(),
                            DimensionKind::Overworld.name(),
                            dimension_sea_level(DimensionKind::Overworld),
                        ))
                        .await?;
                    let position = state
                        .players()
                        .into_iter()
                        .find(|player| player.id == player_id)
                        .map_or(BlockPosition { x: -8, y: 65, z: -9 }, |player| player.position);
                    stream
                        .write_all(&encode_player_position_at(
                            teleport_id,
                            [
                                f64::from(position.x) + 0.5,
                                f64::from(position.y),
                                f64::from(position.z) + 0.5,
                            ],
                        ))
                        .await?;
                    teleport_id = teleport_id.saturating_add(1);
                    let center = ChunkPosition {
                        x: position.x.div_euclid(16),
                        z: position.z.div_euclid(16),
                    };
                    stream
                        .write_all(&encode_chunk_cache_center(center.x, center.z))
                        .await?;
                    stream.write_all(&encode_level_chunks_load_start()).await?;
                    stream_visible_chunks(
                        stream,
                        state,
                        DimensionKind::Overworld,
                        center,
                        session.view_distance,
                        &mut session.loaded_chunks,
                    )
                    .await?;
                    session.chunk_center = center;
                    airborne = false;
                    highest_fall_y = f64::from(position.y);
                }
                debug!(%username, packet_id, "received play packet");
            }
            _ = entity_updates.tick() => {
                refresh_play_commands(
                    stream, state, username, &mut session.advertised_commands,
                ).await?;
                for disconnect in state.disconnects_since(last_disconnect_revision) {
                    last_disconnect_revision = disconnect.revision;
                    if disconnect.player_id == player_id {
                        stream
                            .write_all(&encode_play_disconnect(&disconnect.reason))
                            .await?;
                        return Ok(());
                    }
                }
                for message in state.chat_messages_since(last_chat_revision) {
                    let rendered = message.sender.as_ref().map_or_else(
                        || message.text.clone(),
                        |sender| format!("<{sender}> {}", message.text),
                    );
                    stream
                        .write_all(&encode_system_chat(&rendered, false))
                        .await?;
                    last_chat_revision = message.revision;
                }
                if let Some(vitals) = state.vitals(player_id) {
                    if vitals.revision != vitals_revision {
                        stream
                            .write_all(&encode_set_health(
                                vitals.health,
                                i32::from(vitals.food),
                                vitals.saturation,
                            ))
                            .await?;
                        vitals_revision = vitals.revision;
                    }
                }
                let effects = state.status_effects(player_id);
                let current_effects = effects
                    .iter()
                    .map(|effect| (effect.kind, effect.revision))
                    .collect::<HashMap<_, _>>();
                for effect in &effects {
                    if last_effect_revisions.get(&effect.kind).copied() != Some(effect.revision) {
                        stream
                            .write_all(&encode_update_mob_effect(
                                player_entity_id(player_id),
                                effect.kind.protocol_id(),
                                effect.amplifier,
                                effect.remaining_ticks,
                            ))
                            .await?;
                    }
                }
                for kind in last_effect_revisions
                    .keys()
                    .filter(|kind| !current_effects.contains_key(kind))
                {
                    stream
                        .write_all(&encode_remove_mob_effect(
                            player_entity_id(player_id),
                            kind.protocol_id(),
                        ))
                        .await?;
                }
                last_effect_revisions = current_effects;
                for change in state.block_changes_since(session.last_block_revision) {
                    if change.dimension == session.dimension {
                        stream
                            .write_all(&encode_block_update(
                                change.position.x,
                                change.position.y,
                                change.position.z,
                                block_state_id(change.kind),
                            ))
                            .await?;
                    }
                    session.last_block_revision = change.revision;
                }
                if let Some(inventory) = state.inventory(player_id) {
                    if inventory.revision != inventory_revision {
                        for (slot, stack) in inventory.slots.iter().enumerate() {
                            let (count, item_id, damage) = stack.map_or((0, 0, 0), |stack| {
                                (stack.count, item_protocol_id(stack.kind), stack.damage)
                            });
                            stream
                                .write_all(&encode_set_player_inventory_with_glint(
                                    i32::try_from(slot).unwrap_or(0),
                                    count,
                                    item_id,
                                    damage,
                                    inventory.sharpness_levels[slot] > 0,
                                ))
                                .await?;
                        }
                        inventory_revision = inventory.revision;
                    }
                }
                if let Some(position) = open_furnace {
                    if let Some(furnace) = state.furnace(session.dimension, position) {
                        if furnace.revision != furnace_revision {
                            sync_furnace(stream, furnace, menu_state_id).await?;
                            furnace_revision = furnace.revision;
                        }
                    } else {
                        open_furnace = None;
                    }
                }
                if let Some(position) = open_chest {
                    if let Some(chest) = state.chest(session.dimension, position) {
                        if chest.revision != chest_revision {
                            sync_chest(stream, chest, menu_state_id).await?;
                            chest_revision = chest.revision;
                        }
                    } else {
                        open_chest = None;
                    }
                }
                if let Some(equipment) = state.player_equipment(player_id) {
                    if last_local_equipment != Some(equipment) {
                        sync_player_equipment_inventory(stream, equipment).await?;
                        stream
                            .write_all(&encode_player_equipment(
                                player_entity_id(player_id),
                                equipment,
                            ))
                            .await?;
                        last_local_equipment = Some(equipment);
                    }
                }
                let visible_players: HashMap<_, _> = state
                    .players()
                    .into_iter()
                    .filter(|player| {
                        player.id != player_id && player_dimension(player) == session.dimension
                    })
                    .map(|player| (player.id, player))
                    .collect();
                let player_transforms: HashMap<_, _> = state
                    .player_transforms()
                    .into_iter()
                    .map(|transform| (transform.id, transform))
                    .collect();
                let departed: Vec<_> = last_players
                    .keys()
                    .filter(|id| !visible_players.contains_key(id))
                    .copied()
                    .collect();
                if !departed.is_empty() {
                    let entity_ids: Vec<_> = departed
                        .iter()
                        .filter_map(|id| last_players.get(id).map(|known| known.0))
                        .collect();
                    let profile_ids: Vec<_> = departed.iter().map(|id| *id.as_bytes()).collect();
                    stream.write_all(&encode_remove_entities(&entity_ids)).await?;
                    stream
                        .write_all(&encode_player_info_remove(&profile_ids))
                        .await?;
                    for id in departed {
                        last_players.remove(&id);
                        last_player_equipment.remove(&id);
                    }
                }
                for player in visible_players.values() {
                    let transform = player_transforms.get(&player.id).copied().unwrap_or(
                        PlayerTransform {
                            id: player.id,
                            position: EntityPosition {
                                x: f64::from(player.position.x) + 0.5,
                                y: f64::from(player.position.y),
                                z: f64::from(player.position.z) + 0.5,
                            },
                            yaw: 0.0,
                            pitch: 0.0,
                            on_ground: true,
                            revision: 0,
                        },
                    );
                    if let Some((entity_id, previous)) = last_players.get_mut(&player.id) {
                        if *previous != transform {
                            let delta = [
                                transform.position.x - previous.position.x,
                                transform.position.y - previous.position.y,
                                transform.position.z - previous.position.z,
                            ];
                            if delta.iter().all(|value| value.abs() < 8.0) {
                                stream
                                    .write_all(&encode_move_entity_pos_rot(
                                        *entity_id,
                                        delta,
                                        transform.yaw,
                                        transform.pitch,
                                        transform.on_ground,
                                    ))
                                    .await?;
                                stream
                                    .write_all(&encode_rotate_head(*entity_id, transform.yaw))
                                    .await?;
                            } else {
                                stream
                                    .write_all(&encode_remove_entities(&[*entity_id]))
                                    .await?;
                                stream
                                    .write_all(&encode_remote_player_spawn(
                                        player,
                                        *entity_id,
                                        transform,
                                    ))
                                    .await?;
                            }
                            *previous = transform;
                        }
                    } else {
                        let entity_id = player_entity_id(player.id);
                        stream
                            .write_all(&encode_player_info(&player.name, *player.id.as_bytes()))
                            .await?;
                        stream
                            .write_all(&encode_remote_player_spawn(player, entity_id, transform))
                            .await?;
                        last_players.insert(player.id, (entity_id, transform));
                    }
                }
                for player in visible_players.values() {
                    let Some(equipment) = state.player_equipment(player.id) else {
                        continue;
                    };
                    if last_player_equipment.get(&player.id).copied() != Some(equipment) {
                        stream
                            .write_all(&encode_player_equipment(
                                player_entity_id(player.id),
                                equipment,
                            ))
                            .await?;
                        last_player_equipment.insert(player.id, equipment);
                    }
                }
                for event in state.player_events_since(last_player_event_revision) {
                    last_player_event_revision = last_player_event_revision.max(event.revision);
                    let Some((entity_id, _)) = last_players.get(&event.player_id) else {
                        continue;
                    };
                    let packet = match event.kind {
                        PlayerEventKind::SwingMainArm => encode_animate(*entity_id, 0),
                        PlayerEventKind::SwingOffHand => encode_animate(*entity_id, 3),
                        PlayerEventKind::Hurt => encode_entity_event(*entity_id, 2),
                        PlayerEventKind::Died => encode_entity_event(*entity_id, 3),
                        PlayerEventKind::CriticalHit => encode_animate(*entity_id, 4),
                    };
                    stream.write_all(&packet).await?;
                }
                for impulse in state.player_impulses_since(last_player_impulse_revision) {
                    last_player_impulse_revision =
                        last_player_impulse_revision.max(impulse.revision);
                    let entity_id = if impulse.player_id == player_id {
                        Some(player_entity_id(player_id))
                    } else {
                        last_players.get(&impulse.player_id).map(|known| known.0)
                    };
                    if let Some(entity_id) = entity_id {
                        stream
                            .write_all(&encode_set_entity_motion(
                                entity_id,
                                [
                                    impulse.velocity.x,
                                    impulse.velocity.y,
                                    impulse.velocity.z,
                                ],
                            ))
                            .await?;
                    }
                }
                let mobs = state.dimension_mobs(session.dimension);
                let current_ids: HashSet<_> = mobs.iter().map(|mob| mob.entity_id).collect();
                let removed: Vec<_> = last_sent.keys().copied()
                    .filter(|entity_id| !current_ids.contains(entity_id))
                    .collect();
                if !removed.is_empty() {
                    stream.write_all(&encode_remove_entities(&removed)).await?;
                    for entity_id in &removed {
                        last_sent.remove(entity_id);
                        last_health.remove(entity_id);
                        last_fire.remove(entity_id);
                    }
                }

                for mob in mobs {
                    let current = mob_position(&mob);
                    if let Some(previous) = last_sent.get_mut(&mob.entity_id) {
                        let delta = [
                            current[0] - previous[0],
                            current[1] - previous[1],
                            current[2] - previous[2],
                        ];
                        if delta.iter().any(|component| component.abs() >= 1.0 / 4096.0) {
                            stream.write_all(&encode_move_entity_pos_rot(
                                mob.entity_id,
                                delta,
                                mob.yaw,
                                0.0,
                                true,
                            )).await?;
                            *previous = current;
                        }
                        // Body and head rotation are separate in the vanilla protocol.
                        stream.write_all(&encode_rotate_head(mob.entity_id, mob.yaw)).await?;

                        if last_fire.get(&mob.entity_id).copied() != Some(mob.on_fire) {
                            stream.write_all(&encode_entity_flags(mob.entity_id, mob.on_fire)).await?;
                            last_fire.insert(mob.entity_id, mob.on_fire);
                        }
                        if last_health.get(&mob.entity_id).is_some_and(|health| mob.health < *health) {
                            stream.write_all(&encode_entity_event(mob.entity_id, 2)).await?;
                        }
                        last_health.insert(mob.entity_id, mob.health);
                    } else {
                        stream.write_all(&encode_mob_spawn(&mob)).await?;
                        stream.write_all(&encode_entity_flags(mob.entity_id, mob.on_fire)).await?;
                        stream.write_all(&encode_rotate_head(mob.entity_id, mob.yaw)).await?;
                        last_sent.insert(mob.entity_id, current);
                        last_health.insert(mob.entity_id, mob.health);
                        last_fire.insert(mob.entity_id, mob.on_fire);
                    }
                }

                let items = state.dimension_items(session.dimension);
                let current_item_ids: HashSet<_> =
                    items.iter().map(|item| item.entity_id).collect();
                let removed_items: Vec<_> = last_items
                    .keys()
                    .copied()
                    .filter(|entity_id| !current_item_ids.contains(entity_id))
                    .collect();
                if !removed_items.is_empty() {
                    stream.write_all(&encode_remove_entities(&removed_items)).await?;
                    for entity_id in removed_items {
                        last_items.remove(&entity_id);
                    }
                }
                for item in items {
                    let current = item_position(&item);
                    if let Some(previous) = last_items.get_mut(&item.entity_id) {
                        let delta = [
                            current[0] - previous[0],
                            current[1] - previous[1],
                            current[2] - previous[2],
                        ];
                        if delta.iter().any(|component| component.abs() >= 1.0 / 4096.0) {
                            stream
                                .write_all(&encode_move_entity_pos_rot(
                                    item.entity_id,
                                    delta,
                                    0.0,
                                    0.0,
                                    true,
                                ))
                                .await?;
                            *previous = current;
                        }
                    } else {
                        stream.write_all(&encode_item_spawn(&item)).await?;
                        stream.write_all(&encode_item_data(&item)).await?;
                        last_items.insert(item.entity_id, current);
                    }
                }
            }
            _ = keep_alive.tick() => {
                let id = i64::try_from(state.current_tick()).unwrap_or(i64::MAX);
                stream.write_all(&encode_keep_alive(id)).await?;
            }
        }
        }
    }
    .await;
    if let Some(mut grid) = crafting_table_grid.take() {
        inventory_cursor = return_crafting_table_grid(
            state,
            player_id,
            session.dimension,
            &mut grid,
            inventory_cursor,
        );
    }
    inventory_cursor = return_crafting_grid(
        state,
        player_id,
        session.dimension,
        &mut crafting_grid,
        inventory_cursor,
    );
    let _ = return_inventory_cursor(state, player_id, session.dimension, inventory_cursor);
    result
}

fn click_local_slot(
    slot: &mut InventoryCursor,
    mut cursor: InventoryCursor,
    right_click: bool,
) -> InventoryCursor {
    match (slot.stack, cursor.stack) {
        (Some(mut held), None) => {
            let taken = if right_click {
                held.count.div_ceil(2)
            } else {
                held.count
            };
            cursor = InventoryCursor {
                stack: Some(ItemStack {
                    count: taken,
                    ..held
                }),
                sharpness_level: slot.sharpness_level,
            };
            held.count -= taken;
            slot.stack = (held.count > 0).then_some(held);
            if slot.stack.is_none() {
                slot.sharpness_level = 0;
            }
        }
        (None, Some(mut carried)) => {
            let moved = if right_click { 1 } else { carried.count };
            slot.stack = Some(ItemStack {
                count: moved,
                ..carried
            });
            slot.sharpness_level = cursor.sharpness_level;
            carried.count -= moved;
            cursor.stack = (carried.count > 0).then_some(carried);
            if cursor.stack.is_none() {
                cursor.sharpness_level = 0;
            }
        }
        (Some(mut held), Some(mut carried))
            if held.kind == carried.kind
                && held.damage == carried.damage
                && slot.sharpness_level == cursor.sharpness_level =>
        {
            let moved = if right_click { 1 } else { carried.count }
                .min(inventory_stack_limit(held.kind).saturating_sub(held.count));
            held.count += moved;
            carried.count -= moved;
            slot.stack = Some(held);
            cursor.stack = (carried.count > 0).then_some(carried);
            if cursor.stack.is_none() {
                cursor.sharpness_level = 0;
            }
        }
        (Some(held), Some(carried)) => {
            slot.stack = Some(carried);
            std::mem::swap(&mut slot.sharpness_level, &mut cursor.sharpness_level);
            cursor.stack = Some(held);
        }
        (None, None) => {}
    }
    cursor
}

fn crafting_recipe(grid: &[InventoryCursor; 4]) -> Option<([bool; 4], ItemStack)> {
    let kinds = grid.map(|slot| slot.stack.map(|stack| stack.kind));
    let occupied = kinds.iter().filter(|kind| kind.is_some()).count();
    if occupied == 1 {
        let index = kinds
            .iter()
            .position(|kind| *kind == Some(ItemKind::OakLog))?;
        let mut ingredients = [false; 4];
        ingredients[index] = true;
        return Some((
            ingredients,
            ItemStack {
                kind: ItemKind::OakPlanks,
                count: 4,
                damage: 0,
            },
        ));
    }
    if occupied == 2 {
        let iron = kinds
            .iter()
            .position(|kind| *kind == Some(ItemKind::IronIngot));
        let flint = kinds.iter().position(|kind| *kind == Some(ItemKind::Flint));
        if let (Some(iron), Some(flint)) = (iron, flint) {
            let mut ingredients = [false; 4];
            ingredients[iron] = true;
            ingredients[flint] = true;
            return Some((
                ingredients,
                ItemStack {
                    kind: ItemKind::FlintAndSteel,
                    count: 1,
                    damage: 0,
                },
            ));
        }
        for [top, bottom] in [[0, 2], [1, 3]] {
            if kinds[top] == Some(ItemKind::OakPlanks) && kinds[bottom] == Some(ItemKind::OakPlanks)
            {
                let mut ingredients = [false; 4];
                ingredients[top] = true;
                ingredients[bottom] = true;
                return Some((
                    ingredients,
                    ItemStack {
                        kind: ItemKind::Stick,
                        count: 4,
                        damage: 0,
                    },
                ));
            }
        }
    }
    if kinds.iter().all(|kind| *kind == Some(ItemKind::OakPlanks)) {
        return Some((
            [true; 4],
            ItemStack {
                kind: ItemKind::CraftingTable,
                count: 1,
                damage: 0,
            },
        ));
    }
    None
}

fn craft_from_grid(grid: &mut [InventoryCursor; 4], cursor: &mut InventoryCursor) -> bool {
    let Some((ingredients, output)) = crafting_recipe(grid) else {
        return false;
    };
    match cursor.stack {
        None => {
            cursor.stack = Some(output);
            cursor.sharpness_level = 0;
        }
        Some(mut carried)
            if carried.kind == output.kind
                && carried.damage == output.damage
                && cursor.sharpness_level == 0
                && carried.count <= inventory_stack_limit(output.kind) - output.count =>
        {
            carried.count += output.count;
            cursor.stack = Some(carried);
        }
        _ => return false,
    }
    for (slot, consume) in grid.iter_mut().zip(ingredients) {
        if consume {
            if let Some(stack) = slot.stack.as_mut() {
                stack.count -= 1;
                if stack.count == 0 {
                    *slot = InventoryCursor::default();
                }
            }
        }
    }
    true
}

fn crafting_table_recipe(grid: &[InventoryCursor; 9]) -> Option<([bool; 9], ItemStack)> {
    let kinds = grid.map(|slot| slot.stack.map(|stack| stack.kind));
    let recipe = |entries: &[(usize, ItemKind)], kind: ItemKind, count: u8| {
        if kinds.iter().filter(|kind| kind.is_some()).count() != entries.len()
            || entries
                .iter()
                .any(|(index, kind)| kinds[*index] != Some(*kind))
        {
            return None;
        }
        let mut ingredients = [false; 9];
        for (index, _) in entries {
            ingredients[*index] = true;
        }
        Some((
            ingredients,
            ItemStack {
                kind,
                count,
                damage: 0,
            },
        ))
    };
    let tool_set = |material, pickaxe, axe, shovel, sword| {
        recipe(
            &[
                (0, material),
                (1, material),
                (2, material),
                (4, ItemKind::Stick),
                (7, ItemKind::Stick),
            ],
            pickaxe,
            1,
        )
        .or_else(|| {
            recipe(
                &[
                    (0, material),
                    (1, material),
                    (3, material),
                    (4, ItemKind::Stick),
                    (7, ItemKind::Stick),
                ],
                axe,
                1,
            )
        })
        .or_else(|| {
            recipe(
                &[(0, material), (3, ItemKind::Stick), (6, ItemKind::Stick)],
                shovel,
                1,
            )
        })
        .or_else(|| {
            recipe(
                &[(0, material), (3, material), (6, ItemKind::Stick)],
                sword,
                1,
            )
        })
    };
    let armor_set = |material, helmet, chestplate, leggings, boots| {
        recipe(
            &[
                (0, material),
                (1, material),
                (2, material),
                (3, material),
                (5, material),
            ],
            helmet,
            1,
        )
        .or_else(|| {
            recipe(
                &[
                    (0, material),
                    (2, material),
                    (3, material),
                    (4, material),
                    (5, material),
                    (6, material),
                    (7, material),
                    (8, material),
                ],
                chestplate,
                1,
            )
        })
        .or_else(|| {
            recipe(
                &[
                    (0, material),
                    (1, material),
                    (2, material),
                    (3, material),
                    (5, material),
                    (6, material),
                    (8, material),
                ],
                leggings,
                1,
            )
        })
        .or_else(|| {
            recipe(
                &[(0, material), (2, material), (3, material), (5, material)],
                boots,
                1,
            )
        })
    };

    if kinds.iter().filter(|kind| kind.is_some()).count() == 2 {
        let iron = kinds
            .iter()
            .position(|kind| *kind == Some(ItemKind::IronIngot));
        let flint = kinds.iter().position(|kind| *kind == Some(ItemKind::Flint));
        if let (Some(iron), Some(flint)) = (iron, flint) {
            return recipe(
                &[(iron, ItemKind::IronIngot), (flint, ItemKind::Flint)],
                ItemKind::FlintAndSteel,
                1,
            );
        }
    }

    recipe(
        &[
            (0, ItemKind::Cobblestone),
            (1, ItemKind::Cobblestone),
            (2, ItemKind::Cobblestone),
            (3, ItemKind::Cobblestone),
            (5, ItemKind::Cobblestone),
            (6, ItemKind::Cobblestone),
            (7, ItemKind::Cobblestone),
            (8, ItemKind::Cobblestone),
        ],
        ItemKind::Furnace,
        1,
    )
    .or_else(|| {
        recipe(
            &[
                (0, ItemKind::OakPlanks),
                (1, ItemKind::OakPlanks),
                (2, ItemKind::OakPlanks),
                (3, ItemKind::OakPlanks),
                (5, ItemKind::OakPlanks),
                (6, ItemKind::OakPlanks),
                (7, ItemKind::OakPlanks),
                (8, ItemKind::OakPlanks),
            ],
            ItemKind::Chest,
            1,
        )
    })
    .or_else(|| {
        [[0, 2, 4], [3, 5, 7]].into_iter().find_map(|indices| {
            recipe(
                &[
                    (indices[0], ItemKind::IronIngot),
                    (indices[1], ItemKind::IronIngot),
                    (indices[2], ItemKind::IronIngot),
                ],
                ItemKind::Bucket,
                1,
            )
        })
    })
    .or_else(|| {
        tool_set(
            ItemKind::Cobblestone,
            ItemKind::StonePickaxe,
            ItemKind::StoneAxe,
            ItemKind::StoneShovel,
            ItemKind::StoneSword,
        )
    })
    .or_else(|| {
        tool_set(
            ItemKind::IronIngot,
            ItemKind::IronPickaxe,
            ItemKind::IronAxe,
            ItemKind::IronShovel,
            ItemKind::IronSword,
        )
    })
    .or_else(|| {
        armor_set(
            ItemKind::IronIngot,
            ItemKind::IronHelmet,
            ItemKind::IronChestplate,
            ItemKind::IronLeggings,
            ItemKind::IronBoots,
        )
    })
    .or_else(|| {
        tool_set(
            ItemKind::Diamond,
            ItemKind::DiamondPickaxe,
            ItemKind::DiamondAxe,
            ItemKind::DiamondShovel,
            ItemKind::DiamondSword,
        )
    })
    .or_else(|| {
        armor_set(
            ItemKind::Diamond,
            ItemKind::DiamondHelmet,
            ItemKind::DiamondChestplate,
            ItemKind::DiamondLeggings,
            ItemKind::DiamondBoots,
        )
    })
    .or_else(|| {
        recipe(
            &[
                (0, ItemKind::OakPlanks),
                (1, ItemKind::IronIngot),
                (2, ItemKind::OakPlanks),
                (3, ItemKind::OakPlanks),
                (4, ItemKind::OakPlanks),
                (5, ItemKind::OakPlanks),
                (7, ItemKind::OakPlanks),
            ],
            ItemKind::Shield,
            1,
        )
    })
    .or_else(|| {
        recipe(
            &[
                (0, ItemKind::OakPlanks),
                (1, ItemKind::OakPlanks),
                (2, ItemKind::OakPlanks),
                (4, ItemKind::Stick),
                (7, ItemKind::Stick),
            ],
            ItemKind::WoodenPickaxe,
            1,
        )
    })
    .or_else(|| {
        [[0, 1, 3, 4, 7], [1, 2, 5, 4, 7]]
            .into_iter()
            .find_map(|indices| {
                recipe(
                    &[
                        (indices[0], ItemKind::OakPlanks),
                        (indices[1], ItemKind::OakPlanks),
                        (indices[2], ItemKind::OakPlanks),
                        (indices[3], ItemKind::Stick),
                        (indices[4], ItemKind::Stick),
                    ],
                    ItemKind::WoodenAxe,
                    1,
                )
            })
    })
    .or_else(|| {
        (0..3).find_map(|column| {
            recipe(
                &[
                    (column, ItemKind::OakPlanks),
                    (column + 3, ItemKind::OakPlanks),
                    (column + 6, ItemKind::Stick),
                ],
                ItemKind::WoodenSword,
                1,
            )
        })
    })
    .or_else(|| {
        (0..3).find_map(|column| {
            recipe(
                &[
                    (column, ItemKind::OakPlanks),
                    (column + 3, ItemKind::Stick),
                    (column + 6, ItemKind::Stick),
                ],
                ItemKind::WoodenShovel,
                1,
            )
        })
    })
    .or_else(|| {
        [0, 1, 3, 4].into_iter().find_map(|top_left| {
            recipe(
                &[
                    (top_left, ItemKind::OakPlanks),
                    (top_left + 1, ItemKind::OakPlanks),
                    (top_left + 3, ItemKind::OakPlanks),
                    (top_left + 4, ItemKind::OakPlanks),
                ],
                ItemKind::CraftingTable,
                1,
            )
        })
    })
    .or_else(|| {
        (0..3).find_map(|column| {
            [0, 3].into_iter().find_map(|row| {
                recipe(
                    &[
                        (row + column, ItemKind::OakPlanks),
                        (row + column + 3, ItemKind::OakPlanks),
                    ],
                    ItemKind::Stick,
                    4,
                )
            })
        })
    })
    .or_else(|| {
        let index = kinds
            .iter()
            .position(|kind| *kind == Some(ItemKind::OakLog))?;
        recipe(&[(index, ItemKind::OakLog)], ItemKind::OakPlanks, 4)
    })
}

fn consume_crafting_table_recipe(grid: &mut [InventoryCursor; 9], ingredients: [bool; 9]) {
    for (slot, consume) in grid.iter_mut().zip(ingredients) {
        if consume {
            if let Some(stack) = slot.stack.as_mut() {
                stack.count -= 1;
                if stack.count == 0 {
                    *slot = InventoryCursor::default();
                }
            }
        }
    }
}

fn craft_from_table(grid: &mut [InventoryCursor; 9], cursor: &mut InventoryCursor) -> bool {
    let Some((ingredients, output)) = crafting_table_recipe(grid) else {
        return false;
    };
    match cursor.stack {
        None => cursor.stack = Some(output),
        Some(mut carried)
            if carried.kind == output.kind
                && carried.damage == output.damage
                && cursor.sharpness_level == 0
                && carried.count <= inventory_stack_limit(output.kind) - output.count =>
        {
            carried.count += output.count;
            cursor.stack = Some(carried);
        }
        _ => return false,
    }
    cursor.sharpness_level = 0;
    consume_crafting_table_recipe(grid, ingredients);
    true
}

fn shift_craft_from_table(
    state: &dyn ServerApi,
    player_id: Uuid,
    grid: &mut [InventoryCursor; 9],
) -> bool {
    let mut crafted = false;
    for _ in 0..64 {
        let Some((ingredients, output)) = crafting_table_recipe(grid) else {
            break;
        };
        if inventory_capacity(state, player_id, output) < u16::from(output.count)
            || !state.give_item(player_id, output.kind, output.count)
        {
            break;
        }
        consume_crafting_table_recipe(grid, ingredients);
        crafted = true;
    }
    crafted
}

fn inventory_capacity(state: &dyn ServerApi, player_id: Uuid, item: ItemStack) -> u16 {
    let maximum = inventory_stack_limit(item.kind);
    state.inventory(player_id).map_or(0, |inventory| {
        inventory
            .slots
            .iter()
            .map(|slot| match slot {
                Some(stack) if stack.kind == item.kind && stack.damage == item.damage => {
                    u16::from(maximum.saturating_sub(stack.count))
                }
                None => u16::from(maximum),
                _ => 0,
            })
            .sum()
    })
}

#[derive(Clone, Copy)]
struct InventoryGesture {
    menu_slot: i16,
    button: i8,
    input: i32,
    crafting_table: bool,
}

fn handle_inventory_gesture(
    state: &dyn ServerApi,
    player_id: Uuid,
    dimension: DimensionKind,
    gesture: InventoryGesture,
    mut cursor: InventoryCursor,
    drag_slots: &mut HashSet<PlayerInventorySlot>,
    drag_button: &mut Option<i8>,
) -> InventoryCursor {
    let InventoryGesture {
        menu_slot,
        button,
        input,
        crafting_table,
    } = gesture;
    let player_slot = if crafting_table {
        crafting_table_menu_slot(menu_slot)
    } else {
        player_menu_slot(menu_slot)
    };
    match input {
        0 if matches!(button, 0 | 1) => {
            if menu_slot == -999 {
                if let Some(mut stack) = cursor.stack {
                    let dropped = if button == 0 { stack.count } else { 1 };
                    state.drop_dimension_item(
                        dimension,
                        ItemStack {
                            count: dropped,
                            ..stack
                        },
                        player_block_position(state, player_id),
                    );
                    stack.count -= dropped;
                    cursor.stack = (stack.count > 0).then_some(stack);
                    if cursor.stack.is_none() {
                        cursor.sharpness_level = 0;
                    }
                }
            } else if let Some(slot) = player_slot {
                cursor = state
                    .click_player_inventory_slot(player_id, slot, cursor, button == 1)
                    .unwrap_or(cursor);
            }
        }
        1 if cursor.stack.is_none() => {
            if let Some(slot) = player_slot {
                cursor = quick_move_slot(state, player_id, slot);
            }
        }
        2 if cursor.stack.is_none() => {
            let target = match button {
                0..=8 => Some(PlayerInventorySlot::Storage(button as u8)),
                40 => Some(PlayerInventorySlot::OffHand),
                _ => None,
            };
            if let (Some(source), Some(target)) = (player_slot, target) {
                swap_player_slots(state, player_id, source, target);
            }
        }
        4 if cursor.stack.is_none() && matches!(button, 0 | 1) => {
            if let Some(slot) = player_slot {
                cursor = throw_from_slot(state, player_id, dimension, slot, button == 1);
            }
        }
        5 => {
            let header = button & 3;
            let drag_type = (button >> 2) & 3;
            match header {
                0 if cursor.stack.is_some() && matches!(drag_type, 0 | 1) => {
                    drag_slots.clear();
                    *drag_button = Some(drag_type);
                }
                1 if *drag_button == Some(drag_type) => {
                    if let Some(slot) = player_slot {
                        if slot_accepts_cursor(state, player_id, slot, cursor) {
                            drag_slots.insert(slot);
                        }
                    }
                }
                2 if *drag_button == Some(drag_type) => {
                    cursor =
                        distribute_cursor(state, player_id, cursor, drag_slots, drag_type == 1);
                    drag_slots.clear();
                    *drag_button = None;
                }
                _ => {
                    drag_slots.clear();
                    *drag_button = None;
                }
            }
        }
        6 if cursor.stack.is_some() => {
            cursor = collect_matching_storage(state, player_id, cursor);
        }
        _ => {}
    }
    cursor
}

fn player_menu_slot(slot: i16) -> Option<PlayerInventorySlot> {
    match slot {
        5..=8 => Some(PlayerInventorySlot::Armor(u8::try_from(8 - slot).ok()?)),
        9..=35 => Some(PlayerInventorySlot::Storage(u8::try_from(slot).ok()?)),
        36..=44 => Some(PlayerInventorySlot::Storage(u8::try_from(slot - 36).ok()?)),
        45 => Some(PlayerInventorySlot::OffHand),
        _ => None,
    }
}

fn crafting_table_menu_slot(slot: i16) -> Option<PlayerInventorySlot> {
    match slot {
        10..=36 => Some(PlayerInventorySlot::Storage(u8::try_from(slot - 1).ok()?)),
        37..=45 => Some(PlayerInventorySlot::Storage(u8::try_from(slot - 37).ok()?)),
        _ => None,
    }
}

fn furnace_menu_slot(slot: i16) -> Option<PlayerInventorySlot> {
    match slot {
        3..=29 => Some(PlayerInventorySlot::Storage(u8::try_from(slot + 6).ok()?)),
        30..=38 => Some(PlayerInventorySlot::Storage(u8::try_from(slot - 30).ok()?)),
        _ => None,
    }
}

fn chest_menu_slot(slot: i16) -> Option<PlayerInventorySlot> {
    match slot {
        27..=53 => Some(PlayerInventorySlot::Storage(u8::try_from(slot - 18).ok()?)),
        54..=62 => Some(PlayerInventorySlot::Storage(u8::try_from(slot - 54).ok()?)),
        _ => None,
    }
}

fn shift_chest_slot(
    state: &dyn ServerApi,
    player_id: Uuid,
    dimension: DimensionKind,
    position: BlockPosition,
    menu_slot: i16,
) -> bool {
    if (0..=26).contains(&menu_slot) {
        let Some(chest) = state.chest(dimension, position) else {
            return false;
        };
        let index = usize::try_from(menu_slot).unwrap_or(0);
        let Some(stack) = chest.slots[index] else {
            return false;
        };
        if inventory_capacity(state, player_id, stack) < u16::from(stack.count) {
            return false;
        }
        let cursor = state
            .click_chest_slot(
                dimension,
                position,
                u8::try_from(index).unwrap_or(0),
                InventoryCursor::default(),
                false,
            )
            .unwrap_or_default();
        return return_inventory_cursor(state, player_id, dimension, cursor)
            .stack
            .is_none();
    }
    let Some(player_slot) = chest_menu_slot(menu_slot) else {
        return false;
    };
    let mut cursor = state
        .click_player_inventory_slot(player_id, player_slot, InventoryCursor::default(), false)
        .unwrap_or_default();
    if cursor.stack.is_none() {
        return false;
    }
    let original = cursor;
    for slot in 0..27_u8 {
        cursor = state
            .click_chest_slot(dimension, position, slot, cursor, false)
            .unwrap_or(cursor);
        if cursor.stack.is_none() {
            return true;
        }
    }
    if cursor != original {
        cursor = state
            .click_player_inventory_slot(player_id, player_slot, cursor, false)
            .unwrap_or(cursor);
        let _ = return_inventory_cursor(state, player_id, dimension, cursor);
        true
    } else {
        let _ = state.click_player_inventory_slot(player_id, player_slot, cursor, false);
        false
    }
}

fn slot_contents(
    state: &dyn ServerApi,
    player_id: Uuid,
    slot: PlayerInventorySlot,
) -> InventoryCursor {
    match slot {
        PlayerInventorySlot::Storage(index) => state
            .inventory(player_id)
            .and_then(|inventory| {
                let index = usize::from(index);
                Some(InventoryCursor {
                    stack: *inventory.slots.get(index)?,
                    sharpness_level: *inventory.sharpness_levels.get(index)?,
                })
            })
            .unwrap_or_default(),
        PlayerInventorySlot::Armor(index) => InventoryCursor {
            stack: state
                .player_equipment(player_id)
                .and_then(|equipment| equipment.armor.get(usize::from(index)).copied().flatten()),
            sharpness_level: 0,
        },
        PlayerInventorySlot::OffHand => InventoryCursor {
            stack: state
                .player_equipment(player_id)
                .and_then(|equipment| equipment.off_hand),
            sharpness_level: 0,
        },
    }
}

fn slot_accepts_cursor(
    state: &dyn ServerApi,
    player_id: Uuid,
    slot: PlayerInventorySlot,
    cursor: InventoryCursor,
) -> bool {
    let Some(carried) = cursor.stack else {
        return false;
    };
    if !slot_allows_cursor(slot, cursor) {
        return false;
    }
    let current = slot_contents(state, player_id, slot);
    match current.stack {
        None => true,
        Some(stack) => {
            stack.kind == carried.kind
                && stack.damage == carried.damage
                && current.sharpness_level == cursor.sharpness_level
                && stack.count < inventory_stack_limit(stack.kind)
        }
    }
}

fn slot_allows_cursor(slot: PlayerInventorySlot, cursor: InventoryCursor) -> bool {
    let Some(carried) = cursor.stack else {
        return true;
    };
    match slot {
        PlayerInventorySlot::Armor(index) => matches!(
            (index, carried.kind),
            (0, ItemKind::IronBoots)
                | (1, ItemKind::IronLeggings)
                | (2, ItemKind::IronChestplate)
                | (3, ItemKind::IronHelmet)
                | (0, ItemKind::DiamondBoots)
                | (1, ItemKind::DiamondLeggings)
                | (2, ItemKind::DiamondChestplate)
                | (3, ItemKind::DiamondHelmet)
        ),
        _ => true,
    }
}

fn place_cursor_in_slots(
    state: &dyn ServerApi,
    player_id: Uuid,
    mut cursor: InventoryCursor,
    slots: &[PlayerInventorySlot],
) -> InventoryCursor {
    for empty_pass in [false, true] {
        for &slot in slots {
            if cursor.stack.is_none() {
                return InventoryCursor::default();
            }
            let empty = slot_contents(state, player_id, slot).stack.is_none();
            if empty == empty_pass && slot_accepts_cursor(state, player_id, slot, cursor) {
                cursor = state
                    .click_player_inventory_slot(player_id, slot, cursor, false)
                    .unwrap_or(cursor);
            }
        }
    }
    cursor
}

fn quick_move_slot(
    state: &dyn ServerApi,
    player_id: Uuid,
    source: PlayerInventorySlot,
) -> InventoryCursor {
    let mut cursor = state
        .click_player_inventory_slot(player_id, source, InventoryCursor::default(), false)
        .unwrap_or_default();
    let Some(carried) = cursor.stack else {
        return cursor;
    };
    let mut targets = Vec::new();
    if matches!(source, PlayerInventorySlot::Storage(_)) {
        let armor = match carried.kind {
            ItemKind::IronBoots => Some(0),
            ItemKind::IronLeggings => Some(1),
            ItemKind::IronChestplate => Some(2),
            ItemKind::IronHelmet => Some(3),
            ItemKind::DiamondBoots => Some(0),
            ItemKind::DiamondLeggings => Some(1),
            ItemKind::DiamondChestplate => Some(2),
            ItemKind::DiamondHelmet => Some(3),
            _ => None,
        };
        if let Some(index) = armor {
            targets.push(PlayerInventorySlot::Armor(index));
        } else if carried.kind == ItemKind::Shield {
            targets.push(PlayerInventorySlot::OffHand);
        }
        match source {
            PlayerInventorySlot::Storage(0..=8) => {
                targets.extend((9..36).map(PlayerInventorySlot::Storage));
            }
            PlayerInventorySlot::Storage(_) => {
                targets.extend((0..9).map(PlayerInventorySlot::Storage));
            }
            _ => {}
        }
    } else {
        targets.extend((9..36).map(PlayerInventorySlot::Storage));
        targets.extend((0..9).map(PlayerInventorySlot::Storage));
    }
    cursor = place_cursor_in_slots(state, player_id, cursor, &targets);
    if cursor.stack.is_some() {
        cursor = state
            .click_player_inventory_slot(player_id, source, cursor, false)
            .unwrap_or(cursor);
    }
    cursor
}

fn swap_player_slots(
    state: &dyn ServerApi,
    player_id: Uuid,
    source: PlayerInventorySlot,
    target: PlayerInventorySlot,
) {
    if source == target {
        return;
    }
    let source_item = slot_contents(state, player_id, source);
    let target_item = slot_contents(state, player_id, target);
    let source_accepts = slot_allows_cursor(source, target_item);
    let target_accepts = slot_allows_cursor(target, source_item);
    if !source_accepts || !target_accepts {
        return;
    }
    let mut cursor = state
        .click_player_inventory_slot(player_id, source, InventoryCursor::default(), false)
        .unwrap_or_default();
    cursor = state
        .click_player_inventory_slot(player_id, target, cursor, false)
        .unwrap_or(cursor);
    let _ = state.click_player_inventory_slot(player_id, source, cursor, false);
}

fn throw_from_slot(
    state: &dyn ServerApi,
    player_id: Uuid,
    dimension: DimensionKind,
    source: PlayerInventorySlot,
    entire_stack: bool,
) -> InventoryCursor {
    let mut cursor = state
        .click_player_inventory_slot(player_id, source, InventoryCursor::default(), false)
        .unwrap_or_default();
    let Some(mut stack) = cursor.stack else {
        return cursor;
    };
    let dropped = if entire_stack { stack.count } else { 1 };
    state.drop_dimension_item(
        dimension,
        ItemStack {
            count: dropped,
            ..stack
        },
        player_block_position(state, player_id),
    );
    stack.count -= dropped;
    cursor.stack = (stack.count > 0).then_some(stack);
    if cursor.stack.is_none() {
        cursor.sharpness_level = 0;
    } else {
        cursor = state
            .click_player_inventory_slot(player_id, source, cursor, false)
            .unwrap_or(cursor);
    }
    cursor
}

fn distribute_cursor(
    state: &dyn ServerApi,
    player_id: Uuid,
    mut cursor: InventoryCursor,
    slots: &HashSet<PlayerInventorySlot>,
    one_each: bool,
) -> InventoryCursor {
    loop {
        let before = cursor;
        for &slot in slots {
            if cursor.stack.is_none() {
                return InventoryCursor::default();
            }
            if slot_accepts_cursor(state, player_id, slot, cursor) {
                cursor = state
                    .click_player_inventory_slot(player_id, slot, cursor, true)
                    .unwrap_or(cursor);
            }
        }
        if one_each || cursor == before {
            return cursor;
        }
    }
}

fn collect_matching_storage(
    state: &dyn ServerApi,
    player_id: Uuid,
    mut cursor: InventoryCursor,
) -> InventoryCursor {
    let Some(mut carried) = cursor.stack else {
        return cursor;
    };
    let maximum = inventory_stack_limit(carried.kind);
    for index in 0..36 {
        if carried.count >= maximum {
            break;
        }
        let Some(inventory) = state.inventory(player_id) else {
            break;
        };
        let Some(stack) = inventory.slots[index] else {
            continue;
        };
        if stack.kind != carried.kind
            || stack.damage != carried.damage
            || inventory.sharpness_levels[index] != cursor.sharpness_level
        {
            continue;
        }
        let taken = stack.count.min(maximum - carried.count);
        if state.take_item(player_id, index, taken) {
            carried.count += taken;
        }
    }
    cursor.stack = Some(carried);
    cursor
}

fn player_block_position(state: &dyn ServerApi, player_id: Uuid) -> BlockPosition {
    state
        .players()
        .into_iter()
        .find(|player| player.id == player_id)
        .map_or(BlockPosition::default(), |player| player.position)
}

async fn sync_crafting_grid(
    stream: &mut Connection,
    grid: &[InventoryCursor; 4],
    state_id: i32,
) -> anyhow::Result<()> {
    let output = crafting_recipe(grid).map(|(_, output)| output);
    for (slot, item) in std::iter::once(output)
        .chain(grid.iter().map(|slot| slot.stack))
        .enumerate()
    {
        let (count, item_id, damage) = protocol_stack(item);
        let glint = slot > 0 && grid[slot - 1].sharpness_level > 0;
        stream
            .write_all(&encode_container_set_slot_with_glint(
                0,
                state_id,
                i16::try_from(slot).unwrap_or(0),
                count,
                item_id,
                damage,
                glint,
            ))
            .await?;
    }
    Ok(())
}

async fn sync_crafting_table_grid(
    stream: &mut Connection,
    grid: &[InventoryCursor; 9],
    state_id: i32,
) -> anyhow::Result<()> {
    let output = crafting_table_recipe(grid).map(|(_, output)| output);
    for (slot, item) in std::iter::once(output)
        .chain(grid.iter().map(|slot| slot.stack))
        .enumerate()
    {
        let (count, item_id, damage) = protocol_stack(item);
        let glint = slot > 0 && grid[slot - 1].sharpness_level > 0;
        stream
            .write_all(&encode_container_set_slot_with_glint(
                1,
                state_id,
                i16::try_from(slot).unwrap_or(0),
                count,
                item_id,
                damage,
                glint,
            ))
            .await?;
    }
    Ok(())
}

async fn sync_furnace(
    stream: &mut Connection,
    furnace: FurnaceSnapshot,
    state_id: i32,
) -> anyhow::Result<()> {
    for (slot, item) in [furnace.input, furnace.fuel, furnace.output]
        .into_iter()
        .enumerate()
    {
        let (count, item_id, damage) = protocol_stack(item);
        stream
            .write_all(&encode_container_set_slot_with_glint(
                2,
                state_id,
                i16::try_from(slot).unwrap_or(0),
                count,
                item_id,
                damage,
                false,
            ))
            .await?;
    }
    for (property, value) in [
        furnace.burn_remaining,
        furnace.burn_total,
        furnace.cook_progress,
        furnace.cook_total,
    ]
    .into_iter()
    .enumerate()
    {
        stream
            .write_all(&encode_container_set_data(
                2,
                i16::try_from(property).unwrap_or(0),
                i16::try_from(value).unwrap_or(i16::MAX),
            ))
            .await?;
    }
    Ok(())
}

async fn sync_chest(
    stream: &mut Connection,
    chest: ChestSnapshot,
    state_id: i32,
) -> anyhow::Result<()> {
    for (slot, item) in chest.slots.into_iter().enumerate() {
        let (count, item_id, damage) = protocol_stack(item);
        stream
            .write_all(&encode_container_set_slot_with_glint(
                3,
                state_id,
                i16::try_from(slot).unwrap_or(0),
                count,
                item_id,
                damage,
                chest.sharpness_levels[slot] > 0,
            ))
            .await?;
    }
    Ok(())
}

fn return_crafting_grid(
    state: &dyn ServerApi,
    player_id: Uuid,
    dimension: DimensionKind,
    grid: &mut [InventoryCursor; 4],
    mut cursor: InventoryCursor,
) -> InventoryCursor {
    cursor = return_inventory_cursor(state, player_id, dimension, cursor);
    for slot in grid {
        cursor = return_inventory_cursor(state, player_id, dimension, *slot);
        *slot = InventoryCursor::default();
    }
    cursor
}

fn return_crafting_table_grid(
    state: &dyn ServerApi,
    player_id: Uuid,
    dimension: DimensionKind,
    grid: &mut [InventoryCursor; 9],
    mut cursor: InventoryCursor,
) -> InventoryCursor {
    cursor = return_inventory_cursor(state, player_id, dimension, cursor);
    for slot in grid {
        cursor = return_inventory_cursor(state, player_id, dimension, *slot);
        *slot = InventoryCursor::default();
    }
    cursor
}

async fn sync_player_inventory(
    stream: &mut Connection,
    state: &dyn ServerApi,
    player_id: Uuid,
    cursor: InventoryCursor,
) -> anyhow::Result<()> {
    if let Some(inventory) = state.inventory(player_id) {
        for (slot, stack) in inventory.slots.iter().enumerate() {
            let (count, item_id, damage) = protocol_stack(*stack);
            stream
                .write_all(&encode_set_player_inventory_with_glint(
                    i32::try_from(slot).unwrap_or(0),
                    count,
                    item_id,
                    damage,
                    inventory.sharpness_levels[slot] > 0,
                ))
                .await?;
        }
    }
    if let Some(equipment) = state.player_equipment(player_id) {
        sync_player_equipment_inventory(stream, equipment).await?;
    }
    let (count, item_id, damage) = protocol_stack(cursor.stack);
    stream
        .write_all(&encode_set_cursor_item_with_glint(
            count,
            item_id,
            damage,
            cursor.sharpness_level > 0,
        ))
        .await?;
    Ok(())
}

async fn sync_player_equipment_inventory(
    stream: &mut Connection,
    equipment: PlayerEquipment,
) -> anyhow::Result<()> {
    for (index, stack) in equipment.armor.into_iter().enumerate() {
        let (count, item_id, damage) = protocol_stack(stack);
        stream
            .write_all(&encode_set_player_inventory_with_glint(
                i32::try_from(36 + index).unwrap_or(36),
                count,
                item_id,
                damage,
                false,
            ))
            .await?;
    }
    let (count, item_id, damage) = protocol_stack(equipment.off_hand);
    stream
        .write_all(&encode_set_player_inventory_with_glint(
            40, count, item_id, damage, false,
        ))
        .await?;
    Ok(())
}

fn protocol_stack(stack: Option<ItemStack>) -> (u8, i32, u16) {
    stack.map_or((0, 0, 0), |stack| {
        (stack.count, item_protocol_id(stack.kind), stack.damage)
    })
}

fn return_inventory_cursor(
    state: &dyn ServerApi,
    player_id: Uuid,
    dimension: DimensionKind,
    mut cursor: InventoryCursor,
) -> InventoryCursor {
    for index in 0..36_u8 {
        let Some(carried) = cursor.stack else {
            return InventoryCursor::default();
        };
        let inventory = state.inventory(player_id);
        let can_place = inventory.as_ref().is_some_and(|inventory| {
            let slot = inventory.slots[usize::from(index)];
            slot.is_none()
                || slot.is_some_and(|stack| {
                    stack.kind == carried.kind
                        && stack.damage == carried.damage
                        && inventory.sharpness_levels[usize::from(index)] == cursor.sharpness_level
                        && stack.count < inventory_stack_limit(stack.kind)
                })
        });
        if can_place {
            if let Some(next) = state.click_player_inventory_slot(
                player_id,
                PlayerInventorySlot::Storage(index),
                cursor,
                false,
            ) {
                cursor = next;
            }
        }
    }
    if let Some(stack) = cursor.stack {
        let position = state
            .players()
            .into_iter()
            .find(|player| player.id == player_id)
            .map_or(BlockPosition::default(), |player| player.position);
        state.drop_dimension_item(dimension, stack, position);
    }
    InventoryCursor::default()
}

fn inventory_stack_limit(kind: ItemKind) -> u8 {
    match kind {
        ItemKind::WoodenPickaxe
        | ItemKind::WoodenAxe
        | ItemKind::WoodenShovel
        | ItemKind::WoodenSword
        | ItemKind::StonePickaxe
        | ItemKind::StoneAxe
        | ItemKind::StoneShovel
        | ItemKind::StoneSword
        | ItemKind::IronPickaxe
        | ItemKind::IronAxe
        | ItemKind::IronShovel
        | ItemKind::IronSword
        | ItemKind::DiamondPickaxe
        | ItemKind::DiamondAxe
        | ItemKind::DiamondShovel
        | ItemKind::DiamondSword
        | ItemKind::FlintAndSteel
        | ItemKind::Shield
        | ItemKind::IronHelmet
        | ItemKind::IronChestplate
        | ItemKind::IronLeggings
        | ItemKind::IronBoots => 1,
        ItemKind::DiamondHelmet
        | ItemKind::DiamondChestplate
        | ItemKind::DiamondLeggings
        | ItemKind::DiamondBoots
        | ItemKind::MilkBucket => 1,
        ItemKind::Bucket => 16,
        _ => 64,
    }
}

fn encode_mob_spawn(mob: &MobSnapshot) -> Vec<u8> {
    encode_add_entity(
        mob.entity_id,
        *mob.id.as_bytes(),
        match mob.kind {
            MobKind::Cow => 30,
            MobKind::Pig => 100,
            MobKind::Zombie => 151,
        },
        mob_position(mob),
        [mob.velocity.x, mob.velocity.y, mob.velocity.z],
        mob.yaw,
    )
}

fn player_entity_id(id: Uuid) -> i32 {
    (i32::from_be_bytes(id.as_bytes()[..4].try_into().expect("UUID prefix")) & i32::MAX).max(1)
}

fn player_dimension(player: &PlayerSnapshot) -> DimensionKind {
    if player.world == DimensionKind::Nether.name() {
        DimensionKind::Nether
    } else if player.world == DimensionKind::End.name() {
        DimensionKind::End
    } else {
        DimensionKind::Overworld
    }
}

fn encode_remote_player_spawn(
    player: &PlayerSnapshot,
    entity_id: i32,
    transform: PlayerTransform,
) -> Vec<u8> {
    encode_add_entity_with_rotation(
        entity_id,
        *player.id.as_bytes(),
        156,
        [
            transform.position.x,
            transform.position.y,
            transform.position.z,
        ],
        [0.0; 3],
        transform.yaw,
        transform.pitch,
    )
}

fn mob_position(mob: &MobSnapshot) -> [f64; 3] {
    [mob.position.x, mob.position.y, mob.position.z]
}

fn encode_item_spawn(item: &ItemEntitySnapshot) -> Vec<u8> {
    encode_add_entity(
        item.entity_id,
        *item.id.as_bytes(),
        71,
        item_position(item),
        [item.velocity.x, item.velocity.y, item.velocity.z],
        0.0,
    )
}

fn encode_item_data(item: &ItemEntitySnapshot) -> Vec<u8> {
    encode_item_entity_data(
        item.entity_id,
        item.stack.count,
        item_protocol_id(item.stack.kind),
        item.stack.damage,
    )
}

fn item_position(item: &ItemEntitySnapshot) -> [f64; 3] {
    [item.position.x, item.position.y, item.position.z]
}

async fn stream_visible_chunks(
    stream: &mut Connection,
    state: &dyn ServerApi,
    dimension: DimensionKind,
    center: ChunkPosition,
    radius: i32,
    loaded: &mut HashSet<ChunkPosition>,
) -> anyhow::Result<usize> {
    let visible: HashSet<_> = (center.z - radius..=center.z + radius)
        .flat_map(|z| (center.x - radius..=center.x + radius).map(move |x| ChunkPosition { x, z }))
        .collect();
    let stale: Vec<_> = loaded.difference(&visible).copied().collect();
    for position in stale {
        stream
            .write_all(&encode_forget_level_chunk(position.x, position.z))
            .await?;
        loaded.remove(&position);
    }
    let mut pending = Vec::new();
    for z in center.z - radius..=center.z + radius {
        for x in center.x - radius..=center.x + radius {
            let position = ChunkPosition { x, z };
            if !loaded.contains(&position) {
                pending.push(position);
            }
        }
    }
    pending.sort_by_key(|position| {
        let dx = i64::from(position.x - center.x);
        let dz = i64::from(position.z - center.z);
        dx * dx + dz * dz
    });
    if pending.is_empty() {
        return Ok(0);
    }

    stream.write_all(&encode_chunk_batch_start()).await?;
    for position in &pending {
        let surface = state.ensure_dimension_chunk_surface(dimension, *position);
        let blocks = state
            .dimension_chunk_blocks(dimension, *position)
            .into_iter()
            .map(|block| ChunkBlockState {
                x: u8::try_from(block.position.x.rem_euclid(16)).unwrap_or(0),
                y: i16::try_from(block.position.y).unwrap_or(0),
                z: u8::try_from(block.position.z.rem_euclid(16)).unwrap_or(0),
                state_id: block_state_id(block.kind),
            })
            .collect::<Vec<_>>();
        let light_sources = if dimension == DimensionKind::Nether {
            ((position.z - 1)..=(position.z + 1))
                .flat_map(|z| {
                    ((position.x - 1)..=(position.x + 1)).map(move |x| ChunkPosition { x, z })
                })
                .flat_map(|source_chunk| state.dimension_chunk_blocks(dimension, source_chunk))
                .filter(|block| block.kind == BlockKind::Lava)
                .map(|block| BlockLightSource {
                    x: i16::try_from(block.position.x - position.x * 16).unwrap_or(i16::MAX),
                    y: i16::try_from(block.position.y).unwrap_or(i16::MAX),
                    z: i16::try_from(block.position.z - position.z * 16).unwrap_or(i16::MAX),
                    level: 15,
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        stream
            .write_all(&encode_generated_chunk_with_terrain_and_light_sources(
                position.x,
                position.z,
                &surface,
                &blocks,
                biome_protocol_id(state.dimension_chunk_biome(dimension, *position)),
                if dimension == DimensionKind::End {
                    // End slabs arrive as explicit placements; no foundation
                    // may be synthesized below their finite undersides.
                    ChunkTerrainStates {
                        surface: 0,
                        filler: 0,
                        foundation: 0,
                        bedrock: 0,
                    }
                } else {
                    terrain_protocol_states(state.dimension_chunk_terrain(dimension, *position))
                },
                ChunkLighting {
                    has_skylight: dimension == DimensionKind::Overworld,
                    sources: &light_sources,
                },
            ))
            .await?;
        loaded.insert(*position);
    }
    stream
        .write_all(&encode_chunk_batch_finished(
            i32::try_from(pending.len()).context("chunk batch is too large")?,
        ))
        .await?;
    Ok(pending.len())
}

fn block_state_id(kind: BlockKind) -> i32 {
    match kind {
        BlockKind::Air => 0,
        BlockKind::Water => 86,
        BlockKind::Lava => 102,
        BlockKind::Stone => 1,
        BlockKind::CoalOre => 133,
        BlockKind::IronOre => 131,
        BlockKind::CopperOre => 27790,
        BlockKind::GoldOre => 129,
        BlockKind::RedstoneOre => 6881,
        BlockKind::LapisOre => 563,
        BlockKind::DiamondOre => 5307,
        BlockKind::Cobblestone => 14,
        BlockKind::Grass => 9,
        BlockKind::ShortGrass => 2248,
        BlockKind::Fern => 2249,
        BlockKind::DeadBush => 2250,
        BlockKind::Dirt => 10,
        BlockKind::Sand => 118,
        BlockKind::Gravel => 124,
        BlockKind::Sandstone => 578,
        BlockKind::SnowBlock => 6928,
        BlockKind::Netherrack => 6997,
        BlockKind::SoulSand => 6998,
        BlockKind::Basalt => 7000,
        BlockKind::EndStone => 9477,
        BlockKind::Obsidian => 3369,
        BlockKind::StoneBricks => 7754,
        BlockKind::Bedrock => 85,
        BlockKind::OakLog => 137,
        BlockKind::OakPlanks => 15,
        BlockKind::OakLeaves => 279,
        BlockKind::BirchLog => 142,
        BlockKind::BirchLeaves => 308,
        BlockKind::SpruceLog => 139,
        BlockKind::SpruceLeaves => 280,
        BlockKind::CraftingTable => 5310,
        BlockKind::Furnace => 5328,
        BlockKind::Chest => 3988,
        BlockKind::NetherPortal => 7017,
        BlockKind::EndPortal => 9468,
    }
}

fn block_drop(kind: BlockKind, tool: Option<ItemKind>) -> Option<ItemStack> {
    let tier = tool.map_or(0, pickaxe_tier);
    let kind = match kind {
        BlockKind::Stone if tier >= 1 => ItemKind::Cobblestone,
        BlockKind::Stone => return None,
        BlockKind::CoalOre if tier >= 1 => ItemKind::Coal,
        BlockKind::CopperOre if tier >= 1 => ItemKind::RawCopper,
        BlockKind::IronOre if tier >= 2 => ItemKind::RawIron,
        BlockKind::LapisOre if tier >= 2 => ItemKind::LapisLazuli,
        BlockKind::GoldOre if tier >= 3 => ItemKind::RawGold,
        BlockKind::RedstoneOre if tier >= 3 => ItemKind::Redstone,
        BlockKind::DiamondOre if tier >= 3 => ItemKind::Diamond,
        BlockKind::CoalOre
        | BlockKind::IronOre
        | BlockKind::CopperOre
        | BlockKind::GoldOre
        | BlockKind::RedstoneOre
        | BlockKind::LapisOre
        | BlockKind::DiamondOre => return None,
        BlockKind::Cobblestone if tier >= 1 => ItemKind::Cobblestone,
        BlockKind::Cobblestone => return None,
        BlockKind::Dirt | BlockKind::Grass => ItemKind::Dirt,
        BlockKind::Sand => ItemKind::Sand,
        BlockKind::Gravel => ItemKind::Gravel,
        BlockKind::Sandstone if tier >= 1 => ItemKind::Sandstone,
        BlockKind::Netherrack if tier >= 1 => ItemKind::Netherrack,
        BlockKind::SoulSand => ItemKind::SoulSand,
        BlockKind::Basalt if tier >= 1 => ItemKind::Basalt,
        BlockKind::EndStone if tier >= 1 => ItemKind::EndStone,
        BlockKind::StoneBricks if tier >= 1 => ItemKind::StoneBricks,
        BlockKind::Obsidian if tier >= 4 => ItemKind::Obsidian,
        BlockKind::Obsidian => return None,
        BlockKind::Sandstone
        | BlockKind::Netherrack
        | BlockKind::Basalt
        | BlockKind::EndStone
        | BlockKind::StoneBricks
        | BlockKind::SnowBlock => return None,
        BlockKind::OakLog => ItemKind::OakLog,
        BlockKind::BirchLog => ItemKind::BirchLog,
        BlockKind::SpruceLog => ItemKind::SpruceLog,
        BlockKind::OakPlanks => ItemKind::OakPlanks,
        BlockKind::CraftingTable => ItemKind::CraftingTable,
        BlockKind::Furnace => ItemKind::Furnace,
        BlockKind::Chest => ItemKind::Chest,
        BlockKind::Air
        | BlockKind::ShortGrass
        | BlockKind::Fern
        | BlockKind::DeadBush
        | BlockKind::Water
        | BlockKind::Lava
        | BlockKind::Bedrock
        | BlockKind::NetherPortal
        | BlockKind::EndPortal
        | BlockKind::OakLeaves
        | BlockKind::BirchLeaves
        | BlockKind::SpruceLeaves => return None,
    };
    let count = match kind {
        ItemKind::Redstone | ItemKind::LapisLazuli => 4,
        _ => 1,
    };
    Some(ItemStack {
        kind,
        count,
        damage: 0,
    })
}

fn block_drop_at(
    kind: BlockKind,
    tool: Option<ItemKind>,
    position: BlockPosition,
    tick: u64,
) -> Option<ItemStack> {
    if kind != BlockKind::Gravel {
        return block_drop(kind, tool);
    }
    let mut roll = tick
        ^ (position.x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (position.y as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (position.z as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    roll ^= roll >> 30;
    roll = roll.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    roll ^= roll >> 27;
    Some(ItemStack {
        kind: if roll % 10 == 0 {
            ItemKind::Flint
        } else {
            ItemKind::Gravel
        },
        count: 1,
        damage: 0,
    })
}

fn mining_ticks(kind: BlockKind, tool: Option<ItemKind>) -> u64 {
    if is_pickaxe_block(kind) && tool.is_some_and(is_pickaxe) {
        return match tool.map_or(0, pickaxe_tier) {
            1 => 8,
            2 => 6,
            3 => 4,
            _ => 3,
        };
    }
    if is_shovel_block(kind) && tool.is_some_and(is_shovel) {
        return match tool.map_or(0, tool_tier) {
            1 => 3,
            2 => 2,
            _ => 1,
        };
    }
    if is_axe_block(kind) && tool.is_some_and(is_axe) {
        return match tool.map_or(0, tool_tier) {
            1 => 8,
            2 => 6,
            _ => 4,
        };
    }
    match kind {
        BlockKind::ShortGrass | BlockKind::Fern | BlockKind::DeadBush => 0,
        BlockKind::OakLeaves | BlockKind::BirchLeaves | BlockKind::SpruceLeaves => 4,
        BlockKind::Dirt
        | BlockKind::Grass
        | BlockKind::Sand
        | BlockKind::Gravel
        | BlockKind::SoulSand
        | BlockKind::SnowBlock => 12,
        kind if is_pickaxe_block(kind) => 30,
        kind if is_axe_block(kind) => 30,
        BlockKind::Obsidian => 250,
        BlockKind::Air
        | BlockKind::Water
        | BlockKind::Lava
        | BlockKind::Bedrock
        | BlockKind::NetherPortal
        | BlockKind::EndPortal => u64::MAX,
        _ => 30,
    }
}

fn is_tool(kind: ItemKind) -> bool {
    is_pickaxe(kind) || is_axe(kind) || is_shovel(kind)
}

fn is_pickaxe(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::WoodenPickaxe
            | ItemKind::StonePickaxe
            | ItemKind::IronPickaxe
            | ItemKind::DiamondPickaxe
    )
}

fn is_axe(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::WoodenAxe | ItemKind::StoneAxe | ItemKind::IronAxe | ItemKind::DiamondAxe
    )
}

fn is_shovel(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::WoodenShovel
            | ItemKind::StoneShovel
            | ItemKind::IronShovel
            | ItemKind::DiamondShovel
    )
}

fn is_sword(kind: ItemKind) -> bool {
    matches!(
        kind,
        ItemKind::WoodenSword | ItemKind::StoneSword | ItemKind::IronSword | ItemKind::DiamondSword
    )
}

fn tool_tier(kind: ItemKind) -> u8 {
    match kind {
        ItemKind::WoodenPickaxe
        | ItemKind::WoodenAxe
        | ItemKind::WoodenShovel
        | ItemKind::WoodenSword => 1,
        ItemKind::StonePickaxe
        | ItemKind::StoneAxe
        | ItemKind::StoneShovel
        | ItemKind::StoneSword => 2,
        ItemKind::IronPickaxe | ItemKind::IronAxe | ItemKind::IronShovel | ItemKind::IronSword => 3,
        ItemKind::DiamondPickaxe
        | ItemKind::DiamondAxe
        | ItemKind::DiamondShovel
        | ItemKind::DiamondSword => 4,
        _ => 0,
    }
}

fn pickaxe_tier(kind: ItemKind) -> u8 {
    if is_pickaxe(kind) {
        tool_tier(kind)
    } else {
        0
    }
}

fn is_pickaxe_block(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Stone
            | BlockKind::CoalOre
            | BlockKind::IronOre
            | BlockKind::CopperOre
            | BlockKind::GoldOre
            | BlockKind::RedstoneOre
            | BlockKind::LapisOre
            | BlockKind::DiamondOre
            | BlockKind::Cobblestone
            | BlockKind::Sandstone
            | BlockKind::Netherrack
            | BlockKind::Basalt
            | BlockKind::EndStone
            | BlockKind::Obsidian
            | BlockKind::StoneBricks
            | BlockKind::Furnace
    )
}

fn is_shovel_block(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::Dirt
            | BlockKind::Grass
            | BlockKind::Sand
            | BlockKind::Gravel
            | BlockKind::SoulSand
            | BlockKind::SnowBlock
    )
}

fn is_axe_block(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::OakLog
            | BlockKind::BirchLog
            | BlockKind::SpruceLog
            | BlockKind::OakPlanks
            | BlockKind::CraftingTable
            | BlockKind::Chest
    )
}

fn mob_drop(mob: &MobSnapshot, tick: u64) -> Option<ItemStack> {
    let roll = (u64::from(mob.entity_id.unsigned_abs()) ^ tick) as u8;
    let (kind, count) = match mob.kind {
        MobKind::Zombie => (ItemKind::RottenFlesh, roll % 3),
        MobKind::Cow => (ItemKind::RawBeef, 1 + roll % 3),
        MobKind::Pig => (ItemKind::Porkchop, 1 + roll % 3),
    };
    (count > 0).then_some(ItemStack {
        kind,
        count,
        damage: 0,
    })
}

fn item_protocol_id(kind: ItemKind) -> i32 {
    match kind {
        ItemKind::Stone => 1,
        ItemKind::Cobblestone => 62,
        ItemKind::Dirt => 55,
        ItemKind::Sand => 86,
        ItemKind::Gravel => 90,
        ItemKind::Sandstone => 225,
        ItemKind::SnowBlock => 367,
        ItemKind::Netherrack => 387,
        ItemKind::SoulSand => 388,
        ItemKind::Basalt => 390,
        ItemKind::EndStone => 463,
        ItemKind::Obsidian => 349,
        ItemKind::StoneBricks => 403,
        ItemKind::OakPlanks => 63,
        ItemKind::OakLog => 161,
        ItemKind::BirchLog => 163,
        ItemKind::SpruceLog => 162,
        ItemKind::Apple => 921,
        ItemKind::Stick => 974,
        ItemKind::CraftingTable => 360,
        ItemKind::Furnace => 362,
        ItemKind::Chest => 359,
        ItemKind::IronIngot => 932,
        ItemKind::CopperIngot => 934,
        ItemKind::GoldIngot => 936,
        ItemKind::Flint => 1010,
        ItemKind::FlintAndSteel => 919,
        ItemKind::Bucket => 1040,
        ItemKind::MilkBucket => 1046,
        ItemKind::WoodenPickaxe => 941,
        ItemKind::WoodenAxe => 942,
        ItemKind::WoodenShovel => 940,
        ItemKind::WoodenSword => 939,
        ItemKind::StoneSword => 949,
        ItemKind::StoneShovel => 950,
        ItemKind::StonePickaxe => 951,
        ItemKind::StoneAxe => 952,
        ItemKind::IronSword => 959,
        ItemKind::IronShovel => 960,
        ItemKind::IronPickaxe => 961,
        ItemKind::IronAxe => 962,
        ItemKind::DiamondSword => 964,
        ItemKind::DiamondShovel => 965,
        ItemKind::DiamondPickaxe => 966,
        ItemKind::DiamondAxe => 967,
        ItemKind::Shield => 1325,
        ItemKind::RawBeef => 1139,
        ItemKind::Porkchop => 1011,
        ItemKind::CookedBeef => 1140,
        ItemKind::CookedPorkchop => 1012,
        ItemKind::RottenFlesh => 1143,
        ItemKind::Coal => 924,
        ItemKind::Charcoal => 925,
        ItemKind::RawIron => 931,
        ItemKind::RawCopper => 933,
        ItemKind::RawGold => 935,
        ItemKind::Redstone => 745,
        ItemKind::LapisLazuli => 928,
        ItemKind::Diamond => 926,
        ItemKind::IronHelmet => 994,
        ItemKind::IronChestplate => 995,
        ItemKind::IronLeggings => 996,
        ItemKind::IronBoots => 997,
        ItemKind::DiamondHelmet => 998,
        ItemKind::DiamondChestplate => 999,
        ItemKind::DiamondLeggings => 1000,
        ItemKind::DiamondBoots => 1001,
    }
}

fn iron_armor() -> [Option<ItemStack>; 4] {
    [
        Some(ItemStack {
            kind: ItemKind::IronBoots,
            count: 1,
            damage: 0,
        }),
        Some(ItemStack {
            kind: ItemKind::IronLeggings,
            count: 1,
            damage: 0,
        }),
        Some(ItemStack {
            kind: ItemKind::IronChestplate,
            count: 1,
            damage: 0,
        }),
        Some(ItemStack {
            kind: ItemKind::IronHelmet,
            count: 1,
            damage: 0,
        }),
    ]
}

fn equipment_items(equipment: PlayerEquipment) -> [Option<ItemStack>; 6] {
    [
        equipment.main_hand,
        equipment.off_hand,
        equipment.armor[0],
        equipment.armor[1],
        equipment.armor[2],
        equipment.armor[3],
    ]
}

fn encode_player_equipment(entity_id: i32, equipment: PlayerEquipment) -> Vec<u8> {
    let items = equipment_items(equipment);
    let protocol_slots = [0_u8, 1, 2, 3, 4, 5];
    let entries: Vec<_> = protocol_slots
        .into_iter()
        .zip(items)
        .enumerate()
        .map(|(index, (slot, stack))| {
            let (count, item_id, damage) = stack.map_or((0, 0, 0), |stack| {
                (stack.count, item_protocol_id(stack.kind), stack.damage)
            });
            (
                slot,
                count,
                item_id,
                damage,
                index == 0 && equipment.main_hand_sharpness > 0,
            )
        })
        .collect();
    encode_set_equipment_with_glint(entity_id, &entries)
}

fn attack_damage(equipment: PlayerEquipment) -> f32 {
    let base = equipment.main_hand.map_or(1.0, |stack| match stack.kind {
        ItemKind::WoodenSword => 4.0,
        ItemKind::WoodenAxe => 7.0,
        ItemKind::WoodenPickaxe => 2.0,
        ItemKind::WoodenShovel => 2.5,
        ItemKind::StoneSword => 5.0,
        ItemKind::StoneAxe => 9.0,
        ItemKind::StonePickaxe => 3.0,
        ItemKind::StoneShovel => 3.5,
        ItemKind::IronSword => 6.0,
        ItemKind::IronAxe => 9.0,
        ItemKind::IronPickaxe => 4.0,
        ItemKind::IronShovel => 4.5,
        ItemKind::DiamondSword => 7.0,
        ItemKind::DiamondAxe => 9.0,
        ItemKind::DiamondPickaxe => 5.0,
        ItemKind::DiamondShovel => 5.5,
        _ => 1.0,
    });
    base + if equipment.main_hand_sharpness == 0 {
        0.0
    } else {
        0.5 + 0.5 * f32::from(equipment.main_hand_sharpness)
    }
}

fn attack_cooldown_ticks(equipment: Option<PlayerEquipment>) -> f32 {
    let attacks_per_second =
        equipment
            .and_then(|equipment| equipment.main_hand)
            .map_or(4.0, |stack| match stack.kind {
                ItemKind::WoodenSword => 1.6,
                ItemKind::WoodenAxe => 0.8,
                ItemKind::WoodenPickaxe => 1.2,
                ItemKind::WoodenShovel => 1.0,
                ItemKind::StoneSword | ItemKind::IronSword | ItemKind::DiamondSword => 1.6,
                ItemKind::StoneAxe => 0.8,
                ItemKind::IronAxe => 0.9,
                ItemKind::DiamondAxe => 1.0,
                ItemKind::StonePickaxe | ItemKind::IronPickaxe | ItemKind::DiamondPickaxe => 1.2,
                ItemKind::StoneShovel | ItemKind::IronShovel | ItemKind::DiamondShovel => 1.0,
                _ => 4.0,
            });
    20.0 / attacks_per_second
}

fn attack_charge(elapsed_ticks: u64, equipment: Option<PlayerEquipment>) -> f32 {
    ((elapsed_ticks as f32 + 0.5) / attack_cooldown_ticks(equipment)).clamp(0.0, 1.0)
}

fn attack_damage_scale(charge: f32) -> f32 {
    0.2 + charge.clamp(0.0, 1.0).powi(2) * 0.8
}

fn item_block(kind: ItemKind) -> Option<BlockKind> {
    match kind {
        ItemKind::Stone => Some(BlockKind::Stone),
        ItemKind::Dirt => Some(BlockKind::Dirt),
        ItemKind::Sand => Some(BlockKind::Sand),
        ItemKind::Gravel => Some(BlockKind::Gravel),
        ItemKind::Sandstone => Some(BlockKind::Sandstone),
        ItemKind::SnowBlock => Some(BlockKind::SnowBlock),
        ItemKind::Netherrack => Some(BlockKind::Netherrack),
        ItemKind::SoulSand => Some(BlockKind::SoulSand),
        ItemKind::Basalt => Some(BlockKind::Basalt),
        ItemKind::EndStone => Some(BlockKind::EndStone),
        ItemKind::Obsidian => Some(BlockKind::Obsidian),
        ItemKind::StoneBricks => Some(BlockKind::StoneBricks),
        ItemKind::OakLog => Some(BlockKind::OakLog),
        ItemKind::BirchLog => Some(BlockKind::BirchLog),
        ItemKind::SpruceLog => Some(BlockKind::SpruceLog),
        ItemKind::OakPlanks => Some(BlockKind::OakPlanks),
        ItemKind::CraftingTable => Some(BlockKind::CraftingTable),
        ItemKind::Furnace => Some(BlockKind::Furnace),
        ItemKind::Chest => Some(BlockKind::Chest),
        ItemKind::Cobblestone
        | ItemKind::Stick
        | ItemKind::IronIngot
        | ItemKind::CopperIngot
        | ItemKind::GoldIngot
        | ItemKind::Flint
        | ItemKind::FlintAndSteel
        | ItemKind::Bucket
        | ItemKind::MilkBucket
        | ItemKind::Apple
        | ItemKind::WoodenPickaxe
        | ItemKind::WoodenAxe
        | ItemKind::WoodenShovel
        | ItemKind::WoodenSword
        | ItemKind::StonePickaxe
        | ItemKind::StoneAxe
        | ItemKind::StoneShovel
        | ItemKind::StoneSword
        | ItemKind::IronPickaxe
        | ItemKind::IronAxe
        | ItemKind::IronShovel
        | ItemKind::IronSword
        | ItemKind::DiamondPickaxe
        | ItemKind::DiamondAxe
        | ItemKind::DiamondShovel
        | ItemKind::DiamondSword
        | ItemKind::Shield
        | ItemKind::RawBeef
        | ItemKind::Porkchop
        | ItemKind::CookedBeef
        | ItemKind::CookedPorkchop
        | ItemKind::RottenFlesh
        | ItemKind::Coal
        | ItemKind::Charcoal
        | ItemKind::RawIron
        | ItemKind::RawCopper
        | ItemKind::RawGold
        | ItemKind::Redstone
        | ItemKind::LapisLazuli
        | ItemKind::Diamond
        | ItemKind::IronHelmet
        | ItemKind::IronChestplate
        | ItemKind::IronLeggings
        | ItemKind::IronBoots => None,
        ItemKind::DiamondHelmet
        | ItemKind::DiamondChestplate
        | ItemKind::DiamondLeggings
        | ItemKind::DiamondBoots => None,
    }
}

fn biome_protocol_id(kind: BiomeKind) -> i32 {
    kind.protocol_id()
}

fn terrain_protocol_states(terrain: carbon_api::TerrainProfile) -> ChunkTerrainStates {
    ChunkTerrainStates {
        surface: block_state_id(terrain.surface),
        filler: block_state_id(terrain.filler),
        foundation: block_state_id(terrain.foundation),
        bedrock: block_state_id(terrain.bedrock),
    }
}

fn portal_destination(
    state: &dyn ServerApi,
    dimension: DimensionKind,
    feet: BlockPosition,
) -> Option<DimensionKind> {
    let head = BlockPosition {
        y: feet.y.saturating_add(1),
        ..feet
    };
    let contains = |kind| {
        state.dimension_block_at(dimension, feet) == kind
            || state.dimension_block_at(dimension, head) == kind
    };
    if contains(BlockKind::EndPortal) {
        return match dimension {
            DimensionKind::Overworld => Some(DimensionKind::End),
            DimensionKind::End => Some(DimensionKind::Overworld),
            DimensionKind::Nether => None,
        };
    }
    if contains(BlockKind::NetherPortal) {
        return match dimension {
            DimensionKind::Overworld => Some(DimensionKind::Nether),
            DimensionKind::Nether => Some(DimensionKind::Overworld),
            DimensionKind::End => None,
        };
    }
    None
}

fn command_dimension(command: &str) -> Option<DimensionKind> {
    match command {
        "dimension_overworld" => Some(DimensionKind::Overworld),
        "dimension_nether" => Some(DimensionKind::Nether),
        "dimension_end" => Some(DimensionKind::End),
        _ => None,
    }
}

fn dimension_seed(seed: i64, dimension: DimensionKind) -> i64 {
    match dimension {
        DimensionKind::Overworld => seed,
        DimensionKind::Nether => seed ^ 0x4e37_4e52,
        DimensionKind::End => seed ^ 0xe11d,
    }
}

fn dimension_sea_level(dimension: DimensionKind) -> i32 {
    match dimension {
        DimensionKind::Overworld => 63,
        DimensionKind::Nether => 32,
        DimensionKind::End => 0,
    }
}

fn adjacent_position(position: BlockPosition, face: i32) -> BlockPosition {
    let (dx, dy, dz) = match face {
        0 => (0, -1, 0),
        1 => (0, 1, 0),
        2 => (0, 0, -1),
        3 => (0, 0, 1),
        4 => (-1, 0, 0),
        5 => (1, 0, 0),
        _ => (0, 0, 0),
    };
    BlockPosition {
        x: position.x.saturating_add(dx),
        y: position.y.saturating_add(dy),
        z: position.z.saturating_add(dz),
    }
}

fn player_can_reach(state: &dyn ServerApi, player_id: Uuid, target: BlockPosition) -> bool {
    state.players().into_iter().any(|player| {
        if player.id != player_id {
            return false;
        }
        let dx = i64::from(player.position.x - target.x);
        let dy = i64::from(player.position.y - target.y);
        let dz = i64::from(player.position.z - target.z);
        dx * dx + dy * dy + dz * dz <= 49
    })
}

fn player_can_reach_entity(state: &dyn ServerApi, player_id: Uuid, target: &MobSnapshot) -> bool {
    state.players().into_iter().any(|player| {
        if player.id != player_id {
            return false;
        }
        let dx = f64::from(player.position.x) + 0.5 - target.position.x;
        let dy = f64::from(player.position.y) + 1.0 - target.position.y;
        let dz = f64::from(player.position.z) + 0.5 - target.position.z;
        dx * dx + dy * dy + dz * dz <= 16.0
    })
}

fn player_can_reach_player(state: &dyn ServerApi, attacker_id: Uuid, target_id: Uuid) -> bool {
    let transforms = state.player_transforms();
    let Some(attacker) = transforms.iter().find(|player| player.id == attacker_id) else {
        return false;
    };
    let Some(target) = transforms.iter().find(|player| player.id == target_id) else {
        return false;
    };
    let dx = attacker.position.x - target.position.x;
    let dy = attacker.position.y + 1.0 - (target.position.y + 1.0);
    let dz = attacker.position.z - target.position.z;
    dx * dx + dy * dy + dz * dz <= 16.0
}

fn apply_sweeping_attack(
    state: &dyn ServerApi,
    dimension: DimensionKind,
    attacker_id: Uuid,
    center: EntityPosition,
    excluded_player: Option<Uuid>,
    excluded_mob: Option<i32>,
    tick: u64,
) {
    let transforms: HashMap<_, _> = state
        .player_transforms()
        .into_iter()
        .map(|transform| (transform.id, transform))
        .collect();
    for player in state.players().into_iter().filter(|player| {
        player.id != attacker_id
            && Some(player.id) != excluded_player
            && player_dimension(player) == dimension
    }) {
        let Some(transform) = transforms.get(&player.id) else {
            continue;
        };
        let dx = transform.position.x - center.x;
        let dy = transform.position.y - center.y;
        let dz = transform.position.z - center.z;
        if dx * dx + dz * dz <= 2.25
            && dy.abs() <= 1.5
            && state.damage_player_combat(player.id, 1.0)
        {
            state.knockback_player(attacker_id, player.id, 0.4);
        }
    }
    for mob in state.dimension_mobs(dimension) {
        if Some(mob.entity_id) == excluded_mob {
            continue;
        }
        let dx = mob.position.x - center.x;
        let dy = mob.position.y - center.y;
        let dz = mob.position.z - center.z;
        if dx * dx + dz * dz > 2.25 || dy.abs() > 1.5 {
            continue;
        }
        let Some(damaged) = state.damage_dimension_mob(dimension, mob.entity_id, 1.0) else {
            continue;
        };
        if damaged.health <= 0.0 {
            if let Some(stack) = mob_drop(&damaged, tick) {
                state.drop_dimension_item(
                    dimension,
                    stack,
                    BlockPosition {
                        x: floor_to_i32(damaged.position.x),
                        y: floor_to_i32(damaged.position.y),
                        z: floor_to_i32(damaged.position.z),
                    },
                );
            }
        }
    }
}

fn floor_to_i32(value: f64) -> i32 {
    value
        .floor()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

fn requires_spawn_rescue(x: f64, y: f64, z: f64) -> bool {
    const WORLD_BORDER: f64 = 29_999_984.0;
    !(-WORLD_BORDER..=WORLD_BORDER).contains(&x)
        || !(-WORLD_BORDER..=WORLD_BORDER).contains(&z)
        || !(-32.0..=320.0).contains(&y)
}

async fn disconnect_login(stream: &mut Connection, message: &str) -> anyhow::Result<()> {
    let reason = serde_json::json!({ "text": message });
    stream
        .write_all(&frame_packet(0, &encode_string(&reason.to_string())))
        .await?;
    stream.shutdown().await?;
    Ok(())
}

fn login_denial(config: &ServerConfig, state: &dyn ServerApi, username: &str) -> Option<String> {
    if let Some(ban) = state.active_ban(username) {
        let expiry = ban.expires_at_unix.map_or_else(
            || "This ban is permanent.".into(),
            |expires| {
                let remaining = expires.saturating_sub(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                );
                format!("This ban expires in {remaining} seconds.")
            },
        );
        return Some(format!("You are banned: {} {expiry}", ban.reason));
    }
    let operator = state.is_operator(username);
    if config.allowlist_enabled && !operator && !state.is_allowlisted(username) {
        return Some("You are not on this Carbon server's allowlist.".into());
    }
    if !operator && u32::try_from(state.players().len()).unwrap_or(u32::MAX) >= config.max_players {
        return Some("This Carbon server is full.".into());
    }
    None
}

fn format_player_id(id: [u8; 16]) -> String {
    let hex = id.map(|byte| format!("{byte:02x}")).concat();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn offline_player_id(username: &str) -> [u8; 16] {
    let mut id: [u8; 16] = Md5::digest(format!("OfflinePlayer:{username}").as_bytes()).into();
    id[6] = (id[6] & 0x0f) | 0x30;
    id[8] = (id[8] & 0x3f) | 0x80;
    id
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{
        apply_sweeping_attack, attack_charge, attack_cooldown_ticks, attack_damage,
        attack_damage_scale, block_drop, block_drop_at, collect_matching_storage,
        command_dimension, craft_from_grid, craft_from_table, crafting_recipe,
        crafting_table_menu_slot, crafting_table_recipe, distribute_cursor, login_denial,
        mining_ticks, offline_player_id, player_entity_id, player_menu_slot, quick_move_slot,
        requires_spawn_rescue, return_inventory_cursor, shift_craft_from_table, swap_player_slots,
        throw_from_slot,
    };

    #[test]
    fn flint_and_steel_recipe_is_shapeless_in_both_crafting_grids() {
        let stack = |kind| InventoryCursor {
            stack: Some(ItemStack {
                kind,
                count: 1,
                damage: 0,
            }),
            sharpness_level: 0,
        };
        let mut player_grid = [InventoryCursor::default(); 4];
        player_grid[0] = stack(ItemKind::Flint);
        player_grid[3] = stack(ItemKind::IronIngot);
        assert_eq!(
            crafting_recipe(&player_grid).unwrap().1.kind,
            ItemKind::FlintAndSteel
        );

        let mut table_grid = [InventoryCursor::default(); 9];
        table_grid[2] = stack(ItemKind::IronIngot);
        table_grid[7] = stack(ItemKind::Flint);
        assert_eq!(
            crafting_table_recipe(&table_grid).unwrap().1.kind,
            ItemKind::FlintAndSteel
        );
    }

    #[test]
    fn bucket_recipe_accepts_both_vertical_positions() {
        for indices in [[0_usize, 2, 4], [3, 5, 7]] {
            let mut grid = [InventoryCursor::default(); 9];
            for index in indices {
                grid[index].stack = Some(ItemStack {
                    kind: ItemKind::IronIngot,
                    count: 1,
                    damage: 0,
                });
            }
            let (_, output) = crafting_table_recipe(&grid).expect("bucket recipe");
            assert_eq!(
                output,
                ItemStack {
                    kind: ItemKind::Bucket,
                    count: 1,
                    damage: 0,
                }
            );
        }
    }
    use crate::state::ServerState;
    use carbon_api::{
        BlockKind, BlockPosition, DimensionKind, EntityPosition, GameMode, InventoryCursor,
        ItemKind, ItemStack, PlayerEquipment, PlayerInventorySlot, PlayerSnapshot, ServerApi,
    };
    use carbon_config::ServerConfig;
    use tokio::sync::watch;

    #[test]
    fn play_command_visibility_and_execution_share_live_permission_rules() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let visible = super::visible_play_commands(&state, "Helper");
        assert!(visible.contains(&"craft_planks"));
        assert!(visible.contains(&"dimension_nether"));
        for label in ["say", "equip_iron", "equip_shield", "enchant_sharpness"] {
            assert!(!visible.contains(&label));
            assert!(!super::play_command_allowed(&state, "Helper", label));
        }
        assert!(super::play_command_allowed(
            &state,
            "Helper",
            "craft_wooden_pickaxe"
        ));
        assert!(!super::play_command_allowed(&state, "Helper", "stop"));
        state
            .grant_permission("helper", "carbon.command.*")
            .unwrap();
        state
            .grant_permission("helper", "!carbon.command.equip_iron")
            .unwrap();
        let visible = super::visible_play_commands(&state, "HELPER");
        assert!(visible.contains(&"say"));
        assert!(!visible.contains(&"equip_iron"));
        for (label, _) in super::PLAY_COMMANDS {
            assert_eq!(
                visible.contains(label),
                super::play_command_allowed(&state, "helper", label)
            );
        }
        assert!(!super::visible_play_commands(&state, "Other").contains(&"say"));
        state.grant_permission("helper", "!*").unwrap();
        assert!(!super::play_command_allowed(&state, "helper", "say"));
        state.set_operator("helper", true).unwrap();
        assert_eq!(
            super::visible_play_commands(&state, "helper").len(),
            super::PLAY_COMMANDS.len()
        );
        state.set_operator("helper", false).unwrap();
        assert!(!super::play_command_allowed(&state, "helper", "say"));
    }

    #[test]
    fn plant_states_and_prototype_breaking_are_explicit() {
        for (kind, id) in [
            (BlockKind::ShortGrass, 2248),
            (BlockKind::Fern, 2249),
            (BlockKind::DeadBush, 2250),
        ] {
            assert_eq!(super::block_state_id(kind), id);
            assert_eq!(super::mining_ticks(kind, None), 0);
            assert_eq!(super::block_drop(kind, None), None);
            assert!(kind.is_replaceable());
        }
        assert!(!BlockKind::Stone.is_replaceable());
        assert!(!BlockKind::Water.is_replaceable());
        assert_eq!(super::block_state_id(BlockKind::Lava), 102);
        assert_eq!(super::mining_ticks(BlockKind::Lava, None), u64::MAX);
        assert_eq!(super::block_drop(BlockKind::Lava, None), None);
        assert!(!BlockKind::Lava.is_replaceable());
    }

    #[test]
    fn generated_portals_route_only_between_supported_dimensions() {
        let state = ServerState::new("world".into(), 42, watch::channel(false).0);
        assert_eq!(super::block_state_id(BlockKind::EndPortal), 9468);
        assert_eq!(super::mining_ticks(BlockKind::EndPortal, None), u64::MAX);
        assert_eq!(super::block_drop(BlockKind::EndPortal, None), None);
        let end_portal = BlockPosition { x: 8, y: 65, z: -8 };
        assert_eq!(
            super::portal_destination(&state, DimensionKind::Overworld, end_portal),
            Some(DimensionKind::End)
        );
        assert_eq!(
            super::portal_destination(&state, DimensionKind::End, end_portal),
            Some(DimensionKind::Overworld)
        );
        assert_eq!(
            super::portal_destination(&state, DimensionKind::Nether, end_portal),
            None
        );
        let nether_surface = state.ensure_dimension_chunk_surface(
            DimensionKind::Nether,
            carbon_api::ChunkPosition { x: 0, z: -1 },
        );
        let nether_portal = BlockPosition {
            x: 0,
            y: i32::from(nether_surface[128]) + 1,
            z: -8,
        };
        assert_eq!(
            super::portal_destination(
                &state,
                DimensionKind::Overworld,
                BlockPosition { x: 0, y: 65, z: -8 }
            ),
            Some(DimensionKind::Nether)
        );
        assert_eq!(
            super::portal_destination(&state, DimensionKind::Nether, nether_portal),
            Some(DimensionKind::Overworld)
        );
        assert_eq!(
            super::portal_destination(&state, DimensionKind::End, nether_portal),
            None
        );
    }

    #[tokio::test]
    async fn command_tree_refresh_sends_only_changed_permission_snapshots() {
        use tokio::io::AsyncReadExt;
        use tokio::net::{TcpListener, TcpStream};
        use tokio::time::{timeout, Duration};
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let mut server = super::Connection::new(server);
        let mut advertised = super::visible_play_commands(&state, "Helper");
        assert!(
            !super::refresh_play_commands(&mut server, &state, "Helper", &mut advertised)
                .await
                .unwrap()
        );
        assert!(timeout(Duration::from_millis(25), client.read_u8())
            .await
            .is_err());
        // Grant, explicit deny, deny removal, revocation, op, and deop must
        // each publish the exact current tree without reconnecting.
        for step in 0..6 {
            match step {
                0 => {
                    state
                        .grant_permission("Helper", "carbon.command.say")
                        .unwrap();
                }
                1 => {
                    state
                        .grant_permission("Helper", "!carbon.command.say")
                        .unwrap();
                }
                2 => {
                    state
                        .revoke_permission("Helper", "!carbon.command.say")
                        .unwrap();
                }
                3 => {
                    state
                        .revoke_permission("Helper", "carbon.command.say")
                        .unwrap();
                }
                4 => {
                    state.set_operator("Helper", true).unwrap();
                }
                _ => {
                    state.set_operator("Helper", false).unwrap();
                }
            }
            assert!(
                super::refresh_play_commands(&mut server, &state, "Helper", &mut advertised)
                    .await
                    .unwrap()
            );
            let packet = timeout(
                Duration::from_secs(2),
                carbon_protocol::read_frame(&mut client),
            )
            .await
            .unwrap()
            .unwrap();
            let expected =
                carbon_protocol::encode_commands(&super::visible_play_commands(&state, "Helper"));
            let (_, prefix) = carbon_protocol::decode_varint(&expected).unwrap();
            assert_eq!(packet.as_slice(), &expected[prefix..]);
            assert_eq!(advertised.contains(&"say"), matches!(step, 0 | 2 | 4));
            assert!(
                !super::refresh_play_commands(&mut server, &state, "Helper", &mut advertised)
                    .await
                    .unwrap()
            );
        }
        assert!(timeout(Duration::from_millis(25), client.read_u8())
            .await
            .is_err());
    }

    #[test]
    fn login_access_control_enforces_bans_allowlist_capacity_and_operator_bypass() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let config = ServerConfig {
            max_players: 1,
            allowlist_enabled: true,
            ..ServerConfig::default()
        };

        state.set_banned("Blocked", true).unwrap();
        assert!(login_denial(&config, &state, "blocked")
            .unwrap()
            .contains("banned"));
        assert!(login_denial(&config, &state, "Stranger")
            .unwrap()
            .contains("allowlist"));
        state.set_allowlisted("Friend", true).unwrap();
        assert_eq!(login_denial(&config, &state, "friend"), None);

        assert!(state.add_player(PlayerSnapshot {
            id: uuid::Uuid::new_v4(),
            name: "Friend".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: GameMode::Survival,
        }));
        state.set_allowlisted("Second", true).unwrap();
        assert!(login_denial(&config, &state, "Second")
            .unwrap()
            .contains("full"));
        state.set_operator("Owner", true).unwrap();
        assert_eq!(login_denial(&config, &state, "Owner"), None);
    }

    #[test]
    fn derives_the_vanilla_offline_player_id() {
        assert_eq!(
            offline_player_id("Notch"),
            [
                0xb5, 0x0a, 0xd3, 0x85, 0x82, 0x9d, 0x31, 0x41, 0xa2, 0x16, 0x7e, 0x7d, 0x75, 0x39,
                0xba, 0x7f,
            ]
        );
    }

    #[test]
    fn remote_player_identity_is_stable() {
        let id = uuid::Uuid::from_bytes(offline_player_id("Notch"));
        assert_eq!(player_entity_id(id), 0x350a_d385);
    }

    #[test]
    fn maps_verified_player_menu_slots_to_raw_inventory_slots() {
        assert_eq!(player_menu_slot(5), Some(PlayerInventorySlot::Armor(3)));
        assert_eq!(player_menu_slot(8), Some(PlayerInventorySlot::Armor(0)));
        assert_eq!(player_menu_slot(9), Some(PlayerInventorySlot::Storage(9)));
        assert_eq!(player_menu_slot(36), Some(PlayerInventorySlot::Storage(0)));
        assert_eq!(player_menu_slot(44), Some(PlayerInventorySlot::Storage(8)));
        assert_eq!(player_menu_slot(45), Some(PlayerInventorySlot::OffHand));
        assert_eq!(player_menu_slot(0), None);
        assert_eq!(player_menu_slot(-999), None);
        assert_eq!(
            crafting_table_menu_slot(10),
            Some(PlayerInventorySlot::Storage(9))
        );
        assert_eq!(
            crafting_table_menu_slot(37),
            Some(PlayerInventorySlot::Storage(0))
        );
        assert_eq!(
            crafting_table_menu_slot(45),
            Some(PlayerInventorySlot::Storage(8))
        );
        assert_eq!(crafting_table_menu_slot(9), None);
    }

    #[test]
    fn closing_inventory_returns_the_cursor_stack_to_storage() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let id = uuid::Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Cursor".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::OakLog, 5));
        let cursor = state
            .click_player_inventory_slot(
                id,
                PlayerInventorySlot::Storage(0),
                InventoryCursor::default(),
                false,
            )
            .unwrap();
        assert!(state.inventory(id).unwrap().slots[0].is_none());
        assert_eq!(
            return_inventory_cursor(&state, id, DimensionKind::Overworld, cursor),
            InventoryCursor::default()
        );
        assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().count, 5);
    }

    #[test]
    fn native_two_by_two_grid_crafts_exact_outputs_and_inputs() {
        let mut grid = [InventoryCursor::default(); 4];
        grid[0].stack = Some(ItemStack {
            kind: ItemKind::OakLog,
            count: 2,
            damage: 0,
        });
        let mut cursor = InventoryCursor::default();
        assert!(craft_from_grid(&mut grid, &mut cursor));
        assert_eq!(cursor.stack.unwrap().count, 4);
        assert_eq!(grid[0].stack.unwrap().count, 1);
        assert!(craft_from_grid(&mut grid, &mut cursor));
        assert_eq!(cursor.stack.unwrap().count, 8);
        assert!(grid.iter().all(|slot| slot.stack.is_none()));

        grid[0].stack = Some(ItemStack {
            kind: ItemKind::OakPlanks,
            count: 1,
            damage: 0,
        });
        grid[2].stack = grid[0].stack;
        cursor = InventoryCursor::default();
        assert!(craft_from_grid(&mut grid, &mut cursor));
        assert_eq!(cursor.stack.unwrap().kind, ItemKind::Stick);
        assert_eq!(cursor.stack.unwrap().count, 4);
    }

    #[test]
    fn crafting_table_matches_shaped_tools_weapons_and_shield() {
        let stack = |kind| InventoryCursor {
            stack: Some(ItemStack {
                kind,
                count: 1,
                damage: 0,
            }),
            sharpness_level: 0,
        };
        for (entries, expected) in [
            (
                vec![
                    (0, ItemKind::OakPlanks),
                    (1, ItemKind::OakPlanks),
                    (2, ItemKind::OakPlanks),
                    (3, ItemKind::OakPlanks),
                    (5, ItemKind::OakPlanks),
                    (6, ItemKind::OakPlanks),
                    (7, ItemKind::OakPlanks),
                    (8, ItemKind::OakPlanks),
                ],
                ItemKind::Chest,
            ),
            (
                vec![
                    (0, ItemKind::Cobblestone),
                    (1, ItemKind::Cobblestone),
                    (2, ItemKind::Cobblestone),
                    (4, ItemKind::Stick),
                    (7, ItemKind::Stick),
                ],
                ItemKind::StonePickaxe,
            ),
            (
                vec![
                    (0, ItemKind::IronIngot),
                    (1, ItemKind::IronIngot),
                    (3, ItemKind::IronIngot),
                    (4, ItemKind::Stick),
                    (7, ItemKind::Stick),
                ],
                ItemKind::IronAxe,
            ),
            (
                vec![
                    (0, ItemKind::IronIngot),
                    (1, ItemKind::IronIngot),
                    (2, ItemKind::IronIngot),
                    (3, ItemKind::IronIngot),
                    (5, ItemKind::IronIngot),
                ],
                ItemKind::IronHelmet,
            ),
            (
                vec![
                    (0, ItemKind::IronIngot),
                    (2, ItemKind::IronIngot),
                    (3, ItemKind::IronIngot),
                    (4, ItemKind::IronIngot),
                    (5, ItemKind::IronIngot),
                    (6, ItemKind::IronIngot),
                    (7, ItemKind::IronIngot),
                    (8, ItemKind::IronIngot),
                ],
                ItemKind::IronChestplate,
            ),
            (
                vec![
                    (0, ItemKind::Diamond),
                    (1, ItemKind::Diamond),
                    (2, ItemKind::Diamond),
                    (4, ItemKind::Stick),
                    (7, ItemKind::Stick),
                ],
                ItemKind::DiamondPickaxe,
            ),
            (
                vec![
                    (0, ItemKind::Diamond),
                    (2, ItemKind::Diamond),
                    (3, ItemKind::Diamond),
                    (4, ItemKind::Diamond),
                    (5, ItemKind::Diamond),
                    (6, ItemKind::Diamond),
                    (7, ItemKind::Diamond),
                    (8, ItemKind::Diamond),
                ],
                ItemKind::DiamondChestplate,
            ),
            (
                vec![
                    (0, ItemKind::OakPlanks),
                    (1, ItemKind::OakPlanks),
                    (2, ItemKind::OakPlanks),
                    (4, ItemKind::Stick),
                    (7, ItemKind::Stick),
                ],
                ItemKind::WoodenPickaxe,
            ),
            (
                vec![
                    (1, ItemKind::OakPlanks),
                    (2, ItemKind::OakPlanks),
                    (5, ItemKind::OakPlanks),
                    (4, ItemKind::Stick),
                    (7, ItemKind::Stick),
                ],
                ItemKind::WoodenAxe,
            ),
            (
                vec![
                    (0, ItemKind::OakPlanks),
                    (3, ItemKind::Stick),
                    (6, ItemKind::Stick),
                ],
                ItemKind::WoodenShovel,
            ),
            (
                vec![
                    (2, ItemKind::OakPlanks),
                    (5, ItemKind::OakPlanks),
                    (8, ItemKind::Stick),
                ],
                ItemKind::WoodenSword,
            ),
            (
                vec![
                    (0, ItemKind::OakPlanks),
                    (1, ItemKind::IronIngot),
                    (2, ItemKind::OakPlanks),
                    (3, ItemKind::OakPlanks),
                    (4, ItemKind::OakPlanks),
                    (5, ItemKind::OakPlanks),
                    (7, ItemKind::OakPlanks),
                ],
                ItemKind::Shield,
            ),
        ] {
            let mut grid = [InventoryCursor::default(); 9];
            for (index, kind) in entries {
                grid[index] = stack(kind);
            }
            assert_eq!(crafting_table_recipe(&grid).unwrap().1.kind, expected);
            let mut cursor = InventoryCursor::default();
            assert!(craft_from_table(&mut grid, &mut cursor));
            assert_eq!(cursor.stack.unwrap().kind, expected);
            assert!(grid.iter().all(|slot| slot.stack.is_none()));
        }
    }

    #[test]
    fn shift_crafting_repeats_until_ingredients_are_consumed() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let id = uuid::Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "TableCrafter".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: GameMode::Survival,
        }));
        let mut grid = [InventoryCursor::default(); 9];
        for index in [0, 1, 2] {
            grid[index].stack = Some(ItemStack {
                kind: ItemKind::OakPlanks,
                count: 2,
                damage: 0,
            });
        }
        for index in [4, 7] {
            grid[index].stack = Some(ItemStack {
                kind: ItemKind::Stick,
                count: 2,
                damage: 0,
            });
        }
        assert!(shift_craft_from_table(&state, id, &mut grid));
        let crafted = state
            .inventory(id)
            .unwrap()
            .slots
            .iter()
            .flatten()
            .filter(|stack| stack.kind == ItemKind::WoodenPickaxe)
            .count();
        assert_eq!(crafted, 2);
        assert!(grid.iter().all(|slot| slot.stack.is_none()));
    }

    #[test]
    fn advanced_inventory_gestures_preserve_exact_item_counts() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let id = uuid::Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Gestures".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::IronHelmet, 1));
        assert_eq!(
            quick_move_slot(&state, id, PlayerInventorySlot::Storage(0)),
            InventoryCursor::default()
        );
        assert_eq!(
            state.player_equipment(id).unwrap().armor[3].unwrap().kind,
            ItemKind::IronHelmet
        );

        assert!(state.give_item(id, ItemKind::OakLog, 5));
        assert!(state.give_item(id, ItemKind::Apple, 1));
        swap_player_slots(
            &state,
            id,
            PlayerInventorySlot::Storage(0),
            PlayerInventorySlot::Storage(1),
        );
        let inventory = state.inventory(id).unwrap();
        assert_eq!(inventory.slots[0].unwrap().kind, ItemKind::Apple);
        assert_eq!(inventory.slots[1].unwrap().kind, ItemKind::OakLog);
        assert_eq!(
            throw_from_slot(
                &state,
                id,
                DimensionKind::Overworld,
                PlayerInventorySlot::Storage(1),
                false,
            ),
            InventoryCursor::default()
        );
        assert_eq!(state.inventory(id).unwrap().slots[1].unwrap().count, 4);
        assert_eq!(state.items().last().unwrap().stack.count, 1);
    }

    #[test]
    fn drag_and_collect_all_do_not_duplicate_stacks() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let id = uuid::Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Collector".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::OakLog, 5));
        let cursor = state
            .click_player_inventory_slot(
                id,
                PlayerInventorySlot::Storage(0),
                InventoryCursor::default(),
                false,
            )
            .unwrap();
        let slots = HashSet::from([
            PlayerInventorySlot::Storage(1),
            PlayerInventorySlot::Storage(2),
        ]);
        assert_eq!(
            distribute_cursor(&state, id, cursor, &slots, false),
            InventoryCursor::default()
        );
        let inventory = state.inventory(id).unwrap();
        assert_eq!(
            inventory.slots[1].unwrap().count + inventory.slots[2].unwrap().count,
            5
        );

        assert!(state.give_item(id, ItemKind::OakLog, 59));
        let cursor = state
            .click_player_inventory_slot(
                id,
                PlayerInventorySlot::Storage(2),
                InventoryCursor::default(),
                false,
            )
            .unwrap_or_default();
        let collected = collect_matching_storage(&state, id, cursor);
        assert!(collected.stack.is_some());
        let stored: u16 = state
            .inventory(id)
            .unwrap()
            .slots
            .iter()
            .flatten()
            .map(|stack| u16::from(stack.count))
            .sum();
        assert_eq!(stored + u16::from(collected.stack.unwrap().count), 64);
    }

    #[test]
    fn held_weapons_provide_distinct_attack_damage() {
        let mut equipment = PlayerEquipment::default();
        assert_eq!(attack_damage(equipment), 1.0);
        equipment.main_hand = Some(ItemStack {
            kind: ItemKind::WoodenSword,
            count: 1,
            damage: 0,
        });
        assert_eq!(attack_damage(equipment), 4.0);
        equipment.main_hand_sharpness = 1;
        assert_eq!(attack_damage(equipment), 5.0);
        equipment.main_hand = Some(ItemStack {
            kind: ItemKind::WoodenAxe,
            count: 1,
            damage: 0,
        });
        equipment.main_hand_sharpness = 0;
        assert_eq!(attack_damage(equipment), 7.0);
    }

    #[test]
    fn weapon_attack_speed_scales_cooldown_damage() {
        let mut equipment = PlayerEquipment::default();
        assert_eq!(attack_cooldown_ticks(Some(equipment)), 5.0);
        equipment.main_hand = Some(ItemStack {
            kind: ItemKind::WoodenSword,
            count: 1,
            damage: 0,
        });
        assert_eq!(attack_cooldown_ticks(Some(equipment)), 12.5);
        assert_eq!(attack_charge(12, Some(equipment)), 1.0);
        assert!((attack_damage_scale(0.0) - 0.2).abs() < f32::EPSILON);
        assert!((attack_damage_scale(0.5) - 0.4).abs() < f32::EPSILON);
        assert_eq!(attack_damage_scale(1.0), 1.0);
    }

    #[test]
    fn sweeping_attack_damages_nearby_bystanders_only() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        let attacker = uuid::Uuid::new_v4();
        let primary = uuid::Uuid::new_v4();
        let bystander = uuid::Uuid::new_v4();
        for (id, name, x) in [
            (attacker, "Attacker", 100),
            (primary, "Primary", 101),
            (bystander, "Bystander", 102),
        ] {
            assert!(state.add_player(PlayerSnapshot {
                id,
                name: name.into(),
                world: "world".into(),
                position: BlockPosition { x, y: 65, z: 100 },
                game_mode: GameMode::Survival,
            }));
        }
        apply_sweeping_attack(
            &state,
            DimensionKind::Overworld,
            attacker,
            EntityPosition {
                x: 101.5,
                y: 65.0,
                z: 100.5,
            },
            Some(primary),
            None,
            0,
        );
        assert_eq!(state.vitals(primary).unwrap().health, 20.0);
        assert_eq!(state.vitals(bystander).unwrap().health, 19.0);
    }

    #[test]
    fn rescues_players_before_they_can_fall_forever() {
        assert!(!requires_spawn_rescue(-7.5, 65.0, -8.5));
        assert!(requires_spawn_rescue(-7.5, -33.0, -8.5));
        assert!(!requires_spawn_rescue(10_000.0, 65.0, -8.5));
    }

    #[test]
    fn blocks_drop_the_correct_item_and_amount() {
        assert_eq!(
            block_drop(BlockKind::Stone, Some(ItemKind::WoodenPickaxe)),
            Some(ItemStack {
                kind: ItemKind::Cobblestone,
                count: 1,
                damage: 0,
            })
        );
        assert_eq!(block_drop(BlockKind::Stone, None), None);
        assert_eq!(
            block_drop(BlockKind::OakLog, None).unwrap().kind,
            ItemKind::OakLog
        );
        assert_eq!(block_drop(BlockKind::OakLog, None).unwrap().count, 1);
        assert_eq!(
            block_drop(BlockKind::Gravel, None).unwrap().kind,
            ItemKind::Gravel
        );
        assert_eq!(
            block_drop(BlockKind::BirchLog, None).unwrap().kind,
            ItemKind::BirchLog
        );
        assert_eq!(
            block_drop(BlockKind::SpruceLog, None).unwrap().kind,
            ItemKind::SpruceLog
        );
    }

    #[test]
    fn gravel_drop_roll_is_stable_and_bounded() {
        let position = BlockPosition {
            x: 17,
            y: 32,
            z: -9,
        };
        let first = block_drop_at(BlockKind::Gravel, None, position, 42).unwrap();
        assert_eq!(
            block_drop_at(BlockKind::Gravel, None, position, 42).unwrap(),
            first
        );
        let flint = (0..1_000)
            .filter(|tick| {
                block_drop_at(BlockKind::Gravel, None, position, *tick)
                    .is_some_and(|stack| stack.kind == ItemKind::Flint)
            })
            .count();
        assert!(
            (50..=150).contains(&flint),
            "unexpected flint count: {flint}"
        );
    }

    #[test]
    fn tool_tiers_control_harvests_and_mining_speed() {
        assert!(
            mining_ticks(BlockKind::Stone, Some(ItemKind::WoodenPickaxe))
                < mining_ticks(BlockKind::Stone, None)
        );
        assert!(
            mining_ticks(BlockKind::OakLog, Some(ItemKind::WoodenAxe))
                < mining_ticks(BlockKind::OakLog, None)
        );
        assert!(
            mining_ticks(BlockKind::Dirt, Some(ItemKind::WoodenShovel))
                < mining_ticks(BlockKind::Dirt, None)
        );
        assert!(
            mining_ticks(BlockKind::Gravel, Some(ItemKind::WoodenShovel))
                < mining_ticks(BlockKind::Gravel, None)
        );
        assert!(
            mining_ticks(BlockKind::Stone, Some(ItemKind::IronPickaxe))
                < mining_ticks(BlockKind::Stone, Some(ItemKind::StonePickaxe))
        );
        assert!(
            mining_ticks(BlockKind::Stone, Some(ItemKind::StonePickaxe))
                < mining_ticks(BlockKind::Stone, Some(ItemKind::WoodenPickaxe))
        );
        assert_eq!(
            block_drop(BlockKind::DiamondOre, Some(ItemKind::WoodenPickaxe)),
            None
        );
        assert_eq!(
            block_drop(BlockKind::IronOre, Some(ItemKind::StonePickaxe))
                .unwrap()
                .kind,
            ItemKind::RawIron
        );
        assert_eq!(
            block_drop(BlockKind::DiamondOre, Some(ItemKind::IronPickaxe))
                .unwrap()
                .kind,
            ItemKind::Diamond
        );
        assert_eq!(
            block_drop(BlockKind::Obsidian, Some(ItemKind::IronPickaxe)),
            None
        );
        assert_eq!(
            block_drop(BlockKind::Obsidian, Some(ItemKind::DiamondPickaxe))
                .unwrap()
                .kind,
            ItemKind::Obsidian
        );
        assert!(
            mining_ticks(BlockKind::Obsidian, Some(ItemKind::DiamondPickaxe))
                < mining_ticks(BlockKind::Obsidian, Some(ItemKind::IronPickaxe))
        );
    }

    #[test]
    fn dimension_commands_map_to_the_three_builtin_worlds() {
        assert_eq!(
            command_dimension("dimension_overworld"),
            Some(DimensionKind::Overworld)
        );
        assert_eq!(
            command_dimension("dimension_nether"),
            Some(DimensionKind::Nether)
        );
        assert_eq!(command_dimension("dimension_end"), Some(DimensionKind::End));
        assert_eq!(DimensionKind::End.type_id(), 2);
        assert_eq!(DimensionKind::Nether.type_id(), 3);
    }
}
