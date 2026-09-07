use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use carbon_api::{
    BlockChange, BlockKind, BlockPlacement, BlockPosition, ChatMessage, ChestSnapshot,
    ChunkPosition, ChunkSnapshot, DimensionKind, EntityPosition, FurnaceSlot, FurnaceSnapshot,
    InventoryCursor, ItemEntitySnapshot, ItemKind, ItemStack, MobKind, MobSnapshot,
    ModerationRecord, PlayerBan, PlayerCombatState, PlayerDisconnect, PlayerEquipment, PlayerEvent,
    PlayerEventKind, PlayerImpulse, PlayerInventory, PlayerInventorySlot, PlayerSnapshot,
    PlayerTransform, PlayerVitals, ServerApi, StatusEffectKind, StatusEffectSnapshot,
    TerrainProfile, WorldSnapshot,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tracing::info;
use uuid::Uuid;

const CURRENT_SAVE_SCHEMA_VERSION: u32 = 2;
const CURRENT_GENERATOR_VERSION: u32 = 1;
const OLDEST_SUPPORTED_SAVE_SCHEMA_VERSION: u32 = 1;

use crate::world::PrototypeWorld;
use crate::{NAME, VERSION};

pub struct ServerState {
    tick: AtomicU64,
    players: RwLock<Vec<PlayerSnapshot>>,
    player_transforms: RwLock<HashMap<Uuid, PlayerTransform>>,
    player_event_revision: AtomicU64,
    player_events: RwLock<VecDeque<PlayerEvent>>,
    disconnect_revision: AtomicU64,
    disconnects: RwLock<VecDeque<PlayerDisconnect>>,
    inventories: RwLock<HashMap<Uuid, PlayerInventory>>,
    equipment: RwLock<HashMap<Uuid, PlayerEquipment>>,
    combat_states: RwLock<HashMap<Uuid, PlayerCombatState>>,
    player_impulse_revision: AtomicU64,
    player_impulses: RwLock<VecDeque<PlayerImpulse>>,
    chat_revision: AtomicU64,
    chat_messages: RwLock<VecDeque<ChatMessage>>,
    vitals: RwLock<HashMap<Uuid, PlayerVitals>>,
    effect_revision: AtomicU64,
    status_effects: RwLock<HashMap<Uuid, HashMap<StatusEffectKind, StatusEffectSnapshot>>>,
    lava_burn_until: RwLock<HashMap<Uuid, u64>>,
    saved_locations: RwLock<HashMap<Uuid, SavedLocation>>,
    worlds: RwLock<Vec<WorldSnapshot>>,
    simulation: RwLock<PrototypeWorld>,
    dimensions: RwLock<HashMap<DimensionKind, PrototypeWorld>>,
    furnaces: RwLock<HashMap<(DimensionKind, BlockPosition), FurnaceSnapshot>>,
    chests: RwLock<HashMap<(DimensionKind, BlockPosition), ChestSnapshot>>,
    block_revision: AtomicU64,
    block_changes: RwLock<VecDeque<BlockChange>>,
    operators: RwLock<HashMap<String, String>>,
    operator_path: Option<PathBuf>,
    banned_players: RwLock<HashMap<String, BanEntry>>,
    banned_path: Option<PathBuf>,
    allowlisted_players: RwLock<HashMap<String, String>>,
    allowlist_path: Option<PathBuf>,
    moderation_records: RwLock<VecDeque<ModerationRecord>>,
    moderation_path: Option<PathBuf>,
    permissions: RwLock<HashMap<String, BTreeSet<String>>>,
    permissions_path: Option<PathBuf>,
    save_path: Option<PathBuf>,
    shutdown: watch::Sender<bool>,
}

#[derive(Default)]
struct AccessLists {
    banned_players: HashMap<String, BanEntry>,
    banned_path: Option<PathBuf>,
    allowlisted_players: HashMap<String, String>,
    allowlist_path: Option<PathBuf>,
    moderation_records: VecDeque<ModerationRecord>,
    moderation_path: Option<PathBuf>,
    permissions: HashMap<String, BTreeSet<String>>,
    permissions_path: Option<PathBuf>,
}

#[derive(Clone, Serialize, Deserialize)]
struct BanEntry {
    name: String,
    reason: String,
    #[serde(default)]
    expires_at_unix: Option<u64>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SavedBanEntry {
    LegacyName(String),
    Record(BanEntry),
}

#[derive(Serialize, Deserialize)]
struct StoredModerationRecord {
    timestamp_unix: u64,
    actor: String,
    action: String,
    target: Option<String>,
    detail: String,
}

#[derive(Serialize, Deserialize)]
struct SaveData {
    version: u32,
    #[serde(default)]
    generator_version: u32,
    blocks: Vec<SavedBlock>,
    inventories: Vec<SavedInventory>,
    #[serde(default)]
    furnaces: Vec<SavedFurnace>,
    #[serde(default)]
    chests: Vec<SavedChest>,
}

#[derive(Serialize, Deserialize)]
struct SavedChest {
    dimension: String,
    x: i32,
    y: i32,
    z: i32,
    slots: Vec<Option<SavedStack>>,
    #[serde(default)]
    sharpness_levels: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
struct SavedFurnace {
    dimension: String,
    x: i32,
    y: i32,
    z: i32,
    input: Option<SavedStack>,
    fuel: Option<SavedStack>,
    output: Option<SavedStack>,
    burn_remaining: u16,
    burn_total: u16,
    cook_progress: u16,
}

#[derive(Serialize, Deserialize)]
struct SavedBlock {
    #[serde(default)]
    dimension: String,
    x: i32,
    y: i32,
    z: i32,
    kind: String,
}

#[derive(Serialize, Deserialize)]
struct SavedInventory {
    player_id: String,
    slots: Vec<Option<SavedStack>>,
    health: f32,
    food: u8,
    saturation: f32,
    #[serde(default)]
    location: Option<SavedLocation>,
    #[serde(default)]
    armor: Vec<Option<SavedStack>>,
    #[serde(default)]
    selected_slot: u8,
    #[serde(default)]
    sharpness_levels: Vec<u8>,
    #[serde(default)]
    off_hand: Option<SavedStack>,
    #[serde(default)]
    effects: Vec<SavedStatusEffect>,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedStatusEffect {
    kind: String,
    amplifier: u8,
    remaining_ticks: u32,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedLocation {
    dimension: String,
    x: i32,
    y: i32,
    z: i32,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedStack {
    kind: String,
    count: u8,
    damage: u16,
}

impl ServerState {
    fn collapse_nether_portal_near(
        &self,
        dimension: DimensionKind,
        broken_frame: BlockPosition,
    ) -> usize {
        let mut pending = VecDeque::new();
        for y in broken_frame.y.saturating_sub(4)..=broken_frame.y.saturating_add(4) {
            for z in broken_frame.z.saturating_sub(3)..=broken_frame.z.saturating_add(3) {
                for x in broken_frame.x.saturating_sub(3)..=broken_frame.x.saturating_add(3) {
                    let position = BlockPosition { x, y, z };
                    if self.dimension_block_at(dimension, position) == BlockKind::NetherPortal {
                        pending.push_back(position);
                    }
                }
            }
        }
        let mut connected = HashSet::new();
        while let Some(position) = pending.pop_front() {
            if !connected.insert(position) {
                continue;
            }
            for neighbor in [
                BlockPosition {
                    x: position.x - 1,
                    ..position
                },
                BlockPosition {
                    x: position.x + 1,
                    ..position
                },
                BlockPosition {
                    y: position.y - 1,
                    ..position
                },
                BlockPosition {
                    y: position.y + 1,
                    ..position
                },
                BlockPosition {
                    z: position.z - 1,
                    ..position
                },
                BlockPosition {
                    z: position.z + 1,
                    ..position
                },
            ] {
                if !connected.contains(&neighbor)
                    && self.dimension_block_at(dimension, neighbor) == BlockKind::NetherPortal
                {
                    pending.push_back(neighbor);
                }
            }
        }
        for position in &connected {
            self.set_dimension_block(dimension, *position, BlockKind::Air);
        }
        connected.len()
    }

    fn activate_nether_portal_near(
        &self,
        dimension: DimensionKind,
        ignition: BlockPosition,
    ) -> bool {
        if dimension == DimensionKind::End {
            return false;
        }
        for along_offset in 0..=3 {
            for y_offset in 0..=4 {
                for along_x in [true, false] {
                    let anchor = BlockPosition {
                        x: if along_x {
                            ignition.x - along_offset
                        } else {
                            ignition.x
                        },
                        y: ignition.y - y_offset,
                        z: if along_x {
                            ignition.z
                        } else {
                            ignition.z - along_offset
                        },
                    };
                    let at = |along: i32, y: i32| BlockPosition {
                        x: anchor.x + if along_x { along } else { 0 },
                        y: anchor.y + y,
                        z: anchor.z + if along_x { 0 } else { along },
                    };
                    let frame_complete = (0..=4).all(|y| {
                        (0..=3).all(|along| {
                            let boundary = along == 0 || along == 3 || y == 0 || y == 4;
                            let kind = self.dimension_block_at(dimension, at(along, y));
                            if boundary {
                                kind == BlockKind::Obsidian
                            } else {
                                kind == BlockKind::Air
                            }
                        })
                    });
                    if frame_complete {
                        for y in 1..=3 {
                            for along in 1..=2 {
                                self.set_dimension_block(
                                    dimension,
                                    at(along, y),
                                    BlockKind::NetherPortal,
                                );
                            }
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn repair_world(&self) -> anyhow::Result<()> {
        let (world_name, seed) = self
            .worlds
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .first()
            .map(|world| (world.name.clone(), world.seed))
            .unwrap_or_else(|| ("world".into(), 0));
        let online = self
            .players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|player| player.id)
            .collect::<Vec<_>>();
        for id in &online {
            self.request_disconnect(*id, "The world was reset by the server console.");
        }

        *self
            .simulation
            .write()
            .unwrap_or_else(|error| error.into_inner()) = PrototypeWorld::new(seed);
        *self
            .dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner()) = HashMap::from([
            (
                DimensionKind::Nether,
                PrototypeWorld::new_dimension(seed ^ 0x4e37_4e52, DimensionKind::Nether),
            ),
            (
                DimensionKind::End,
                PrototypeWorld::new_dimension(seed ^ 0xe11d, DimensionKind::End),
            ),
        ]);
        *self
            .worlds
            .write()
            .unwrap_or_else(|error| error.into_inner()) = vec![
            WorldSnapshot {
                name: world_name.clone(),
                seed,
                age_ticks: 0,
                player_count: online.len(),
            },
            WorldSnapshot {
                name: DimensionKind::Nether.name().into(),
                seed: seed ^ 0x4e37_4e52,
                age_ticks: 0,
                player_count: 0,
            },
            WorldSnapshot {
                name: DimensionKind::End.name().into(),
                seed: seed ^ 0xe11d,
                age_ticks: 0,
                player_count: 0,
            },
        ];

        self.furnaces
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.chests
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.block_changes
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.saved_locations
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.status_effects
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.lava_burn_until
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.player_events
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.player_impulses
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .clear();

        let spawn = self.safe_surface_position(DimensionKind::Overworld, -8, -9);
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        for player in &mut *players {
            player.world = world_name.clone();
            player.position = spawn;
        }
        drop(players);
        let mut transforms = self
            .player_transforms
            .write()
            .unwrap_or_else(|error| error.into_inner());
        for id in &online {
            transforms.insert(
                *id,
                PlayerTransform {
                    id: *id,
                    position: EntityPosition {
                        x: f64::from(spawn.x) + 0.5,
                        y: f64::from(spawn.y),
                        z: f64::from(spawn.z) + 0.5,
                    },
                    yaw: 0.0,
                    pitch: 0.0,
                    on_ground: true,
                    revision: 0,
                },
            );
        }
        drop(transforms);
        let ids = online.clone();
        *self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner()) = ids
            .iter()
            .map(|id| (*id, PlayerInventory::default()))
            .collect();
        *self
            .equipment
            .write()
            .unwrap_or_else(|error| error.into_inner()) = ids
            .iter()
            .map(|id| (*id, PlayerEquipment::default()))
            .collect();
        *self
            .vitals
            .write()
            .unwrap_or_else(|error| error.into_inner()) = ids
            .iter()
            .map(|id| (*id, PlayerVitals::default()))
            .collect();
        *self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner()) = ids
            .iter()
            .map(|id| (*id, PlayerCombatState::default()))
            .collect();

        self.block_revision.store(0, Ordering::Relaxed);
        self.save()?;
        if let Some(path) = &self.save_path {
            let backup = path.with_extension("json.bak");
            if backup.exists() {
                fs::remove_file(backup)?;
            }
        }
        Ok(())
    }

    fn safe_surface_position(
        &self,
        dimension: DimensionKind,
        preferred_x: i32,
        preferred_z: i32,
    ) -> BlockPosition {
        let fallback = match dimension {
            DimensionKind::End => (-8, -8),
            _ => (-8, -9),
        };
        let candidates = std::iter::once((preferred_x, preferred_z))
            .chain((1_i32..=8).flat_map(|radius| {
                (-radius..=radius).flat_map(move |dz| {
                    (-radius..=radius)
                        .filter(move |dx| (*dx).abs() == radius || dz.abs() == radius)
                        .map(move |dx| (preferred_x + dx, preferred_z + dz))
                })
            }))
            .chain(std::iter::once(fallback));
        for (x, z) in candidates {
            let surface = self.ensure_dimension_chunk_surface(
                dimension,
                ChunkPosition {
                    x: x.div_euclid(16),
                    z: z.div_euclid(16),
                },
            );
            let index = usize::try_from(z.rem_euclid(16) * 16 + x.rem_euclid(16)).unwrap_or(0);
            let position = BlockPosition {
                x,
                y: i32::from(surface[index]) + 1,
                z,
            };
            let head = BlockPosition {
                y: position.y + 1,
                ..position
            };
            let support = BlockPosition {
                y: position.y - 1,
                ..position
            };
            if self.dimension_block_at(dimension, position) == BlockKind::Air
                && self.dimension_block_at(dimension, head) == BlockKind::Air
                && !matches!(
                    self.dimension_block_at(dimension, support),
                    BlockKind::Air
                        | BlockKind::Water
                        | BlockKind::Lava
                        | BlockKind::NetherPortal
                        | BlockKind::EndPortal
                )
            {
                return position;
            }
        }
        BlockPosition {
            x: fallback.0,
            y: 65,
            z: fallback.1,
        }
    }

    #[cfg(test)]
    pub fn new(world_name: String, seed: i64, shutdown: watch::Sender<bool>) -> Self {
        Self::from_operators(
            world_name,
            seed,
            shutdown,
            HashMap::new(),
            None,
            AccessLists::default(),
        )
    }

    pub fn with_operator_file(
        world_name: String,
        seed: i64,
        shutdown: watch::Sender<bool>,
        path: impl Into<PathBuf>,
    ) -> anyhow::Result<Self> {
        let path = path.into();
        let names: Vec<String> = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let operators = names
            .into_iter()
            .map(|name| (name.to_ascii_lowercase(), name))
            .collect();
        let directory = path.parent().unwrap_or_else(|| Path::new("."));
        let banned_path = directory.join("banned-players.json");
        let allowlist_path = directory.join("allowlist.json");
        let moderation_path = directory.join("moderation-audit.jsonl");
        let permissions_path = directory.join("permissions.json");
        let access = AccessLists {
            banned_players: load_bans(&banned_path)?,
            banned_path: Some(banned_path),
            allowlisted_players: load_name_map(&allowlist_path)?,
            allowlist_path: Some(allowlist_path),
            moderation_records: load_moderation_records(&moderation_path)?,
            moderation_path: Some(moderation_path),
            permissions: load_permissions(&permissions_path)?,
            permissions_path: Some(permissions_path),
        };
        let save_path = directory.join("world-save.json");
        let mut state =
            Self::from_operators(world_name, seed, shutdown, operators, Some(path), access);
        state.save_path = Some(save_path);
        state.load_save()?;
        Ok(state)
    }

    fn from_operators(
        world_name: String,
        seed: i64,
        shutdown: watch::Sender<bool>,
        operators: HashMap<String, String>,
        operator_path: Option<PathBuf>,
        access: AccessLists,
    ) -> Self {
        Self {
            tick: AtomicU64::new(0),
            players: RwLock::new(Vec::new()),
            player_transforms: RwLock::new(HashMap::new()),
            player_event_revision: AtomicU64::new(0),
            player_events: RwLock::new(VecDeque::new()),
            disconnect_revision: AtomicU64::new(0),
            disconnects: RwLock::new(VecDeque::new()),
            inventories: RwLock::new(HashMap::new()),
            equipment: RwLock::new(HashMap::new()),
            combat_states: RwLock::new(HashMap::new()),
            player_impulse_revision: AtomicU64::new(0),
            player_impulses: RwLock::new(VecDeque::new()),
            chat_revision: AtomicU64::new(0),
            chat_messages: RwLock::new(VecDeque::new()),
            vitals: RwLock::new(HashMap::new()),
            effect_revision: AtomicU64::new(0),
            status_effects: RwLock::new(HashMap::new()),
            lava_burn_until: RwLock::new(HashMap::new()),
            saved_locations: RwLock::new(HashMap::new()),
            worlds: RwLock::new(vec![
                WorldSnapshot {
                    name: world_name,
                    seed,
                    age_ticks: 0,
                    player_count: 0,
                },
                WorldSnapshot {
                    name: DimensionKind::Nether.name().into(),
                    seed: seed ^ 0x4e37_4e52,
                    age_ticks: 0,
                    player_count: 0,
                },
                WorldSnapshot {
                    name: DimensionKind::End.name().into(),
                    seed: seed ^ 0xe11d,
                    age_ticks: 0,
                    player_count: 0,
                },
            ]),
            simulation: RwLock::new(PrototypeWorld::new(seed)),
            dimensions: RwLock::new(HashMap::from([
                (
                    DimensionKind::Nether,
                    PrototypeWorld::new_dimension(seed ^ 0x4e37_4e52, DimensionKind::Nether),
                ),
                (
                    DimensionKind::End,
                    PrototypeWorld::new_dimension(seed ^ 0xe11d, DimensionKind::End),
                ),
            ])),
            furnaces: RwLock::new(HashMap::new()),
            chests: RwLock::new(HashMap::new()),
            block_revision: AtomicU64::new(0),
            block_changes: RwLock::new(VecDeque::new()),
            operators: RwLock::new(operators),
            operator_path,
            banned_players: RwLock::new(access.banned_players),
            banned_path: access.banned_path,
            allowlisted_players: RwLock::new(access.allowlisted_players),
            allowlist_path: access.allowlist_path,
            moderation_records: RwLock::new(access.moderation_records),
            moderation_path: access.moderation_path,
            permissions: RwLock::new(access.permissions),
            permissions_path: access.permissions_path,
            save_path: None,
            shutdown,
        }
    }

    fn record_chat(&self, sender: Option<String>, text: String) {
        let revision = self.chat_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let mut messages = self
            .chat_messages
            .write()
            .unwrap_or_else(|error| error.into_inner());
        messages.push_back(ChatMessage {
            revision,
            sender,
            text,
        });
        while messages.len() > 1_024 {
            messages.pop_front();
        }
    }

    pub fn advance_tick(&self) -> u64 {
        let tick = self.tick.fetch_add(1, Ordering::Relaxed) + 1;
        self.tick_furnaces();
        self.tick_lava_hazards(tick);
        let mut worlds = self
            .worlds
            .write()
            .unwrap_or_else(|error| error.into_inner());
        for world in &mut *worlds {
            world.age_ticks = world.age_ticks.saturating_add(1);
        }
        drop(worlds);
        let players = self
            .players
            .read()
            .unwrap_or_else(|error| error.into_inner());
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .tick(tick, &players);
        let pickups = self
            .simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .collect_pickups(&players);
        for (player_id, stack) in pickups {
            if !self.give_item(player_id, stack.kind, stack.count) {
                if let Some(player) = players.iter().find(|player| player.id == player_id) {
                    self.drop_item(stack, player.position);
                }
            }
        }
        let mut dimension_pickups = Vec::new();
        {
            let mut dimensions = self
                .dimensions
                .write()
                .unwrap_or_else(|error| error.into_inner());
            for (dimension, world) in dimensions.iter_mut() {
                let local_players: Vec<_> = players
                    .iter()
                    .filter(|player| player.world == dimension.name())
                    .cloned()
                    .collect();
                world.tick(tick, &local_players);
                dimension_pickups.extend(
                    world
                        .collect_pickups(&local_players)
                        .into_iter()
                        .map(|pickup| (*dimension, pickup)),
                );
            }
        }
        for (dimension, (player_id, stack)) in dimension_pickups {
            if !self.give_item(player_id, stack.kind, stack.count) {
                if let Some(player) = players.iter().find(|player| player.id == player_id) {
                    self.drop_dimension_item(dimension, stack, player.position);
                }
            }
        }
        let mobs = self
            .simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .mobs();
        if tick % 20 == 0 {
            for player in players.iter() {
                for mob in &mobs {
                    if mob.kind != MobKind::Zombie {
                        continue;
                    }
                    let dx = mob.position.x - (f64::from(player.position.x) + 0.5);
                    let dy = mob.position.y - f64::from(player.position.y);
                    let dz = mob.position.z - (f64::from(player.position.z) + 0.5);
                    if dx * dx + dz * dz <= 4.0 && dy.abs() <= 2.0 {
                        self.damage_player_combat(player.id, 2.0);
                    }
                }
            }
        }
        if tick % 80 == 0 {
            let mut vitals = self
                .vitals
                .write()
                .unwrap_or_else(|error| error.into_inner());
            for value in vitals.values_mut() {
                if value.health > 0.0 && value.food >= 18 && value.health < 20.0 {
                    value.health = (value.health + 1.0).min(20.0);
                    value.saturation = (value.saturation - 1.0).max(0.0);
                    value.revision = value.revision.saturating_add(1);
                } else if value.health > 1.0 && value.food == 0 {
                    value.health = (value.health - 1.0).max(1.0);
                    value.revision = value.revision.saturating_add(1);
                }
            }
        }
        self.tick_status_effects(tick);
        if tick % 1_600 == 0 {
            let mut vitals = self
                .vitals
                .write()
                .unwrap_or_else(|error| error.into_inner());
            for value in vitals.values_mut().filter(|value| value.food > 0) {
                value.food -= 1;
                value.revision = value.revision.saturating_add(1);
            }
        }
        if tick % 200 == 0 {
            if let Err(error) = self.save() {
                tracing::warn!(%error, "could not persist world save");
            }
        }
        tick
    }

    fn tick_lava_hazards(&self, tick: u64) {
        let exposed: Vec<_> = self
            .players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|player| player.world == DimensionKind::Nether.name())
            .filter(|player| {
                [
                    player.position,
                    BlockPosition {
                        y: player.position.y + 1,
                        ..player.position
                    },
                ]
                .into_iter()
                .any(|position| {
                    self.dimension_block_at(DimensionKind::Nether, position) == BlockKind::Lava
                })
            })
            .map(|player| player.id)
            .collect();
        {
            let mut burning = self
                .lava_burn_until
                .write()
                .unwrap_or_else(|error| error.into_inner());
            for id in &exposed {
                burning.insert(*id, tick.saturating_add(80));
            }
            burning.retain(|_, until| *until >= tick);
        }
        if tick % 10 == 0 {
            let burning = self
                .lava_burn_until
                .read()
                .unwrap_or_else(|error| error.into_inner());
            for id in burning.keys() {
                self.damage_player(*id, if exposed.contains(id) { 2.0 } else { 1.0 });
            }
        }
    }

    fn tick_status_effects(&self, tick: u64) {
        let mut periodic = Vec::new();
        {
            let mut all_effects = self
                .status_effects
                .write()
                .unwrap_or_else(|error| error.into_inner());
            for (id, effects) in &mut *all_effects {
                for effect in effects.values_mut() {
                    let interval = match effect.kind {
                        StatusEffectKind::Regeneration => Some(
                            50_u64
                                .checked_shr(u32::from(effect.amplifier))
                                .unwrap_or(0)
                                .max(1),
                        ),
                        StatusEffectKind::Poison => Some(
                            25_u64
                                .checked_shr(u32::from(effect.amplifier))
                                .unwrap_or(0)
                                .max(1),
                        ),
                        StatusEffectKind::Hunger => Some(
                            80_u64
                                .checked_shr(u32::from(effect.amplifier))
                                .unwrap_or(0)
                                .max(1),
                        ),
                        _ => None,
                    };
                    if interval.is_some_and(|interval| tick % interval == 0) {
                        periodic.push((*id, effect.kind));
                    }
                    effect.remaining_ticks = effect.remaining_ticks.saturating_sub(1);
                }
                effects.retain(|_, effect| effect.remaining_ticks > 0);
            }
            all_effects.retain(|_, effects| !effects.is_empty());
        }

        let mut hurt = Vec::new();
        let mut vitals = self
            .vitals
            .write()
            .unwrap_or_else(|error| error.into_inner());
        for (id, kind) in periodic {
            let Some(value) = vitals.get_mut(&id).filter(|value| value.health > 0.0) else {
                continue;
            };
            let changed = match kind {
                StatusEffectKind::Regeneration if value.health < 20.0 => {
                    value.health = (value.health + 1.0).min(20.0);
                    true
                }
                StatusEffectKind::Poison if value.health > 1.0 => {
                    value.health = (value.health - 1.0).max(1.0);
                    hurt.push(id);
                    true
                }
                StatusEffectKind::Hunger if value.food > 0 => {
                    value.food -= 1;
                    value.saturation = (value.saturation - 0.5).max(0.0);
                    true
                }
                _ => false,
            };
            if changed {
                value.revision = value.revision.saturating_add(1);
            }
        }
        drop(vitals);
        for id in hurt {
            self.record_player_event(id, PlayerEventKind::Hurt);
        }
    }

    fn tick_furnaces(&self) {
        let mut furnaces = self
            .furnaces
            .write()
            .unwrap_or_else(|error| error.into_inner());
        for furnace in furnaces.values_mut() {
            let recipe = furnace.input.and_then(|stack| smelting_result(stack.kind));
            let can_smelt = recipe.is_some_and(|kind| can_accept(furnace.output, kind));
            if furnace.burn_remaining == 0 && can_smelt {
                let burn_ticks = furnace.fuel.map_or(0, |stack| fuel_burn_ticks(stack.kind));
                if burn_ticks > 0 {
                    take_one(&mut furnace.fuel);
                    furnace.burn_remaining = burn_ticks;
                    furnace.burn_total = burn_ticks;
                    furnace.revision = furnace.revision.wrapping_add(1);
                }
            }
            let burning = furnace.burn_remaining > 0;
            if burning {
                furnace.burn_remaining -= 1;
                furnace.revision = furnace.revision.wrapping_add(1);
            }
            if burning && can_smelt {
                furnace.cook_progress = furnace.cook_progress.saturating_add(1);
                if furnace.cook_progress >= furnace.cook_total.max(1) {
                    take_one(&mut furnace.input);
                    let output_kind = recipe.expect("smelting recipe was checked");
                    match &mut furnace.output {
                        Some(output) => output.count = output.count.saturating_add(1),
                        slot @ None => {
                            *slot = Some(ItemStack {
                                kind: output_kind,
                                count: 1,
                                damage: 0,
                            })
                        }
                    }
                    furnace.cook_progress = 0;
                }
                furnace.revision = furnace.revision.wrapping_add(1);
            } else if !can_smelt && furnace.cook_progress != 0 {
                furnace.cook_progress = 0;
                furnace.revision = furnace.revision.wrapping_add(1);
            }
        }
    }

    fn record_player_event(&self, player_id: Uuid, kind: PlayerEventKind) {
        let revision = self.player_event_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let mut events = self
            .player_events
            .write()
            .unwrap_or_else(|error| error.into_inner());
        events.push_back(PlayerEvent {
            revision,
            player_id,
            kind,
        });
        while events.len() > 1_024 {
            events.pop_front();
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = &self.save_path else {
            return Ok(());
        };
        let mut blocks: Vec<_> = self
            .simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .edits()
            .into_iter()
            .map(|block| SavedBlock {
                dimension: DimensionKind::Overworld.name().into(),
                x: block.position.x,
                y: block.position.y,
                z: block.position.z,
                kind: block_kind_name(block.kind).to_owned(),
            })
            .collect();
        let dimensions = self
            .dimensions
            .read()
            .unwrap_or_else(|error| error.into_inner());
        for (dimension, world) in dimensions.iter() {
            blocks.extend(world.edits().into_iter().map(|block| SavedBlock {
                dimension: dimension.name().into(),
                x: block.position.x,
                y: block.position.y,
                z: block.position.z,
                kind: block_kind_name(block.kind).to_owned(),
            }));
        }
        drop(dimensions);
        let active_locations: HashMap<_, _> = self
            .players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|player| {
                (
                    player.id,
                    SavedLocation {
                        dimension: player.world.clone(),
                        x: player.position.x,
                        y: player.position.y,
                        z: player.position.z,
                    },
                )
            })
            .collect();
        let saved_locations = self
            .saved_locations
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let vitals = self
            .vitals
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let equipment = self
            .equipment
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let status_effects = self
            .status_effects
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let inventories = self
            .inventories
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|(id, inventory)| {
                let player_vitals = vitals.get(id).copied().unwrap_or_default();
                SavedInventory {
                    player_id: id.to_string(),
                    slots: inventory
                        .slots
                        .iter()
                        .map(|slot| {
                            slot.map(|stack| SavedStack {
                                kind: stack.kind.as_str().to_owned(),
                                count: stack.count,
                                damage: stack.damage,
                            })
                        })
                        .collect(),
                    health: player_vitals.health,
                    food: player_vitals.food,
                    saturation: player_vitals.saturation,
                    location: active_locations
                        .get(id)
                        .or_else(|| saved_locations.get(id))
                        .cloned(),
                    armor: equipment
                        .get(id)
                        .map_or([None; 4], |equipment| equipment.armor)
                        .into_iter()
                        .map(|slot| {
                            slot.map(|stack| SavedStack {
                                kind: stack.kind.as_str().to_owned(),
                                count: stack.count,
                                damage: stack.damage,
                            })
                        })
                        .collect(),
                    selected_slot: equipment
                        .get(id)
                        .map_or(0, |equipment| equipment.selected_slot),
                    sharpness_levels: inventory.sharpness_levels.to_vec(),
                    off_hand: equipment.get(id).and_then(|equipment| {
                        equipment.off_hand.map(|stack| SavedStack {
                            kind: stack.kind.as_str().to_owned(),
                            count: stack.count,
                            damage: stack.damage,
                        })
                    }),
                    effects: status_effects
                        .get(id)
                        .into_iter()
                        .flat_map(HashMap::values)
                        .map(|effect| SavedStatusEffect {
                            kind: effect.kind.as_str().to_owned(),
                            amplifier: effect.amplifier,
                            remaining_ticks: effect.remaining_ticks,
                        })
                        .collect(),
                }
            })
            .collect();
        let furnaces = self
            .furnaces
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|((dimension, position), furnace)| SavedFurnace {
                dimension: dimension.name().into(),
                x: position.x,
                y: position.y,
                z: position.z,
                input: furnace.input.map(save_stack),
                fuel: furnace.fuel.map(save_stack),
                output: furnace.output.map(save_stack),
                burn_remaining: furnace.burn_remaining,
                burn_total: furnace.burn_total,
                cook_progress: furnace.cook_progress,
            })
            .collect();
        let chests = self
            .chests
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|((dimension, position), chest)| SavedChest {
                dimension: dimension.name().into(),
                x: position.x,
                y: position.y,
                z: position.z,
                slots: chest
                    .slots
                    .iter()
                    .map(|slot| slot.map(save_stack))
                    .collect(),
                sharpness_levels: chest.sharpness_levels.to_vec(),
            })
            .collect();
        let data = SaveData {
            version: CURRENT_SAVE_SCHEMA_VERSION,
            generator_version: CURRENT_GENERATOR_VERSION,
            blocks,
            inventories,
            furnaces,
            chests,
        };
        write_world_save(path, &serde_json::to_vec_pretty(&data)?)?;
        Ok(())
    }

    fn load_save(&mut self) -> anyhow::Result<()> {
        let Some(path) = &self.save_path else {
            return Ok(());
        };
        let Some(data) = load_save_file(path)? else {
            return Ok(());
        };
        let world = self
            .simulation
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        for block in data.blocks {
            let Some(kind) = parse_block_kind(&block.kind) else {
                continue;
            };
            let placement = BlockPlacement {
                position: BlockPosition {
                    x: block.x,
                    y: block.y,
                    z: block.z,
                },
                kind,
            };
            match parse_dimension(&block.dimension) {
                DimensionKind::Overworld => world.apply_edit(placement),
                dimension => {
                    if let Some(target) = self
                        .dimensions
                        .get_mut()
                        .unwrap_or_else(|error| error.into_inner())
                        .get_mut(&dimension)
                    {
                        target.apply_edit(placement);
                    }
                }
            }
        }
        for saved in data.inventories {
            let Ok(id) = Uuid::parse_str(&saved.player_id) else {
                continue;
            };
            let mut inventory = PlayerInventory::default();
            for (index, slot) in saved.slots.into_iter().take(36).enumerate() {
                inventory.slots[index] = slot.and_then(|stack| {
                    Some(ItemStack {
                        kind: parse_item_kind(&stack.kind)?,
                        count: stack.count,
                        damage: stack.damage,
                    })
                });
            }
            for (index, level) in saved.sharpness_levels.into_iter().take(36).enumerate() {
                inventory.sharpness_levels[index] = level.min(5);
            }
            self.inventories
                .get_mut()
                .unwrap_or_else(|error| error.into_inner())
                .insert(id, inventory);
            let mut equipment = PlayerEquipment {
                selected_slot: saved.selected_slot.min(8),
                off_hand: saved.off_hand.and_then(|stack| {
                    Some(ItemStack {
                        kind: parse_item_kind(&stack.kind)?,
                        count: stack.count,
                        damage: stack.damage,
                    })
                }),
                ..PlayerEquipment::default()
            };
            for (index, slot) in saved.armor.into_iter().take(4).enumerate() {
                equipment.armor[index] = slot.and_then(|stack| {
                    Some(ItemStack {
                        kind: parse_item_kind(&stack.kind)?,
                        count: stack.count,
                        damage: stack.damage,
                    })
                });
            }
            self.equipment
                .get_mut()
                .unwrap_or_else(|error| error.into_inner())
                .insert(id, equipment);
            self.vitals
                .get_mut()
                .unwrap_or_else(|error| error.into_inner())
                .insert(
                    id,
                    PlayerVitals {
                        health: saved.health,
                        food: saved.food,
                        saturation: saved.saturation,
                        revision: 0,
                    },
                );
            let loaded_effects = saved
                .effects
                .into_iter()
                .filter_map(|effect| {
                    let kind = parse_status_effect_kind(&effect.kind)?;
                    let revision = self.effect_revision.fetch_add(1, Ordering::Relaxed) + 1;
                    Some((
                        kind,
                        StatusEffectSnapshot {
                            kind,
                            amplifier: effect.amplifier.min(4),
                            remaining_ticks: effect.remaining_ticks,
                            revision,
                        },
                    ))
                })
                .filter(|(_, effect)| effect.remaining_ticks > 0)
                .collect::<HashMap<_, _>>();
            if !loaded_effects.is_empty() {
                self.status_effects
                    .get_mut()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(id, loaded_effects);
            }
            if let Some(location) = saved.location {
                self.saved_locations
                    .get_mut()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(id, location);
            }
        }
        for saved in data.furnaces {
            let dimension = parse_dimension(&saved.dimension);
            let position = BlockPosition {
                x: saved.x,
                y: saved.y,
                z: saved.z,
            };
            self.ensure_dimension_chunk_surface(
                dimension,
                ChunkPosition {
                    x: position.x.div_euclid(16),
                    z: position.z.div_euclid(16),
                },
            );
            if self.dimension_block_at(dimension, position) != BlockKind::Furnace {
                continue;
            }
            self.furnaces
                .get_mut()
                .unwrap_or_else(|error| error.into_inner())
                .insert(
                    (dimension, position),
                    FurnaceSnapshot {
                        input: saved.input.and_then(load_stack),
                        fuel: saved.fuel.and_then(load_stack),
                        output: saved.output.and_then(load_stack),
                        burn_remaining: saved.burn_remaining,
                        burn_total: saved.burn_total,
                        cook_progress: saved.cook_progress,
                        cook_total: 200,
                        revision: 0,
                    },
                );
        }
        for saved in data.chests {
            let dimension = parse_dimension(&saved.dimension);
            let position = BlockPosition {
                x: saved.x,
                y: saved.y,
                z: saved.z,
            };
            self.ensure_dimension_chunk_surface(
                dimension,
                ChunkPosition {
                    x: position.x.div_euclid(16),
                    z: position.z.div_euclid(16),
                },
            );
            if self.dimension_block_at(dimension, position) != BlockKind::Chest {
                continue;
            }
            let mut chest = ChestSnapshot::default();
            for (index, slot) in saved.slots.into_iter().take(27).enumerate() {
                chest.slots[index] = slot.and_then(load_stack);
            }
            for (index, level) in saved.sharpness_levels.into_iter().take(27).enumerate() {
                chest.sharpness_levels[index] = level.min(5);
            }
            self.chests
                .get_mut()
                .unwrap_or_else(|error| error.into_inner())
                .insert((dimension, position), chest);
        }
        Ok(())
    }

    fn generated_chest_loot(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
    ) -> Option<ChestSnapshot> {
        if dimension == DimensionKind::Overworld {
            return self
                .simulation
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .generated_chest_loot(position);
        }
        self.dimensions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&dimension)
            .expect("built-in dimension exists")
            .generated_chest_loot(position)
    }
}

impl ServerApi for ServerState {
    fn name(&self) -> &str {
        NAME
    }
    fn version(&self) -> &str {
        VERSION
    }
    fn current_tick(&self) -> u64 {
        self.tick.load(Ordering::Relaxed)
    }

    fn players(&self) -> Vec<PlayerSnapshot> {
        self.players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn add_player(&self, mut player: PlayerSnapshot) -> bool {
        let mut restored_location = false;
        if let Some(location) = self
            .saved_locations
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&player.id)
        {
            restored_location = true;
            player.world = location.dimension.clone();
            player.position = BlockPosition {
                x: location.x,
                y: location.y,
                z: location.z,
            };
        }
        let dimension = parse_dimension(&player.world);
        let feet = self.dimension_block_at(dimension, player.position);
        let head = self.dimension_block_at(
            dimension,
            BlockPosition {
                y: player.position.y.saturating_add(1),
                ..player.position
            },
        );
        if restored_location && (feet != BlockKind::Air || head != BlockKind::Air) {
            player.position =
                self.safe_surface_position(dimension, player.position.x, player.position.z);
        }
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if players.iter().any(|current| current.id == player.id) {
            return false;
        }
        players.push(player);
        let joined = players.last().cloned().expect("player was just inserted");
        let id = joined.id;
        drop(players);
        self.player_transforms
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                id,
                PlayerTransform {
                    id,
                    position: EntityPosition {
                        x: f64::from(joined.position.x) + 0.5,
                        y: f64::from(joined.position.y),
                        z: f64::from(joined.position.z) + 0.5,
                    },
                    yaw: 0.0,
                    pitch: 0.0,
                    on_ground: true,
                    revision: 0,
                },
            );
        self.inventories
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .entry(id)
            .or_default();
        self.equipment
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .entry(id)
            .or_default();
        self.combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id, PlayerCombatState::default());
        self.vitals
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .entry(id)
            .or_default();
        self.status_effects
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .entry(id)
            .or_default();
        self.update_world_player_counts();
        self.record_chat(None, format!("{} joined the game", joined.name));
        true
    }

    fn remove_player(&self, id: Uuid) -> bool {
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let original_len = players.len();
        let leaving_name = players
            .iter()
            .find(|player| player.id == id)
            .map(|player| player.name.clone());
        if let Some(player) = players.iter().find(|player| player.id == id) {
            self.saved_locations
                .write()
                .unwrap_or_else(|error| error.into_inner())
                .insert(
                    id,
                    SavedLocation {
                        dimension: player.world.clone(),
                        x: player.position.x,
                        y: player.position.y,
                        z: player.position.z,
                    },
                );
        }
        players.retain(|player| player.id != id);
        let removed = players.len() != original_len;
        drop(players);
        if removed {
            self.lava_burn_until
                .write()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&id);
            self.player_transforms
                .write()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&id);
            self.combat_states
                .write()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&id);
            self.update_world_player_counts();
            if let Some(name) = leaving_name {
                self.record_chat(None, format!("{name} left the game"));
            }
        }
        removed
    }

    fn disconnects_since(&self, revision: u64) -> Vec<PlayerDisconnect> {
        self.disconnects
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|disconnect| disconnect.revision > revision)
            .cloned()
            .collect()
    }

    fn request_disconnect(&self, id: Uuid, reason: &str) -> bool {
        let valid_reason = !reason.is_empty()
            && reason.chars().count() <= 256
            && !reason.chars().any(char::is_control);
        let online = self
            .players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .any(|player| player.id == id);
        if !valid_reason || !online {
            return false;
        }
        let revision = self.disconnect_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let mut disconnects = self
            .disconnects
            .write()
            .unwrap_or_else(|error| error.into_inner());
        disconnects.push_back(PlayerDisconnect {
            revision,
            player_id: id,
            reason: reason.to_owned(),
        });
        while disconnects.len() > 1_024 {
            disconnects.pop_front();
        }
        true
    }

    fn update_player_position(&self, id: Uuid, position: BlockPosition) -> bool {
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(player) = players.iter_mut().find(|player| player.id == id) else {
            return false;
        };
        player.position = position;
        drop(players);
        if let Some(transform) = self
            .player_transforms
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&id)
        {
            transform.position = EntityPosition {
                x: f64::from(position.x) + 0.5,
                y: f64::from(position.y),
                z: f64::from(position.z) + 0.5,
            };
            transform.revision = transform.revision.saturating_add(1);
        }
        true
    }

    fn player_transforms(&self) -> Vec<PlayerTransform> {
        self.player_transforms
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .copied()
            .collect()
    }

    fn update_player_transform(&self, transform: PlayerTransform) -> bool {
        let mut transforms = self
            .player_transforms
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(current) = transforms.get_mut(&transform.id) else {
            return false;
        };
        *current = PlayerTransform {
            revision: current.revision.saturating_add(1),
            ..transform
        };
        drop(transforms);
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(player) = players.iter_mut().find(|player| player.id == transform.id) else {
            return false;
        };
        player.position = BlockPosition {
            x: transform.position.x.floor() as i32,
            y: transform.position.y.floor() as i32,
            z: transform.position.z.floor() as i32,
        };
        true
    }

    fn player_events_since(&self, revision: u64) -> Vec<PlayerEvent> {
        self.player_events
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|event| event.revision > revision)
            .copied()
            .collect()
    }

    fn swing_player(&self, id: Uuid, off_hand: bool) -> bool {
        if !self
            .player_transforms
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&id)
        {
            return false;
        }
        self.record_player_event(
            id,
            if off_hand {
                PlayerEventKind::SwingOffHand
            } else {
                PlayerEventKind::SwingMainArm
            },
        );
        true
    }

    fn critical_hit_player(&self, id: Uuid) -> bool {
        if !self
            .player_transforms
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&id)
        {
            return false;
        }
        self.record_player_event(id, PlayerEventKind::CriticalHit);
        true
    }

    fn worlds(&self) -> Vec<WorldSnapshot> {
        self.worlds
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn chunks(&self) -> Vec<ChunkSnapshot> {
        self.simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .chunks()
    }

    fn chunk_surface(&self, position: ChunkPosition) -> Option<[i16; 256]> {
        self.simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .chunk_surface(position)
    }

    fn ensure_chunk_surface(&self, position: ChunkPosition) -> [i16; 256] {
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .ensure_chunk_surface(position)
    }

    fn chunk_biome(&self, position: ChunkPosition) -> carbon_api::BiomeKind {
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .ensure_chunk_biome(position)
    }

    fn chunk_terrain(&self, position: ChunkPosition) -> TerrainProfile {
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .ensure_chunk_terrain(position)
    }

    fn chunk_blocks(&self, position: ChunkPosition) -> Vec<BlockPlacement> {
        self.simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .chunk_blocks(position)
    }

    fn block_at(&self, position: BlockPosition) -> BlockKind {
        self.simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .block_at(position)
    }

    fn set_block(&self, position: BlockPosition, kind: BlockKind) -> bool {
        let previous = self.block_at(position);
        let changed = self
            .simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .set_block(position, kind);
        if changed {
            let revision = self.block_revision.fetch_add(1, Ordering::Relaxed) + 1;
            let mut changes = self
                .block_changes
                .write()
                .unwrap_or_else(|error| error.into_inner());
            changes.push_back(BlockChange {
                revision,
                dimension: DimensionKind::Overworld,
                position,
                kind,
            });
            if changes.len() > 4_096 {
                changes.pop_front();
            }
        }
        if changed {
            if let Some(y) = position.y.checked_add(1) {
                let above = BlockPosition { y, ..position };
                let plant = self.block_at(above);
                if plant.is_surface_plant() && !plant.plant_survives_on(kind) {
                    self.set_block(above, BlockKind::Air);
                }
            }
        }
        if changed && previous == BlockKind::Obsidian && kind != BlockKind::Obsidian {
            self.collapse_nether_portal_near(DimensionKind::Overworld, position);
        }
        changed
    }

    fn block_changes_since(&self, revision: u64) -> Vec<BlockChange> {
        self.block_changes
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|change| change.revision > revision)
            .copied()
            .collect()
    }

    fn change_player_dimension(&self, id: Uuid, dimension: DimensionKind) -> bool {
        let world_name = match dimension {
            DimensionKind::Overworld => self
                .worlds
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .first()
                .map_or_else(|| "world".into(), |world| world.name.clone()),
            _ => dimension.name().into(),
        };
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(player) = players.iter_mut().find(|player| player.id == id) else {
            return false;
        };
        let current_dimension = if player.world == DimensionKind::Nether.name() {
            DimensionKind::Nether
        } else if player.world == DimensionKind::End.name() {
            DimensionKind::End
        } else {
            DimensionKind::Overworld
        };
        let (mut x, mut z) = match (current_dimension, dimension) {
            (_, DimensionKind::End) => (-8, -8),
            (DimensionKind::End, DimensionKind::Overworld) => (0, 0),
            (DimensionKind::Overworld, DimensionKind::Nether) => (
                player.position.x.div_euclid(8),
                player.position.z.div_euclid(8),
            ),
            (DimensionKind::Nether, DimensionKind::Overworld) => (
                player.position.x.saturating_mul(8),
                player.position.z.saturating_mul(8),
            ),
            _ => (player.position.x, player.position.z),
        };
        let mut surface = self.ensure_dimension_chunk_surface(
            dimension,
            ChunkPosition {
                x: x.div_euclid(16),
                z: z.div_euclid(16),
            },
        );
        let mut index = usize::try_from(z.rem_euclid(16) * 16 + x.rem_euclid(16)).unwrap_or(0);
        if dimension == DimensionKind::End && surface[index] < -64 {
            x = -8;
            z = -8;
            surface =
                self.ensure_dimension_chunk_surface(dimension, ChunkPosition { x: -1, z: -1 });
            index = 8 * 16 + 8;
        }
        player.world = world_name;
        player.position.x = x;
        player.position.z = z;
        player.position.y = i32::from(surface[index]) + 1;
        let position = player.position;
        drop(players);
        if let Some(transform) = self
            .player_transforms
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&id)
        {
            transform.position = EntityPosition {
                x: f64::from(position.x) + 0.5,
                y: f64::from(position.y),
                z: f64::from(position.z) + 0.5,
            };
            transform.revision = transform.revision.saturating_add(1);
        }
        if let Some(combat) = self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&id)
        {
            *combat = PlayerCombatState::default();
        }
        true
    }

    fn ensure_dimension_chunk_surface(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> [i16; 256] {
        if dimension == DimensionKind::Overworld {
            return self.ensure_chunk_surface(position);
        }
        self.dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&dimension)
            .expect("built-in dimension exists")
            .ensure_chunk_surface(position)
    }

    fn dimension_chunk_biome(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> carbon_api::BiomeKind {
        if dimension == DimensionKind::Overworld {
            return self.chunk_biome(position);
        }
        self.dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&dimension)
            .expect("built-in dimension exists")
            .ensure_chunk_biome(position)
    }

    fn dimension_chunk_terrain(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> TerrainProfile {
        if dimension == DimensionKind::Overworld {
            return self.chunk_terrain(position);
        }
        self.dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&dimension)
            .expect("built-in dimension exists")
            .ensure_chunk_terrain(position)
    }

    fn dimension_chunk_blocks(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> Vec<BlockPlacement> {
        if dimension == DimensionKind::Overworld {
            return self.chunk_blocks(position);
        }
        self.dimensions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&dimension)
            .expect("built-in dimension exists")
            .chunk_blocks(position)
    }

    fn dimension_block_at(&self, dimension: DimensionKind, position: BlockPosition) -> BlockKind {
        if dimension == DimensionKind::Overworld {
            return self.block_at(position);
        }
        self.dimensions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&dimension)
            .expect("built-in dimension exists")
            .block_at(position)
    }

    fn set_dimension_block(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
        kind: BlockKind,
    ) -> bool {
        if dimension == DimensionKind::Overworld {
            return self.set_block(position, kind);
        }
        let previous = self.dimension_block_at(dimension, position);
        let changed = self
            .dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&dimension)
            .expect("built-in dimension exists")
            .set_block(position, kind);
        if changed {
            let revision = self.block_revision.fetch_add(1, Ordering::Relaxed) + 1;
            self.block_changes
                .write()
                .unwrap_or_else(|error| error.into_inner())
                .push_back(BlockChange {
                    revision,
                    dimension,
                    position,
                    kind,
                });
        }
        if changed {
            if let Some(y) = position.y.checked_add(1) {
                let above = BlockPosition { y, ..position };
                let plant = self.dimension_block_at(dimension, above);
                if plant.is_surface_plant() && !plant.plant_survives_on(kind) {
                    self.set_dimension_block(dimension, above, BlockKind::Air);
                }
            }
        }
        if changed && previous == BlockKind::Obsidian && kind != BlockKind::Obsidian {
            self.collapse_nether_portal_near(dimension, position);
        }
        changed
    }

    fn ignite_nether_portal(&self, dimension: DimensionKind, position: BlockPosition) -> bool {
        self.activate_nether_portal_near(dimension, position)
    }

    fn furnace(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
    ) -> Option<FurnaceSnapshot> {
        if self.dimension_block_at(dimension, position) != BlockKind::Furnace {
            return None;
        }
        let mut furnaces = self
            .furnaces
            .write()
            .unwrap_or_else(|error| error.into_inner());
        Some(
            *furnaces
                .entry((dimension, position))
                .or_insert(FurnaceSnapshot {
                    cook_total: 200,
                    ..FurnaceSnapshot::default()
                }),
        )
    }

    fn click_furnace_slot(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
        slot: FurnaceSlot,
        cursor: InventoryCursor,
        right_click: bool,
    ) -> Option<InventoryCursor> {
        if self.dimension_block_at(dimension, position) != BlockKind::Furnace {
            return None;
        }
        let mut furnaces = self
            .furnaces
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let furnace = furnaces
            .entry((dimension, position))
            .or_insert(FurnaceSnapshot {
                cook_total: 200,
                ..FurnaceSnapshot::default()
            });
        let target = match slot {
            FurnaceSlot::Input => &mut furnace.input,
            FurnaceSlot::Fuel => &mut furnace.fuel,
            FurnaceSlot::Output => &mut furnace.output,
        };
        let result = click_stack(*target, 0, cursor, right_click, |kind| match slot {
            FurnaceSlot::Input => smelting_result(kind).is_some(),
            FurnaceSlot::Fuel => fuel_burn_ticks(kind) > 0,
            FurnaceSlot::Output => false,
        });
        if result.slot != *target {
            *target = result.slot;
            furnace.revision = furnace.revision.wrapping_add(1);
        }
        Some(result.cursor)
    }

    fn take_furnace_contents(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
    ) -> Vec<ItemStack> {
        self.furnaces
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&(dimension, position))
            .into_iter()
            .flat_map(|furnace| [furnace.input, furnace.fuel, furnace.output])
            .flatten()
            .collect()
    }

    fn chest(&self, dimension: DimensionKind, position: BlockPosition) -> Option<ChestSnapshot> {
        if self.dimension_block_at(dimension, position) != BlockKind::Chest {
            return None;
        }
        let initial = self
            .generated_chest_loot(dimension, position)
            .unwrap_or_default();
        let mut chests = self
            .chests
            .write()
            .unwrap_or_else(|error| error.into_inner());
        Some(*chests.entry((dimension, position)).or_insert(initial))
    }

    fn click_chest_slot(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
        slot: u8,
        cursor: InventoryCursor,
        right_click: bool,
    ) -> Option<InventoryCursor> {
        if slot >= 27 || self.dimension_block_at(dimension, position) != BlockKind::Chest {
            return None;
        }
        let initial = self
            .generated_chest_loot(dimension, position)
            .unwrap_or_default();
        let mut chests = self
            .chests
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let chest = chests.entry((dimension, position)).or_insert(initial);
        let index = usize::from(slot);
        let result = click_stack(
            chest.slots[index],
            chest.sharpness_levels[index],
            cursor,
            right_click,
            |_| true,
        );
        if result.slot != chest.slots[index]
            || result.slot_sharpness != chest.sharpness_levels[index]
        {
            chest.slots[index] = result.slot;
            chest.sharpness_levels[index] = result.slot_sharpness;
            chest.revision = chest.revision.wrapping_add(1);
        }
        Some(result.cursor)
    }

    fn take_chest_contents(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
    ) -> Vec<ItemStack> {
        let generated = self.generated_chest_loot(dimension, position);
        let stored = self
            .chests
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&(dimension, position));
        stored
            .or(generated)
            .into_iter()
            .flat_map(|chest| chest.slots)
            .flatten()
            .collect()
    }

    fn inventory(&self, id: Uuid) -> Option<PlayerInventory> {
        self.inventories
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .cloned()
    }

    fn player_equipment(&self, id: Uuid) -> Option<PlayerEquipment> {
        let mut equipment = self
            .equipment
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .copied()?;
        let inventory = self
            .inventories
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .cloned()?;
        equipment.main_hand = inventory.slots[usize::from(equipment.selected_slot)];
        equipment.main_hand_sharpness =
            inventory.sharpness_levels[usize::from(equipment.selected_slot)];
        equipment.revision = equipment.revision.saturating_add(inventory.revision);
        Some(equipment)
    }

    fn set_selected_slot(&self, id: Uuid, slot: u8) -> bool {
        if slot >= 9 {
            return false;
        }
        let mut equipment = self
            .equipment
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(equipment) = equipment.get_mut(&id) else {
            return false;
        };
        if equipment.selected_slot != slot {
            equipment.selected_slot = slot;
            equipment.revision = equipment.revision.saturating_add(1);
        }
        true
    }

    fn enchant_selected_weapon(&self, id: Uuid, sharpness_level: u8) -> bool {
        let selected_slot = self
            .equipment
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .map(|equipment| usize::from(equipment.selected_slot));
        let Some(selected_slot) = selected_slot else {
            return false;
        };
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        if !inventory.slots[selected_slot].is_some_and(|stack| {
            matches!(
                stack.kind,
                ItemKind::WoodenSword
                    | ItemKind::WoodenAxe
                    | ItemKind::StoneSword
                    | ItemKind::StoneAxe
                    | ItemKind::IronSword
                    | ItemKind::IronAxe
                    | ItemKind::DiamondSword
                    | ItemKind::DiamondAxe
            )
        }) {
            return false;
        }
        let level = sharpness_level.clamp(1, 5);
        if inventory.sharpness_levels[selected_slot] != level {
            inventory.sharpness_levels[selected_slot] = level;
            inventory.revision = inventory.revision.saturating_add(1);
        }
        true
    }

    fn set_player_armor(&self, id: Uuid, armor: [Option<ItemStack>; 4]) -> bool {
        let mut equipment = self
            .equipment
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(equipment) = equipment.get_mut(&id) else {
            return false;
        };
        if equipment.armor != armor {
            equipment.armor = armor;
            equipment.revision = equipment.revision.saturating_add(1);
        }
        true
    }

    fn set_offhand(&self, id: Uuid, stack: Option<ItemStack>) -> bool {
        let mut equipment = self
            .equipment
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(equipment) = equipment.get_mut(&id) else {
            return false;
        };
        if equipment.off_hand != stack {
            equipment.off_hand = stack;
            equipment.revision = equipment.revision.saturating_add(1);
        }
        true
    }

    fn click_player_inventory_slot(
        &self,
        id: Uuid,
        slot: PlayerInventorySlot,
        cursor: InventoryCursor,
        right_click: bool,
    ) -> Option<InventoryCursor> {
        match slot {
            PlayerInventorySlot::Storage(index) => {
                let index = usize::from(index);
                if index >= 36 {
                    return None;
                }
                let mut inventories = self
                    .inventories
                    .write()
                    .unwrap_or_else(|error| error.into_inner());
                let inventory = inventories.get_mut(&id)?;
                let previous = (inventory.slots[index], inventory.sharpness_levels[index]);
                let next = click_stack(previous.0, previous.1, cursor, right_click, |_| true);
                if previous != (next.slot, next.slot_sharpness) {
                    inventory.slots[index] = next.slot;
                    inventory.sharpness_levels[index] = next.slot_sharpness;
                    inventory.revision = inventory.revision.saturating_add(1);
                }
                Some(next.cursor)
            }
            PlayerInventorySlot::Armor(index) => {
                let index = usize::from(index);
                if index >= 4 {
                    return None;
                }
                let mut equipment = self
                    .equipment
                    .write()
                    .unwrap_or_else(|error| error.into_inner());
                let equipment = equipment.get_mut(&id)?;
                let previous = equipment.armor[index];
                let next = click_stack(previous, 0, cursor, right_click, |kind| {
                    armor_slot_accepts(index, kind)
                });
                if previous != next.slot {
                    equipment.armor[index] = next.slot;
                    equipment.revision = equipment.revision.saturating_add(1);
                }
                Some(next.cursor)
            }
            PlayerInventorySlot::OffHand => {
                let mut equipment = self
                    .equipment
                    .write()
                    .unwrap_or_else(|error| error.into_inner());
                let equipment = equipment.get_mut(&id)?;
                let previous = equipment.off_hand;
                let next = click_stack(previous, 0, cursor, right_click, |_| true);
                if previous != next.slot {
                    equipment.off_hand = next.slot;
                    equipment.revision = equipment.revision.saturating_add(1);
                }
                let shield_equipped = next
                    .slot
                    .is_some_and(|stack| stack.kind == ItemKind::Shield);
                if !shield_equipped {
                    if let Some(combat) = self
                        .combat_states
                        .write()
                        .unwrap_or_else(|error| error.into_inner())
                        .get_mut(&id)
                    {
                        combat.blocking_since = None;
                    }
                }
                Some(next.cursor)
            }
        }
    }

    fn player_combat_state(&self, id: Uuid) -> Option<PlayerCombatState> {
        self.combat_states
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .copied()
    }

    fn set_player_sprinting(&self, id: Uuid, sprinting: bool) -> bool {
        let mut states = self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(state) = states.get_mut(&id) else {
            return false;
        };
        state.sprinting = sprinting;
        true
    }

    fn set_player_falling(&self, id: Uuid, falling: bool) -> bool {
        let mut states = self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(state) = states.get_mut(&id) else {
            return false;
        };
        state.falling = falling;
        true
    }

    fn set_player_blocking(&self, id: Uuid, blocking: bool) -> bool {
        let mut states = self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(state) = states.get_mut(&id) else {
            return false;
        };
        let tick = self.current_tick();
        if blocking && tick < state.shield_disabled_until {
            state.blocking_since = None;
            return false;
        }
        state.blocking_since = blocking.then_some(tick);
        true
    }

    fn disable_player_shield(&self, id: Uuid, duration_ticks: u64) -> bool {
        let mut states = self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(state) = states.get_mut(&id) else {
            return false;
        };
        state.blocking_since = None;
        state.shield_disabled_until = self.current_tick().saturating_add(duration_ticks);
        true
    }

    fn give_item(&self, id: Uuid, kind: ItemKind, mut count: u8) -> bool {
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        let original_count = count;
        let maximum = max_stack_size(kind);
        for stack in inventory.slots.iter_mut().flatten() {
            if stack.kind == kind && stack.damage == 0 && stack.count < maximum {
                let moved = count.min(maximum - stack.count);
                stack.count += moved;
                count -= moved;
                if count == 0 {
                    inventory.revision = inventory.revision.saturating_add(1);
                    return true;
                }
            }
        }
        let (slots, sharpness_levels) = (&mut inventory.slots, &mut inventory.sharpness_levels);
        for (index, slot) in slots
            .iter_mut()
            .enumerate()
            .filter(|(_, slot)| slot.is_none())
        {
            let moved = count.min(maximum);
            *slot = Some(ItemStack {
                kind,
                count: moved,
                damage: 0,
            });
            sharpness_levels[index] = 0;
            count -= moved;
            if count == 0 {
                inventory.revision = inventory.revision.saturating_add(1);
                return true;
            }
        }
        if count < original_count {
            inventory.revision = inventory.revision.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn take_item(&self, id: Uuid, slot: usize, count: u8) -> bool {
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        let Some(stack) = inventory.slots.get_mut(slot).and_then(Option::as_mut) else {
            return false;
        };
        if stack.count < count {
            return false;
        }
        stack.count -= count;
        if inventory.slots[slot].is_some_and(|stack| stack.count == 0) {
            inventory.slots[slot] = None;
            inventory.sharpness_levels[slot] = 0;
        }
        inventory.revision = inventory.revision.saturating_add(1);
        true
    }

    fn damage_item(&self, id: Uuid, slot: usize, amount: u16) -> bool {
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        let Some(stack) = inventory.slots.get_mut(slot).and_then(Option::as_mut) else {
            return false;
        };
        if !matches!(
            stack.kind,
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
        ) {
            return false;
        }
        stack.damage = stack.damage.saturating_add(amount);
        if stack.damage >= equipment_max_damage(stack.kind) {
            inventory.slots[slot] = None;
            inventory.sharpness_levels[slot] = 0;
        }
        inventory.revision = inventory.revision.saturating_add(1);
        true
    }

    fn craft(&self, id: Uuid, recipe: &str) -> bool {
        let Some((ingredients, output)) = recipe_definition(recipe) else {
            return false;
        };
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        if ingredients.iter().any(|(kind, needed)| {
            inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| stack.kind == *kind)
                .map(|stack| u16::from(stack.count))
                .sum::<u16>()
                < u16::from(*needed)
        }) {
            return false;
        }
        let output_maximum = max_stack_size(output.kind);
        let capacity: u16 = inventory
            .slots
            .iter()
            .map(|slot| match slot {
                Some(stack) if stack.kind == output.kind && stack.damage == output.damage => {
                    u16::from(output_maximum - stack.count)
                }
                None => u16::from(output_maximum),
                _ => 0,
            })
            .sum();
        if capacity < u16::from(output.count) {
            return false;
        }
        for (kind, mut needed) in ingredients {
            for (index, slot) in inventory.slots.iter_mut().enumerate() {
                let Some(stack) = slot.as_mut().filter(|stack| stack.kind == kind) else {
                    continue;
                };
                let removed = stack.count.min(needed);
                stack.count -= removed;
                needed -= removed;
                if stack.count == 0 {
                    *slot = None;
                    inventory.sharpness_levels[index] = 0;
                }
                if needed == 0 {
                    break;
                }
            }
        }
        add_stack(inventory, output);
        inventory.revision = inventory.revision.saturating_add(1);
        true
    }

    fn consume_food(&self, id: Uuid, slot: usize) -> bool {
        let mut vitals = self
            .vitals
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(player_vitals) = vitals.get_mut(&id) else {
            return false;
        };
        if player_vitals.health <= 0.0 || player_vitals.food >= 20 {
            return false;
        }
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        let Some(stack) = inventory.slots.get(slot).and_then(|slot| *slot) else {
            return false;
        };
        let (food, saturation) = match stack.kind {
            ItemKind::Apple => (4, 2.4),
            ItemKind::RawBeef | ItemKind::Porkchop => (3, 1.8),
            ItemKind::CookedBeef | ItemKind::CookedPorkchop => (8, 12.8),
            ItemKind::RottenFlesh => (4, 0.8),
            _ => return false,
        };
        let rotten_flesh = stack.kind == ItemKind::RottenFlesh;
        let stack = inventory.slots[slot].as_mut().expect("validated slot");
        stack.count -= 1;
        if stack.count == 0 {
            inventory.slots[slot] = None;
            inventory.sharpness_levels[slot] = 0;
        }
        player_vitals.food = player_vitals.food.saturating_add(food).min(20);
        player_vitals.saturation =
            (player_vitals.saturation + saturation).min(f32::from(player_vitals.food));
        player_vitals.revision = player_vitals.revision.saturating_add(1);
        inventory.revision = inventory.revision.saturating_add(1);
        drop(inventories);
        drop(vitals);
        if rotten_flesh {
            self.apply_status_effect(id, StatusEffectKind::Hunger, 0, 600);
        }
        true
    }

    fn fill_milk_bucket(&self, id: Uuid, slot: usize) -> bool {
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        let Some(bucket) = inventory.slots.get(slot).and_then(|slot| *slot) else {
            return false;
        };
        if bucket.kind != ItemKind::Bucket || bucket.count == 0 {
            return false;
        }
        if bucket.count == 1 {
            inventory.slots[slot] = Some(ItemStack {
                kind: ItemKind::MilkBucket,
                count: 1,
                damage: 0,
            });
        } else {
            let Some(empty_slot) = inventory.slots.iter().position(Option::is_none) else {
                return false;
            };
            inventory.slots[slot]
                .as_mut()
                .expect("validated bucket")
                .count -= 1;
            inventory.slots[empty_slot] = Some(ItemStack {
                kind: ItemKind::MilkBucket,
                count: 1,
                damage: 0,
            });
            inventory.sharpness_levels[empty_slot] = 0;
        }
        inventory.sharpness_levels[slot] = 0;
        inventory.revision = inventory.revision.saturating_add(1);
        true
    }

    fn consume_milk(&self, id: Uuid, slot: usize) -> bool {
        let mut inventories = self
            .inventories
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(inventory) = inventories.get_mut(&id) else {
            return false;
        };
        let Some(milk) = inventory.slots.get(slot).and_then(|slot| *slot) else {
            return false;
        };
        if milk.kind != ItemKind::MilkBucket || milk.count != 1 {
            return false;
        }
        inventory.slots[slot] = Some(ItemStack {
            kind: ItemKind::Bucket,
            count: 1,
            damage: 0,
        });
        inventory.sharpness_levels[slot] = 0;
        inventory.revision = inventory.revision.saturating_add(1);
        drop(inventories);
        self.status_effects
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&id);
        self.lava_burn_until
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&id);
        true
    }

    fn vitals(&self, id: Uuid) -> Option<PlayerVitals> {
        self.vitals
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .copied()
    }

    fn status_effects(&self, id: Uuid) -> Vec<StatusEffectSnapshot> {
        let mut effects = self
            .status_effects
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&id)
            .map(|effects| effects.values().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        effects.sort_unstable_by_key(|effect| effect.kind.protocol_id());
        effects
    }

    fn apply_status_effect(
        &self,
        id: Uuid,
        kind: StatusEffectKind,
        amplifier: u8,
        duration_ticks: u32,
    ) -> bool {
        if duration_ticks == 0
            || amplifier > 4
            || !self
                .players
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .iter()
                .any(|player| player.id == id)
        {
            return false;
        }
        let revision = self.effect_revision.fetch_add(1, Ordering::Relaxed) + 1;
        self.status_effects
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .entry(id)
            .or_default()
            .insert(
                kind,
                StatusEffectSnapshot {
                    kind,
                    amplifier,
                    remaining_ticks: duration_ticks,
                    revision,
                },
            );
        true
    }

    fn clear_status_effect(&self, id: Uuid, kind: StatusEffectKind) -> bool {
        let mut all_effects = self
            .status_effects
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let removed = all_effects
            .get_mut(&id)
            .and_then(|effects| effects.remove(&kind))
            .is_some();
        if all_effects.get(&id).is_some_and(HashMap::is_empty) {
            all_effects.remove(&id);
        }
        removed
    }

    fn damage_player(&self, id: Uuid, amount: f32) -> bool {
        let mut vitals = self
            .vitals
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(value) = vitals.get_mut(&id) else {
            return false;
        };
        if value.health <= 0.0 || amount <= 0.0 {
            return false;
        }
        value.health = (value.health - amount).max(0.0);
        let died = value.health <= 0.0;
        value.revision = value.revision.saturating_add(1);
        drop(vitals);
        self.record_player_event(
            id,
            if died {
                PlayerEventKind::Died
            } else {
                PlayerEventKind::Hurt
            },
        );
        true
    }

    fn damage_player_combat(&self, id: Uuid, amount: f32) -> bool {
        let (armor_points, toughness) =
            self.player_equipment(id)
                .map_or((0.0_f32, 0.0_f32), |equipment| {
                    equipment.armor.iter().flatten().fold(
                        (0.0, 0.0),
                        |(points, toughness), stack| {
                            let (piece_points, piece_toughness) = match stack.kind {
                                ItemKind::IronHelmet | ItemKind::IronBoots => (2.0, 0.0),
                                ItemKind::IronLeggings => (5.0, 0.0),
                                ItemKind::IronChestplate => (6.0, 0.0),
                                ItemKind::DiamondHelmet | ItemKind::DiamondBoots => (3.0, 2.0),
                                ItemKind::DiamondLeggings => (6.0, 2.0),
                                ItemKind::DiamondChestplate => (8.0, 2.0),
                                _ => (0.0, 0.0),
                            };
                            (points + piece_points, toughness + piece_toughness)
                        },
                    )
                });
        let protection = (armor_points / 5.0)
            .max(armor_points - amount / (2.0 + toughness / 4.0))
            .min(20.0);
        let resistance = self
            .status_effects(id)
            .into_iter()
            .find(|effect| effect.kind == StatusEffectKind::Resistance)
            .map_or(0.0, |effect| 0.2 * f32::from(effect.amplifier + 1))
            .min(0.8);
        let damaged =
            self.damage_player(id, amount * (1.0 - protection / 25.0) * (1.0 - resistance));
        if damaged {
            let mut equipment = self
                .equipment
                .write()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(equipment) = equipment.get_mut(&id) {
                for slot in &mut equipment.armor {
                    let Some(stack) = slot.as_mut() else {
                        continue;
                    };
                    stack.damage = stack.damage.saturating_add(1);
                    if stack.damage >= equipment_max_damage(stack.kind) {
                        *slot = None;
                    }
                }
                equipment.revision = equipment.revision.saturating_add(1);
            }
        }
        damaged
    }

    fn attack_player(&self, attacker: Uuid, target: Uuid, amount: f32) -> bool {
        let combat = self.player_combat_state(target);
        let equipment = self.player_equipment(target);
        let shield_ready = combat
            .filter(|state| self.current_tick() >= state.shield_disabled_until)
            .and_then(|state| state.blocking_since)
            .is_some_and(|since| self.current_tick().saturating_sub(since) >= 5)
            && equipment.is_some_and(|equipment| {
                equipment
                    .off_hand
                    .is_some_and(|stack| stack.kind == ItemKind::Shield)
            });
        if shield_ready {
            let transforms = self.player_transforms();
            let attacker_transform = transforms.iter().find(|player| player.id == attacker);
            let target_transform = transforms.iter().find(|player| player.id == target);
            if let (Some(attacker_transform), Some(target_transform)) =
                (attacker_transform, target_transform)
            {
                let dx = attacker_transform.position.x - target_transform.position.x;
                let dz = attacker_transform.position.z - target_transform.position.z;
                let yaw = f64::from(target_transform.yaw).to_radians();
                let facing_dot = dx * -yaw.sin() + dz * yaw.cos();
                if facing_dot > 0.0 {
                    let mut equipment = self
                        .equipment
                        .write()
                        .unwrap_or_else(|error| error.into_inner());
                    if let Some(equipment) = equipment.get_mut(&target) {
                        if let Some(shield) = equipment.off_hand.as_mut() {
                            if amount >= 3.0 {
                                shield.damage =
                                    shield.damage.saturating_add(1 + amount.floor() as u16);
                                if shield.damage >= equipment_max_damage(ItemKind::Shield) {
                                    equipment.off_hand = None;
                                    self.set_player_blocking(target, false);
                                }
                                equipment.revision = equipment.revision.saturating_add(1);
                            }
                        }
                    }
                    return false;
                }
            }
        }
        self.damage_player_combat(target, amount)
    }

    fn knockback_player(&self, attacker: Uuid, target: Uuid, strength: f64) -> bool {
        if strength <= 0.0 || !strength.is_finite() {
            return false;
        }
        let transforms = self.player_transforms();
        let Some(attacker) = transforms.iter().find(|player| player.id == attacker) else {
            return false;
        };
        let Some(target_transform) = transforms.iter().find(|player| player.id == target) else {
            return false;
        };
        let mut dx = target_transform.position.x - attacker.position.x;
        let mut dz = target_transform.position.z - attacker.position.z;
        let length = dx.hypot(dz);
        if length < 0.000_1 {
            let yaw = f64::from(attacker.yaw).to_radians();
            dx = -yaw.sin();
            dz = yaw.cos();
        } else {
            dx /= length;
            dz /= length;
        }
        let revision = self.player_impulse_revision.fetch_add(1, Ordering::Relaxed) + 1;
        let mut impulses = self
            .player_impulses
            .write()
            .unwrap_or_else(|error| error.into_inner());
        impulses.push_back(PlayerImpulse {
            revision,
            player_id: target,
            velocity: EntityPosition {
                x: dx * strength,
                y: 0.4,
                z: dz * strength,
            },
        });
        while impulses.len() > 1_024 {
            impulses.pop_front();
        }
        true
    }

    fn player_impulses_since(&self, revision: u64) -> Vec<PlayerImpulse> {
        self.player_impulses
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|impulse| impulse.revision > revision)
            .copied()
            .collect()
    }

    fn respawn_player(&self, id: Uuid) -> bool {
        let mut vitals = self
            .vitals
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(value) = vitals.get_mut(&id) else {
            return false;
        };
        *value = PlayerVitals {
            revision: value.revision.saturating_add(1),
            ..PlayerVitals::default()
        };
        drop(vitals);
        self.status_effects
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&id);
        let spawn = self.safe_surface_position(DimensionKind::Overworld, -8, -9);
        let mut players = self
            .players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let Some(player) = players.iter_mut().find(|player| player.id == id) else {
            return false;
        };
        let world_name = self
            .worlds
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .first()
            .map_or_else(|| "world".into(), |world| world.name.clone());
        player.world = world_name;
        player.position = spawn;
        drop(players);
        let moved = self.update_player_position(id, spawn);
        if let Some(combat) = self
            .combat_states
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&id)
        {
            *combat = PlayerCombatState::default();
        }
        moved
    }

    fn mobs(&self) -> Vec<MobSnapshot> {
        self.simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .mobs()
    }

    fn spawn_mob(&self, kind: MobKind, position: BlockPosition) -> MobSnapshot {
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .spawn_mob(kind, position)
    }

    fn damage_mob(&self, entity_id: i32, amount: f32) -> Option<MobSnapshot> {
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .damage_mob(entity_id, amount)
    }

    fn dimension_mobs(&self, dimension: DimensionKind) -> Vec<MobSnapshot> {
        if dimension == DimensionKind::Overworld {
            return self.mobs();
        }
        self.dimensions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&dimension)
            .expect("built-in dimension exists")
            .mobs()
    }

    fn damage_dimension_mob(
        &self,
        dimension: DimensionKind,
        entity_id: i32,
        amount: f32,
    ) -> Option<MobSnapshot> {
        if dimension == DimensionKind::Overworld {
            return self.damage_mob(entity_id, amount);
        }
        self.dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&dimension)
            .expect("built-in dimension exists")
            .damage_mob(entity_id, amount)
    }

    fn items(&self) -> Vec<ItemEntitySnapshot> {
        self.simulation
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .items()
    }

    fn drop_item(&self, stack: ItemStack, position: BlockPosition) -> ItemEntitySnapshot {
        self.simulation
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .drop_item(stack, position)
    }

    fn dimension_items(&self, dimension: DimensionKind) -> Vec<ItemEntitySnapshot> {
        if dimension == DimensionKind::Overworld {
            return self.items();
        }
        self.dimensions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&dimension)
            .expect("built-in dimension exists")
            .items()
    }

    fn drop_dimension_item(
        &self,
        dimension: DimensionKind,
        stack: ItemStack,
        position: BlockPosition,
    ) -> ItemEntitySnapshot {
        if dimension == DimensionKind::Overworld {
            return self.drop_item(stack, position);
        }
        self.dimensions
            .write()
            .unwrap_or_else(|error| error.into_inner())
            .get_mut(&dimension)
            .expect("built-in dimension exists")
            .drop_item(stack, position)
    }

    fn operators(&self) -> Vec<String> {
        let mut names: Vec<_> = self
            .operators
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .cloned()
            .collect();
        names.sort_by_key(|name| name.to_ascii_lowercase());
        names
    }

    fn is_operator(&self, name: &str) -> bool {
        self.operators
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&name.to_ascii_lowercase())
    }

    fn set_operator(&self, name: &str, operator: bool) -> carbon_api::Result<bool> {
        let key = name.to_ascii_lowercase();
        let mut operators = self
            .operators
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if operator {
            match operators.entry(key) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(name.to_owned());
                }
                std::collections::hash_map::Entry::Occupied(_) => return Ok(false),
            }
        } else if operators.remove(&key).is_none() {
            return Ok(false);
        }
        if let Some(path) = &self.operator_path {
            persist_operators(path, &operators)?;
        }
        Ok(true)
    }

    fn banned_players(&self) -> Vec<String> {
        self.bans().into_iter().map(|ban| ban.name).collect()
    }

    fn bans(&self) -> Vec<PlayerBan> {
        let now = unix_now();
        let mut bans: Vec<_> = self
            .banned_players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .values()
            .filter(|ban| !ban_expired(ban, now))
            .map(player_ban)
            .collect();
        bans.sort_by_key(|ban| ban.name.to_ascii_lowercase());
        bans
    }

    fn active_ban(&self, name: &str) -> Option<PlayerBan> {
        let key = name.to_ascii_lowercase();
        let now = unix_now();
        let mut bans = self
            .banned_players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let expired = bans.get(&key).is_some_and(|ban| ban_expired(ban, now));
        if expired {
            bans.remove(&key);
            if let Some(path) = &self.banned_path {
                if let Err(error) = persist_bans(path, &bans) {
                    tracing::warn!(%error, "could not persist expired ban cleanup");
                }
            }
            return None;
        }
        bans.get(&key).map(player_ban)
    }

    fn is_banned(&self, name: &str) -> bool {
        self.active_ban(name).is_some()
    }

    fn ban_player(
        &self,
        name: &str,
        reason: &str,
        expires_at_unix: Option<u64>,
    ) -> carbon_api::Result<bool> {
        if reason.is_empty() || reason.chars().count() > 256 || reason.chars().any(char::is_control)
        {
            return Ok(false);
        }
        if expires_at_unix.is_some_and(|expires| expires <= unix_now()) {
            return Ok(false);
        }
        let key = name.to_ascii_lowercase();
        let entry = BanEntry {
            name: name.to_owned(),
            reason: reason.to_owned(),
            expires_at_unix,
        };
        let mut bans = self
            .banned_players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let changed = match bans.get(&key) {
            Some(current) => {
                current.reason != entry.reason || current.expires_at_unix != entry.expires_at_unix
            }
            None => true,
        };
        if changed {
            bans.insert(key, entry);
            if let Some(path) = &self.banned_path {
                persist_bans(path, &bans)?;
            }
        }
        Ok(changed)
    }

    fn set_banned(&self, name: &str, banned: bool) -> carbon_api::Result<bool> {
        if banned {
            self.ban_player(name, "Banned by an operator.", None)
        } else {
            let mut bans = self
                .banned_players
                .write()
                .unwrap_or_else(|error| error.into_inner());
            let changed = bans.remove(&name.to_ascii_lowercase()).is_some();
            if changed {
                if let Some(path) = &self.banned_path {
                    persist_bans(path, &bans)?;
                }
            }
            Ok(changed)
        }
    }

    fn allowlisted_players(&self) -> Vec<String> {
        sorted_names(
            &self
                .allowlisted_players
                .read()
                .unwrap_or_else(|error| error.into_inner()),
        )
    }

    fn is_allowlisted(&self, name: &str) -> bool {
        self.allowlisted_players
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .contains_key(&name.to_ascii_lowercase())
    }

    fn set_allowlisted(&self, name: &str, allowed: bool) -> carbon_api::Result<bool> {
        let mut names = self
            .allowlisted_players
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let changed = set_name_membership(&mut names, name, allowed);
        if changed {
            if let Some(path) = &self.allowlist_path {
                persist_names(path, &names)?;
            }
        }
        Ok(changed)
    }

    fn permission_nodes(&self, name: &str) -> Vec<String> {
        self.permissions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&name.to_ascii_lowercase())
            .map_or_else(Vec::new, |nodes| nodes.iter().cloned().collect())
    }

    fn has_permission(&self, name: &str, permission: &str) -> bool {
        if self.is_operator(name) {
            return true;
        }
        let Some(permission) = normalize_permission(permission) else {
            return false;
        };
        // Deny rules are stored nodes, never permission queries.
        if permission.starts_with('!') {
            return false;
        }
        self.permissions
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .get(&name.to_ascii_lowercase())
            .is_some_and(|nodes| {
                let mut allowed = false;
                for node in nodes {
                    let (pattern, denied) = node
                        .strip_prefix('!')
                        .map_or((node.as_str(), false), |pattern| (pattern, true));
                    if permission_matches(pattern, &permission) {
                        // An explicit deny wins over every matching grant.
                        if denied {
                            return false;
                        }
                        allowed = true;
                    }
                }
                allowed
            })
    }

    fn grant_permission(&self, name: &str, permission: &str) -> carbon_api::Result<bool> {
        let Some(permission) = normalize_permission(permission) else {
            return Err("invalid permission node".into());
        };
        let mut permissions = self
            .permissions
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let changed = permissions
            .entry(name.to_ascii_lowercase())
            .or_default()
            .insert(permission);
        if changed {
            if let Some(path) = &self.permissions_path {
                persist_permissions(path, &permissions)?;
            }
        }
        Ok(changed)
    }

    fn revoke_permission(&self, name: &str, permission: &str) -> carbon_api::Result<bool> {
        let Some(permission) = normalize_permission(permission) else {
            return Err("invalid permission node".into());
        };
        let key = name.to_ascii_lowercase();
        let mut permissions = self
            .permissions
            .write()
            .unwrap_or_else(|error| error.into_inner());
        let changed = permissions
            .get_mut(&key)
            .is_some_and(|nodes| nodes.remove(&permission));
        if permissions.get(&key).is_some_and(BTreeSet::is_empty) {
            permissions.remove(&key);
        }
        if changed {
            if let Some(path) = &self.permissions_path {
                persist_permissions(path, &permissions)?;
            }
        }
        Ok(changed)
    }

    fn moderation_records(&self, limit: usize) -> Vec<ModerationRecord> {
        let records = self
            .moderation_records
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let mut recent: Vec<_> = records
            .iter()
            .rev()
            .take(limit.min(1_024))
            .cloned()
            .collect();
        recent.reverse();
        recent
    }

    fn record_moderation(
        &self,
        actor: &str,
        action: &str,
        target: Option<&str>,
        detail: &str,
    ) -> carbon_api::Result<()> {
        let valid = |value: &str, maximum: usize| {
            !value.is_empty()
                && value.chars().count() <= maximum
                && !value.chars().any(char::is_control)
        };
        if !valid(actor, 64)
            || !valid(action, 64)
            || !valid(detail, 1_024)
            || target.is_some_and(|value| !valid(value, 64))
        {
            return Err("invalid moderation audit record".into());
        }
        let record = ModerationRecord {
            timestamp_unix: unix_now(),
            actor: actor.to_owned(),
            action: action.to_owned(),
            target: target.map(str::to_owned),
            detail: detail.to_owned(),
        };
        if let Some(path) = &self.moderation_path {
            append_moderation_record(path, &record)?;
        }
        let mut records = self
            .moderation_records
            .write()
            .unwrap_or_else(|error| error.into_inner());
        records.push_back(record);
        while records.len() > 1_024 {
            records.pop_front();
        }
        Ok(())
    }

    fn chat_messages_since(&self, revision: u64) -> Vec<ChatMessage> {
        self.chat_messages
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .filter(|message| message.revision > revision)
            .cloned()
            .collect()
    }

    fn publish_chat(&self, sender: &str, message: &str) -> bool {
        let valid = !message.is_empty()
            && message.chars().count() <= 256
            && !message.chars().any(char::is_control)
            && self
                .players
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .iter()
                .any(|player| player.name == sender);
        if !valid {
            return false;
        }
        info!(target: "carbon::chat", sender, "{message}");
        self.record_chat(Some(sender.to_owned()), message.to_owned());
        true
    }

    fn broadcast(&self, message: &str) {
        info!(target: "carbon::broadcast", "{message}");
        if !message.is_empty()
            && message.chars().count() <= 1_024
            && !message.chars().any(char::is_control)
        {
            self.record_chat(None, message.to_owned());
        }
    }

    fn request_shutdown(&self) {
        self.shutdown.send_replace(true);
    }
}

fn persist_operators(path: &Path, operators: &HashMap<String, String>) -> carbon_api::Result<()> {
    persist_names(path, operators)?;
    Ok(())
}

fn load_name_map(path: &Path) -> anyhow::Result<HashMap<String, String>> {
    let names: Vec<String> = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(names
        .into_iter()
        .map(|name| (name.to_ascii_lowercase(), name))
        .collect())
}

fn load_permissions(path: &Path) -> anyhow::Result<HashMap<String, BTreeSet<String>>> {
    let stored: HashMap<String, Vec<String>> = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => return Err(error.into()),
    };
    let mut permissions: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (name, nodes) in stored {
        let normalized: BTreeSet<_> = nodes
            .into_iter()
            .map(|node| {
                normalize_permission(&node)
                    .ok_or_else(|| anyhow::anyhow!("invalid permission node '{node}'"))
            })
            .collect::<anyhow::Result<_>>()?;
        if !normalized.is_empty() {
            // Merge case aliases so a grant cannot accidentally discard a deny.
            permissions
                .entry(name.to_ascii_lowercase())
                .or_default()
                .extend(normalized);
        }
    }
    Ok(permissions)
}

fn persist_permissions(
    path: &Path,
    permissions: &HashMap<String, BTreeSet<String>>,
) -> carbon_api::Result<()> {
    let ordered: std::collections::BTreeMap<_, _> = permissions
        .iter()
        .map(|(name, nodes)| (name.clone(), nodes.iter().cloned().collect::<Vec<_>>()))
        .collect();
    atomic_write(path, &serde_json::to_vec_pretty(&ordered)?)?;
    Ok(())
}

fn normalize_permission(permission: &str) -> Option<String> {
    let normalized = permission.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > 128 {
        return None;
    }
    let pattern = normalized.strip_prefix('!').unwrap_or(&normalized);
    if pattern == "*" {
        return Some(normalized);
    }
    let segments: Vec<_> = pattern.split('.').collect();
    let valid = segments.iter().enumerate().all(|(index, segment)| {
        !segment.is_empty()
            && ((*segment == "*" && index + 1 == segments.len())
                || segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')))
    });
    valid.then_some(normalized)
}

fn permission_matches(pattern: &str, permission: &str) -> bool {
    pattern == "*"
        || pattern == permission
        || pattern.strip_suffix(".*").is_some_and(|prefix| {
            permission
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with('.') && suffix.len() > 1)
        })
}

fn load_bans(path: &Path) -> anyhow::Result<HashMap<String, BanEntry>> {
    let entries: Vec<SavedBanEntry> = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    Ok(entries
        .into_iter()
        .map(|entry| match entry {
            SavedBanEntry::LegacyName(name) => BanEntry {
                name,
                reason: "Banned by an operator.".into(),
                expires_at_unix: None,
            },
            SavedBanEntry::Record(record) => record,
        })
        .map(|entry| (entry.name.to_ascii_lowercase(), entry))
        .collect())
}

fn load_moderation_records(path: &Path) -> anyhow::Result<VecDeque<ModerationRecord>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(VecDeque::new()),
        Err(error) => return Err(error.into()),
    };
    let mut records = VecDeque::new();
    for (line_index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<StoredModerationRecord>(line) {
            Ok(record) => {
                records.push_back(ModerationRecord {
                    timestamp_unix: record.timestamp_unix,
                    actor: record.actor,
                    action: record.action,
                    target: record.target,
                    detail: record.detail,
                });
                while records.len() > 1_024 {
                    records.pop_front();
                }
            }
            Err(error) => tracing::warn!(
                line = line_index + 1,
                %error,
                "ignored malformed moderation audit line"
            ),
        }
    }
    Ok(records)
}

fn append_moderation_record(path: &Path, record: &ModerationRecord) -> carbon_api::Result<()> {
    let stored = StoredModerationRecord {
        timestamp_unix: record.timestamp_unix,
        actor: record.actor.clone(),
        action: record.action.clone(),
        target: record.target.clone(),
        detail: record.detail.clone(),
    };
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    serde_json::to_writer(&mut file, &stored)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

fn persist_bans(path: &Path, bans: &HashMap<String, BanEntry>) -> carbon_api::Result<()> {
    let mut entries: Vec<_> = bans.values().cloned().collect();
    entries.sort_by_key(|ban| ban.name.to_ascii_lowercase());
    atomic_write(path, &serde_json::to_vec_pretty(&entries)?)?;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn ban_expired(ban: &BanEntry, now: u64) -> bool {
    ban.expires_at_unix.is_some_and(|expires| expires <= now)
}

fn player_ban(ban: &BanEntry) -> PlayerBan {
    PlayerBan {
        name: ban.name.clone(),
        reason: ban.reason.clone(),
        expires_at_unix: ban.expires_at_unix,
    }
}

fn sorted_names(names: &HashMap<String, String>) -> Vec<String> {
    let mut values: Vec<_> = names.values().cloned().collect();
    values.sort_by_key(|name| name.to_ascii_lowercase());
    values
}

fn persist_names(path: &Path, names: &HashMap<String, String>) -> carbon_api::Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(&sorted_names(names))?)?;
    Ok(())
}

fn set_name_membership(names: &mut HashMap<String, String>, name: &str, present: bool) -> bool {
    let key = name.to_ascii_lowercase();
    if present {
        match names.entry(key) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(name.to_owned());
                true
            }
            std::collections::hash_map::Entry::Occupied(_) => false,
        }
    } else {
        names.remove(&key).is_some()
    }
}

// Validate metadata before decoding the payload: future schemas may change its shape.
fn validate_save_version(value: &serde_json::Value) -> anyhow::Result<()> {
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| anyhow::anyhow!("world save schema version must be an unsigned integer"))?;
    anyhow::ensure!(
        (u64::from(OLDEST_SUPPORTED_SAVE_SCHEMA_VERSION)..=u64::from(CURRENT_SAVE_SCHEMA_VERSION))
            .contains(&version),
        "unsupported world save schema version {version}; supported versions are {} through {}",
        OLDEST_SUPPORTED_SAVE_SCHEMA_VERSION,
        CURRENT_SAVE_SCHEMA_VERSION
    );
    let generator = match value.get("generator_version") {
        None if version == 1 => 0,
        Some(value) => value
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("invalid world generator version"))?,
        None => anyhow::bail!("world save schema 2 requires generator_version"),
    };
    anyhow::ensure!(generator <= u64::from(CURRENT_GENERATOR_VERSION),
        "world generator version {generator} is newer than this server supports ({CURRENT_GENERATOR_VERSION}); refusing an unsafe downgrade");
    Ok(())
}

fn decode_save(bytes: &[u8]) -> anyhow::Result<SaveData> {
    let value = serde_json::from_slice(bytes)?;
    validate_save_version(&value)?;
    Ok(serde_json::from_value(value)?)
}

fn load_save_file(path: &Path) -> anyhow::Result<Option<SaveData>> {
    let backup = path.with_extension("json.bak");
    match fs::read(path) {
        Ok(bytes) => {
            let parsed = match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) => {
                    // A readable incompatible header must never roll back to an older backup.
                    validate_save_version(&value)?;
                    serde_json::from_value::<SaveData>(value)
                }
                Err(error) => Err(error),
            };
            match parsed {
                Ok(data) => {
                    if data.generator_version < CURRENT_GENERATOR_VERSION {
                        tracing::warn!("older generator save: unedited terrain uses the current generator; saved edits remain");
                    }
                    Ok(Some(data))
                }
                Err(primary_error) => {
                    let bytes = fs::read(&backup).map_err(|error| anyhow::anyhow!(
                        "invalid primary save ({primary_error}); cannot read backup {}: {error}", backup.display()))?;
                    tracing::warn!(path = %backup.display(), "primary save was invalid; loading backup");
                    decode_save(&bytes).map(Some)
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => match fs::read(&backup) {
            Ok(bytes) => {
                tracing::warn!(path = %backup.display(), "primary save was missing; loading backup");
                decode_save(&bytes).map(Some)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        },
        Err(error) => Err(error.into()),
    }
}

fn write_world_save(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    // Recheck before writing, including files replaced externally while the server ran.
    load_save_file(path)?;
    match fs::read(path) {
        Ok(primary) if decode_save(&primary).is_err() => {
            // Retain evidence and leave the known-good backup intact through replacement.
            let quarantine = path.with_extension(format!("json.corrupt-{}", Uuid::new_v4()));
            fs::rename(path, quarantine)?;
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    atomic_write(path, bytes)
}

#[cfg(test)]
#[path = "save_tests.rs"]
mod save_tests;

fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);

    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup)?;
        }
        fs::rename(path, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        return Err(error.into());
    }
    Ok(())
}

fn recipe_definition(recipe: &str) -> Option<(Vec<(ItemKind, u8)>, ItemStack)> {
    let (ingredients, kind, count) = match recipe.to_ascii_lowercase().as_str() {
        "planks" | "oak_planks" => (vec![(ItemKind::OakLog, 1)], ItemKind::OakPlanks, 4),
        "sticks" | "stick" => (vec![(ItemKind::OakPlanks, 2)], ItemKind::Stick, 4),
        "crafting_table" | "table" => (vec![(ItemKind::OakPlanks, 4)], ItemKind::CraftingTable, 1),
        "wooden_pickaxe" | "pickaxe" => (
            vec![(ItemKind::OakPlanks, 3), (ItemKind::Stick, 2)],
            ItemKind::WoodenPickaxe,
            1,
        ),
        "wooden_axe" | "axe" => (
            vec![(ItemKind::OakPlanks, 3), (ItemKind::Stick, 2)],
            ItemKind::WoodenAxe,
            1,
        ),
        "wooden_shovel" | "shovel" => (
            vec![(ItemKind::OakPlanks, 1), (ItemKind::Stick, 2)],
            ItemKind::WoodenShovel,
            1,
        ),
        "wooden_sword" | "sword" => (
            vec![(ItemKind::OakPlanks, 2), (ItemKind::Stick, 1)],
            ItemKind::WoodenSword,
            1,
        ),
        "shield" => (
            vec![(ItemKind::OakPlanks, 6), (ItemKind::IronIngot, 1)],
            ItemKind::Shield,
            1,
        ),
        "flint_and_steel" | "flintsteel" => (
            vec![(ItemKind::IronIngot, 1), (ItemKind::Flint, 1)],
            ItemKind::FlintAndSteel,
            1,
        ),
        _ => return None,
    };
    Some((
        ingredients,
        ItemStack {
            kind,
            count,
            damage: 0,
        },
    ))
}

fn max_stack_size(kind: ItemKind) -> u8 {
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
        | ItemKind::Shield
        | ItemKind::IronHelmet
        | ItemKind::IronChestplate
        | ItemKind::IronLeggings
        | ItemKind::IronBoots
        | ItemKind::DiamondHelmet
        | ItemKind::DiamondChestplate
        | ItemKind::DiamondLeggings
        | ItemKind::DiamondBoots
        | ItemKind::FlintAndSteel
        | ItemKind::MilkBucket => 1,
        ItemKind::Bucket => 16,
        _ => 64,
    }
}

fn smelting_result(kind: ItemKind) -> Option<ItemKind> {
    match kind {
        ItemKind::RawIron => Some(ItemKind::IronIngot),
        ItemKind::RawCopper => Some(ItemKind::CopperIngot),
        ItemKind::RawGold => Some(ItemKind::GoldIngot),
        ItemKind::Cobblestone => Some(ItemKind::Stone),
        ItemKind::RawBeef => Some(ItemKind::CookedBeef),
        ItemKind::Porkchop => Some(ItemKind::CookedPorkchop),
        ItemKind::OakLog | ItemKind::BirchLog | ItemKind::SpruceLog => Some(ItemKind::Charcoal),
        _ => None,
    }
}

fn fuel_burn_ticks(kind: ItemKind) -> u16 {
    match kind {
        ItemKind::Coal | ItemKind::Charcoal => 1_600,
        ItemKind::OakLog | ItemKind::BirchLog | ItemKind::SpruceLog | ItemKind::OakPlanks => 300,
        ItemKind::Stick => 100,
        _ => 0,
    }
}

fn can_accept(slot: Option<ItemStack>, kind: ItemKind) -> bool {
    slot.map_or(true, |stack| {
        stack.kind == kind && stack.damage == 0 && stack.count < 64
    })
}

fn take_one(slot: &mut Option<ItemStack>) {
    if let Some(stack) = slot {
        stack.count = stack.count.saturating_sub(1);
        if stack.count == 0 {
            *slot = None;
        }
    }
}

fn save_stack(stack: ItemStack) -> SavedStack {
    SavedStack {
        kind: stack.kind.as_str().into(),
        count: stack.count,
        damage: stack.damage,
    }
}

fn load_stack(stack: SavedStack) -> Option<ItemStack> {
    Some(ItemStack {
        kind: parse_item_kind(&stack.kind)?,
        count: stack.count,
        damage: stack.damage,
    })
}

#[derive(Clone, Copy)]
struct ClickStackResult {
    slot: Option<ItemStack>,
    slot_sharpness: u8,
    cursor: InventoryCursor,
}

fn click_stack(
    mut slot: Option<ItemStack>,
    mut slot_sharpness: u8,
    mut cursor: InventoryCursor,
    right_click: bool,
    accepts: impl Fn(ItemKind) -> bool,
) -> ClickStackResult {
    match (slot, cursor.stack) {
        (Some(mut held), None) => {
            let taken = if right_click {
                held.count.div_ceil(2)
            } else {
                held.count
            };
            cursor.stack = Some(ItemStack {
                count: taken,
                ..held
            });
            cursor.sharpness_level = slot_sharpness;
            held.count -= taken;
            slot = (held.count > 0).then_some(held);
            if slot.is_none() {
                slot_sharpness = 0;
            }
        }
        (None, Some(mut carried)) if accepts(carried.kind) => {
            let moved = if right_click { 1 } else { carried.count };
            let moved = moved.min(max_stack_size(carried.kind));
            slot = Some(ItemStack {
                count: moved,
                ..carried
            });
            slot_sharpness = cursor.sharpness_level;
            carried.count -= moved;
            cursor.stack = (carried.count > 0).then_some(carried);
            if cursor.stack.is_none() {
                cursor.sharpness_level = 0;
            }
        }
        (Some(mut held), Some(mut carried))
            if held.kind == carried.kind
                && held.damage == carried.damage
                && slot_sharpness == cursor.sharpness_level
                && accepts(carried.kind) =>
        {
            let space = max_stack_size(held.kind).saturating_sub(held.count);
            let moved = if right_click { 1 } else { carried.count }.min(space);
            held.count += moved;
            carried.count -= moved;
            slot = Some(held);
            cursor.stack = (carried.count > 0).then_some(carried);
            if cursor.stack.is_none() {
                cursor.sharpness_level = 0;
            }
        }
        (Some(held), Some(carried))
            if accepts(carried.kind) && carried.count <= max_stack_size(carried.kind) =>
        {
            slot = Some(carried);
            std::mem::swap(&mut slot_sharpness, &mut cursor.sharpness_level);
            cursor.stack = Some(held);
        }
        _ => {}
    }
    ClickStackResult {
        slot,
        slot_sharpness,
        cursor,
    }
}

fn armor_slot_accepts(slot: usize, kind: ItemKind) -> bool {
    matches!(
        (slot, kind),
        (0, ItemKind::IronBoots)
            | (1, ItemKind::IronLeggings)
            | (2, ItemKind::IronChestplate)
            | (3, ItemKind::IronHelmet)
            | (0, ItemKind::DiamondBoots)
            | (1, ItemKind::DiamondLeggings)
            | (2, ItemKind::DiamondChestplate)
            | (3, ItemKind::DiamondHelmet)
    )
}

fn add_stack(inventory: &mut PlayerInventory, mut incoming: ItemStack) -> bool {
    let maximum = max_stack_size(incoming.kind);
    for stack in inventory.slots.iter_mut().flatten() {
        if stack.kind == incoming.kind && stack.damage == incoming.damage && stack.count < maximum {
            let moved = incoming.count.min(maximum - stack.count);
            stack.count += moved;
            incoming.count -= moved;
            if incoming.count == 0 {
                return true;
            }
        }
    }
    let (slots, sharpness_levels) = (&mut inventory.slots, &mut inventory.sharpness_levels);
    for (index, slot) in slots
        .iter_mut()
        .enumerate()
        .filter(|(_, slot)| slot.is_none())
    {
        let moved = incoming.count.min(maximum);
        *slot = Some(ItemStack {
            count: moved,
            ..incoming
        });
        sharpness_levels[index] = 0;
        incoming.count -= moved;
        if incoming.count == 0 {
            return true;
        }
    }
    false
}

fn block_kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::Air => "air",
        BlockKind::Water => "water",
        BlockKind::Lava => "lava",
        BlockKind::Bedrock => "bedrock",
        BlockKind::Stone => "stone",
        BlockKind::CoalOre => "coal_ore",
        BlockKind::IronOre => "iron_ore",
        BlockKind::CopperOre => "copper_ore",
        BlockKind::GoldOre => "gold_ore",
        BlockKind::RedstoneOre => "redstone_ore",
        BlockKind::LapisOre => "lapis_ore",
        BlockKind::DiamondOre => "diamond_ore",
        BlockKind::Cobblestone => "cobblestone",
        BlockKind::Dirt => "dirt",
        BlockKind::Grass => "grass",
        BlockKind::ShortGrass => "short_grass",
        BlockKind::Fern => "fern",
        BlockKind::DeadBush => "dead_bush",
        BlockKind::Sand => "sand",
        BlockKind::Gravel => "gravel",
        BlockKind::Sandstone => "sandstone",
        BlockKind::SnowBlock => "snow_block",
        BlockKind::Netherrack => "netherrack",
        BlockKind::SoulSand => "soul_sand",
        BlockKind::Basalt => "basalt",
        BlockKind::EndStone => "end_stone",
        BlockKind::Obsidian => "obsidian",
        BlockKind::StoneBricks => "stone_bricks",
        BlockKind::OakLog => "oak_log",
        BlockKind::OakPlanks => "oak_planks",
        BlockKind::OakLeaves => "oak_leaves",
        BlockKind::BirchLog => "birch_log",
        BlockKind::BirchLeaves => "birch_leaves",
        BlockKind::SpruceLog => "spruce_log",
        BlockKind::SpruceLeaves => "spruce_leaves",
        BlockKind::CraftingTable => "crafting_table",
        BlockKind::Furnace => "furnace",
        BlockKind::Chest => "chest",
        BlockKind::NetherPortal => "nether_portal",
        BlockKind::EndPortal => "end_portal",
    }
}

fn parse_block_kind(value: &str) -> Option<BlockKind> {
    match value {
        "air" => Some(BlockKind::Air),
        "water" => Some(BlockKind::Water),
        "lava" => Some(BlockKind::Lava),
        "bedrock" => Some(BlockKind::Bedrock),
        "stone" => Some(BlockKind::Stone),
        "coal_ore" => Some(BlockKind::CoalOre),
        "iron_ore" => Some(BlockKind::IronOre),
        "copper_ore" => Some(BlockKind::CopperOre),
        "gold_ore" => Some(BlockKind::GoldOre),
        "redstone_ore" => Some(BlockKind::RedstoneOre),
        "lapis_ore" => Some(BlockKind::LapisOre),
        "diamond_ore" => Some(BlockKind::DiamondOre),
        "cobblestone" => Some(BlockKind::Cobblestone),
        "dirt" => Some(BlockKind::Dirt),
        "grass" => Some(BlockKind::Grass),
        "short_grass" => Some(BlockKind::ShortGrass),
        "fern" => Some(BlockKind::Fern),
        "dead_bush" => Some(BlockKind::DeadBush),
        "sand" => Some(BlockKind::Sand),
        "gravel" => Some(BlockKind::Gravel),
        "sandstone" => Some(BlockKind::Sandstone),
        "snow_block" => Some(BlockKind::SnowBlock),
        "netherrack" => Some(BlockKind::Netherrack),
        "soul_sand" => Some(BlockKind::SoulSand),
        "basalt" => Some(BlockKind::Basalt),
        "end_stone" => Some(BlockKind::EndStone),
        "obsidian" => Some(BlockKind::Obsidian),
        "stone_bricks" => Some(BlockKind::StoneBricks),
        "oak_log" => Some(BlockKind::OakLog),
        "oak_planks" => Some(BlockKind::OakPlanks),
        "oak_leaves" => Some(BlockKind::OakLeaves),
        "birch_log" => Some(BlockKind::BirchLog),
        "birch_leaves" => Some(BlockKind::BirchLeaves),
        "spruce_log" => Some(BlockKind::SpruceLog),
        "spruce_leaves" => Some(BlockKind::SpruceLeaves),
        "crafting_table" => Some(BlockKind::CraftingTable),
        "furnace" => Some(BlockKind::Furnace),
        "chest" => Some(BlockKind::Chest),
        "nether_portal" => Some(BlockKind::NetherPortal),
        "end_portal" => Some(BlockKind::EndPortal),
        _ => None,
    }
}

fn parse_item_kind(value: &str) -> Option<ItemKind> {
    match value {
        "stone" => Some(ItemKind::Stone),
        "cobblestone" => Some(ItemKind::Cobblestone),
        "dirt" => Some(ItemKind::Dirt),
        "sand" => Some(ItemKind::Sand),
        "gravel" => Some(ItemKind::Gravel),
        "sandstone" => Some(ItemKind::Sandstone),
        "snow_block" => Some(ItemKind::SnowBlock),
        "netherrack" => Some(ItemKind::Netherrack),
        "soul_sand" => Some(ItemKind::SoulSand),
        "basalt" => Some(ItemKind::Basalt),
        "end_stone" => Some(ItemKind::EndStone),
        "obsidian" => Some(ItemKind::Obsidian),
        "stone_bricks" => Some(ItemKind::StoneBricks),
        "oak_log" => Some(ItemKind::OakLog),
        "birch_log" => Some(ItemKind::BirchLog),
        "spruce_log" => Some(ItemKind::SpruceLog),
        "oak_planks" => Some(ItemKind::OakPlanks),
        "stick" => Some(ItemKind::Stick),
        "apple" => Some(ItemKind::Apple),
        "crafting_table" => Some(ItemKind::CraftingTable),
        "furnace" => Some(ItemKind::Furnace),
        "chest" => Some(ItemKind::Chest),
        "iron_ingot" => Some(ItemKind::IronIngot),
        "copper_ingot" => Some(ItemKind::CopperIngot),
        "gold_ingot" => Some(ItemKind::GoldIngot),
        "flint" => Some(ItemKind::Flint),
        "flint_and_steel" | "flintsteel" => Some(ItemKind::FlintAndSteel),
        "bucket" => Some(ItemKind::Bucket),
        "milk_bucket" => Some(ItemKind::MilkBucket),
        "wooden_pickaxe" => Some(ItemKind::WoodenPickaxe),
        "wooden_axe" => Some(ItemKind::WoodenAxe),
        "wooden_shovel" => Some(ItemKind::WoodenShovel),
        "wooden_sword" => Some(ItemKind::WoodenSword),
        "stone_pickaxe" => Some(ItemKind::StonePickaxe),
        "stone_axe" => Some(ItemKind::StoneAxe),
        "stone_shovel" => Some(ItemKind::StoneShovel),
        "stone_sword" => Some(ItemKind::StoneSword),
        "iron_pickaxe" => Some(ItemKind::IronPickaxe),
        "iron_axe" => Some(ItemKind::IronAxe),
        "iron_shovel" => Some(ItemKind::IronShovel),
        "iron_sword" => Some(ItemKind::IronSword),
        "diamond_pickaxe" => Some(ItemKind::DiamondPickaxe),
        "diamond_axe" => Some(ItemKind::DiamondAxe),
        "diamond_shovel" => Some(ItemKind::DiamondShovel),
        "diamond_sword" => Some(ItemKind::DiamondSword),
        "shield" => Some(ItemKind::Shield),
        "beef" => Some(ItemKind::RawBeef),
        "porkchop" => Some(ItemKind::Porkchop),
        "cooked_beef" => Some(ItemKind::CookedBeef),
        "cooked_porkchop" => Some(ItemKind::CookedPorkchop),
        "rotten_flesh" => Some(ItemKind::RottenFlesh),
        "coal" => Some(ItemKind::Coal),
        "charcoal" => Some(ItemKind::Charcoal),
        "raw_iron" => Some(ItemKind::RawIron),
        "raw_copper" => Some(ItemKind::RawCopper),
        "raw_gold" => Some(ItemKind::RawGold),
        "redstone" => Some(ItemKind::Redstone),
        "lapis_lazuli" => Some(ItemKind::LapisLazuli),
        "diamond" => Some(ItemKind::Diamond),
        "iron_helmet" => Some(ItemKind::IronHelmet),
        "iron_chestplate" => Some(ItemKind::IronChestplate),
        "iron_leggings" => Some(ItemKind::IronLeggings),
        "iron_boots" => Some(ItemKind::IronBoots),
        "diamond_helmet" => Some(ItemKind::DiamondHelmet),
        "diamond_chestplate" => Some(ItemKind::DiamondChestplate),
        "diamond_leggings" => Some(ItemKind::DiamondLeggings),
        "diamond_boots" => Some(ItemKind::DiamondBoots),
        _ => None,
    }
}

fn parse_status_effect_kind(value: &str) -> Option<StatusEffectKind> {
    match value.to_ascii_lowercase().as_str() {
        "speed" => Some(StatusEffectKind::Speed),
        "slowness" => Some(StatusEffectKind::Slowness),
        "strength" => Some(StatusEffectKind::Strength),
        "regeneration" | "regen" => Some(StatusEffectKind::Regeneration),
        "resistance" => Some(StatusEffectKind::Resistance),
        "hunger" => Some(StatusEffectKind::Hunger),
        "poison" => Some(StatusEffectKind::Poison),
        _ => None,
    }
}

fn equipment_max_damage(kind: ItemKind) -> u16 {
    match kind {
        ItemKind::WoodenPickaxe
        | ItemKind::WoodenAxe
        | ItemKind::WoodenShovel
        | ItemKind::WoodenSword => 59,
        ItemKind::StonePickaxe
        | ItemKind::StoneAxe
        | ItemKind::StoneShovel
        | ItemKind::StoneSword => 131,
        ItemKind::IronPickaxe | ItemKind::IronAxe | ItemKind::IronShovel | ItemKind::IronSword => {
            250
        }
        ItemKind::DiamondPickaxe
        | ItemKind::DiamondAxe
        | ItemKind::DiamondShovel
        | ItemKind::DiamondSword => 1_561,
        ItemKind::Shield => 336,
        ItemKind::IronHelmet => 165,
        ItemKind::IronChestplate => 240,
        ItemKind::IronLeggings => 225,
        ItemKind::IronBoots => 195,
        ItemKind::DiamondHelmet => 363,
        ItemKind::DiamondChestplate => 528,
        ItemKind::DiamondLeggings => 495,
        ItemKind::DiamondBoots => 429,
        ItemKind::FlintAndSteel => 64,
        _ => u16::MAX,
    }
}

fn parse_dimension(value: &str) -> DimensionKind {
    match value {
        "minecraft:the_nether" => DimensionKind::Nether,
        "minecraft:the_end" => DimensionKind::End,
        _ => DimensionKind::Overworld,
    }
}

impl ServerState {
    fn update_world_player_counts(&self) {
        let players = self
            .players
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let mut worlds = self
            .worlds
            .write()
            .unwrap_or_else(|error| error.into_inner());
        for world in &mut *worlds {
            world.player_count = players
                .iter()
                .filter(|player| player.world == world.name)
                .count();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_player(id: Uuid, name: &str) -> PlayerSnapshot {
        PlayerSnapshot {
            id,
            name: name.into(),
            world: "world".into(),
            position: BlockPosition { x: 0, y: 65, z: 0 },
            game_mode: carbon_api::GameMode::Survival,
        }
    }

    #[test]
    fn status_effects_tick_expire_and_modify_vitals() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(test_player(id, "EffectTest")));
        assert!(state.damage_player(id, 5.0));
        assert!(state.apply_status_effect(id, StatusEffectKind::Regeneration, 4, 3));
        assert!(state.apply_status_effect(id, StatusEffectKind::Hunger, 4, 6));
        for _ in 0..5 {
            state.advance_tick();
        }
        let vitals = state.vitals(id).unwrap();
        assert!(vitals.health > 15.0);
        assert!(vitals.food < 20);
        assert!(state
            .status_effects(id)
            .iter()
            .all(|effect| { effect.kind != StatusEffectKind::Regeneration }));
        state.advance_tick();
        assert!(state.status_effects(id).is_empty());
    }

    #[test]
    fn nether_lava_contact_deals_periodic_environmental_damage() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 42, shutdown);
        let id = Uuid::new_v4();
        let position = BlockPosition {
            x: 40,
            y: 70,
            z: 40,
        };
        let mut player = test_player(id, "LavaTester");
        player.world = DimensionKind::Nether.name().into();
        player.position = position;
        assert!(state.add_player(player));
        assert!(state.set_dimension_block(DimensionKind::Nether, position, BlockKind::Lava));
        for _ in 0..9 {
            state.advance_tick();
        }
        assert_eq!(state.vitals(id).unwrap().health, 20.0);
        state.advance_tick();
        assert_eq!(state.vitals(id).unwrap().health, 18.0);
        assert!(state.set_dimension_block(DimensionKind::Nether, position, BlockKind::Air));
        for _ in 0..10 {
            state.advance_tick();
        }
        assert_eq!(state.vitals(id).unwrap().health, 17.0);
    }

    #[test]
    fn cow_milk_conversion_is_exact_and_milk_cures_all_effects() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(test_player(id, "MilkTest")));
        assert!(state.give_item(id, ItemKind::Bucket, 2));
        assert!(state.apply_status_effect(id, StatusEffectKind::Poison, 0, 200));
        assert!(state.apply_status_effect(id, StatusEffectKind::Slowness, 1, 200));

        assert!(state.fill_milk_bucket(id, 0));
        let inventory = state.inventory(id).unwrap();
        assert_eq!(inventory.slots[0].unwrap().kind, ItemKind::Bucket);
        assert_eq!(inventory.slots[0].unwrap().count, 1);
        let milk_slot = inventory
            .slots
            .iter()
            .position(|slot| slot.is_some_and(|stack| stack.kind == ItemKind::MilkBucket))
            .unwrap();
        assert!(state.consume_milk(id, milk_slot));
        assert!(state.status_effects(id).is_empty());
        let inventory = state.inventory(id).unwrap();
        assert_eq!(
            inventory
                .slots
                .iter()
                .flatten()
                .filter(|stack| stack.kind == ItemKind::Bucket)
                .map(|stack| u16::from(stack.count))
                .sum::<u16>(),
            2
        );
        assert!(inventory
            .slots
            .iter()
            .flatten()
            .all(|stack| stack.kind != ItemKind::MilkBucket));
    }

    #[test]
    fn resistance_reduces_combat_damage_and_effects_survive_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-effects-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let (shutdown, _) = watch::channel(false);
        let state =
            ServerState::with_operator_file("world".into(), 0, shutdown.clone(), path.clone())
                .unwrap();
        let id = Uuid::new_v4();
        assert!(state.add_player(test_player(id, "DurableEffect")));
        assert!(state.apply_status_effect(id, StatusEffectKind::Resistance, 1, 400));
        assert!(state.damage_player_combat(id, 10.0));
        assert_eq!(state.vitals(id).unwrap().health, 14.0);
        state.save().unwrap();

        let reloaded = ServerState::with_operator_file("world".into(), 0, shutdown, path).unwrap();
        let effect = reloaded
            .status_effects(id)
            .into_iter()
            .find(|effect| effect.kind == StatusEffectKind::Resistance)
            .unwrap();
        assert_eq!((effect.amplifier, effect.remaining_ticks), (1, 400));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn chest_contents_and_item_metadata_survive_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-chest-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let (shutdown, _) = watch::channel(false);
        let state =
            ServerState::with_operator_file("world".into(), 0, shutdown.clone(), path.clone())
                .unwrap();
        let position = BlockPosition { x: 4, y: 65, z: 4 };
        assert!(state.set_block(position, BlockKind::Chest));
        let cursor = state
            .click_chest_slot(
                DimensionKind::Overworld,
                position,
                8,
                InventoryCursor {
                    stack: Some(ItemStack {
                        kind: ItemKind::WoodenSword,
                        count: 1,
                        damage: 7,
                    }),
                    sharpness_level: 3,
                },
                false,
            )
            .unwrap();
        assert_eq!(cursor, InventoryCursor::default());
        state.save().unwrap();

        let reloaded = ServerState::with_operator_file("world".into(), 0, shutdown, path).unwrap();
        let chest = reloaded.chest(DimensionKind::Overworld, position).unwrap();
        assert_eq!(chest.slots[8].unwrap().damage, 7);
        assert_eq!(chest.sharpness_levels[8], 3);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn generated_structure_loot_is_saved_and_does_not_refill_after_replacement() {
        let directory =
            std::env::temp_dir().join(format!("carbon-structure-loot-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let state = ServerState::with_operator_file(
            "world".into(),
            42,
            watch::channel(false).0,
            path.clone(),
        )
        .unwrap();
        let chest_position = (-24..=24)
            .flat_map(|z| (-24..=24).map(move |x| ChunkPosition { x, z }))
            .find_map(|chunk| {
                state.ensure_dimension_chunk_surface(DimensionKind::Overworld, chunk);
                state
                    .dimension_chunk_blocks(DimensionKind::Overworld, chunk)
                    .into_iter()
                    .find(|placement| placement.kind == BlockKind::Chest)
                    .map(|placement| placement.position)
            })
            .expect("initial world contains a generated structure chest");
        let initial = state
            .chest(DimensionKind::Overworld, chest_position)
            .unwrap();
        assert_eq!(initial.slots.iter().flatten().count(), 3);
        let claimed_slot = initial.slots.iter().position(Option::is_some).unwrap() as u8;
        let cursor = state
            .click_chest_slot(
                DimensionKind::Overworld,
                chest_position,
                claimed_slot,
                InventoryCursor::default(),
                false,
            )
            .unwrap();
        assert!(cursor.stack.is_some());
        state.save().unwrap();

        let reloaded =
            ServerState::with_operator_file("world".into(), 42, watch::channel(false).0, path)
                .unwrap();
        let persisted = reloaded
            .chest(DimensionKind::Overworld, chest_position)
            .unwrap();
        assert_eq!(persisted.slots[usize::from(claimed_slot)], None);
        assert_eq!(persisted.slots.iter().flatten().count(), 2);
        assert_eq!(
            reloaded
                .take_chest_contents(DimensionKind::Overworld, chest_position)
                .len(),
            2
        );
        assert!(reloaded.set_dimension_block(
            DimensionKind::Overworld,
            chest_position,
            BlockKind::Air
        ));
        assert!(reloaded.set_dimension_block(
            DimensionKind::Overworld,
            chest_position,
            BlockKind::Chest
        ));
        assert_eq!(
            reloaded
                .chest(DimensionKind::Overworld, chest_position)
                .unwrap()
                .slots
                .iter()
                .flatten()
                .count(),
            0
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn furnace_smelt_is_authoritative_and_exact() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let position = BlockPosition { x: 2, y: 65, z: 3 };
        assert!(state.set_block(position, BlockKind::Furnace));
        let cursor = state
            .click_furnace_slot(
                DimensionKind::Overworld,
                position,
                FurnaceSlot::Input,
                InventoryCursor {
                    stack: Some(ItemStack {
                        kind: ItemKind::RawIron,
                        count: 2,
                        damage: 0,
                    }),
                    sharpness_level: 0,
                },
                false,
            )
            .unwrap();
        assert_eq!(cursor.stack, None);
        state.click_furnace_slot(
            DimensionKind::Overworld,
            position,
            FurnaceSlot::Fuel,
            InventoryCursor {
                stack: Some(ItemStack {
                    kind: ItemKind::Coal,
                    count: 1,
                    damage: 0,
                }),
                sharpness_level: 0,
            },
            false,
        );
        for _ in 0..400 {
            state.advance_tick();
        }
        let furnace = state.furnace(DimensionKind::Overworld, position).unwrap();
        assert_eq!(furnace.input, None);
        assert_eq!(
            furnace.output.unwrap(),
            ItemStack {
                kind: ItemKind::IronIngot,
                count: 2,
                damage: 0
            }
        );
        assert_eq!(furnace.burn_remaining, 1_200);
        assert_eq!(
            state
                .take_furnace_contents(DimensionKind::Overworld, position)
                .len(),
            1
        );
    }

    #[test]
    fn wooden_fuel_cooks_food_and_cooked_food_restores_full_nutrition() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let position = BlockPosition { x: 6, y: 65, z: 6 };
        assert!(state.set_block(position, BlockKind::Furnace));
        state.click_furnace_slot(
            DimensionKind::Overworld,
            position,
            FurnaceSlot::Input,
            InventoryCursor {
                stack: Some(ItemStack {
                    kind: ItemKind::RawBeef,
                    count: 1,
                    damage: 0,
                }),
                sharpness_level: 0,
            },
            false,
        );
        state.click_furnace_slot(
            DimensionKind::Overworld,
            position,
            FurnaceSlot::Fuel,
            InventoryCursor {
                stack: Some(ItemStack {
                    kind: ItemKind::OakPlanks,
                    count: 1,
                    damage: 0,
                }),
                sharpness_level: 0,
            },
            false,
        );
        for _ in 0..200 {
            state.advance_tick();
        }
        let furnace = state.furnace(DimensionKind::Overworld, position).unwrap();
        assert_eq!(furnace.burn_remaining, 100);
        assert_eq!(furnace.output.unwrap().kind, ItemKind::CookedBeef);
        assert_eq!(smelting_result(ItemKind::OakLog), Some(ItemKind::Charcoal));
        assert_eq!(fuel_burn_ticks(ItemKind::Stick), 100);

        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Cook".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        {
            let mut vitals = state.vitals.write().unwrap();
            vitals.get_mut(&id).unwrap().food = 10;
            vitals.get_mut(&id).unwrap().saturation = 0.0;
        }
        assert!(state.give_item(id, ItemKind::CookedBeef, 1));
        assert!(state.consume_food(id, 0));
        let vitals = state.vitals(id).unwrap();
        assert_eq!(vitals.food, 18);
        assert!((vitals.saturation - 12.8).abs() < 0.001);
    }

    #[test]
    fn operator_file_survives_a_state_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-ops-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let (shutdown, _) = watch::channel(false);
        let state =
            ServerState::with_operator_file("world".into(), 0, shutdown.clone(), path.clone())
                .unwrap();
        assert!(state.set_operator("CarbonTest", true).unwrap());

        let reloaded = ServerState::with_operator_file("world".into(), 0, shutdown, path).unwrap();
        assert!(reloaded.is_operator("carbontest"));
        assert_eq!(reloaded.operators(), vec!["CarbonTest"]);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn negative_permission_rules_override_grants_and_respect_boundaries() {
        let state = ServerState::new("world".into(), 0, watch::channel(false).0);
        for node in [
            "*",
            "carbon.command.say",
            "!CARBON.COMMAND.SAY",
            "!carbon.world.*",
        ] {
            assert!(state.grant_permission("Helper", node).unwrap());
        }
        assert!(!state.has_permission("HELPER", "carbon.command.say"));
        assert!(state.has_permission("helper", "carbon.command.saymore"));
        assert!(!state.has_permission("helper", "carbon.world.build"));
        assert!(!state.has_permission("helper", "carbon.world.region.build"));
        assert!(state.has_permission("helper", "carbon.world"));
        assert!(state.has_permission("helper", "carbon.worlds.build"));
        assert!(!state.has_permission("helper", "!carbon.command.say"));
        assert!(state
            .revoke_permission("HELPER", "!carbon.command.say")
            .unwrap());
        assert!(state.has_permission("helper", "carbon.command.say"));
        assert!(!state
            .revoke_permission("Helper", "!carbon.command.say")
            .unwrap());
        assert!(state.grant_permission("Helper", "!*").unwrap());
        assert!(!state.has_permission("Helper", "carbon.command.say"));
        assert!(state.set_operator("Helper", true).unwrap());
        assert!(state.has_permission("helper", "carbon.command.say"));
        assert!(state.set_operator("Helper", false).unwrap());
        assert!(!state.has_permission("helper", "carbon.command.say"));

        assert!(state.grant_permission("Builder", "carbon.world.*").unwrap());
        assert!(state.has_permission("Builder", "carbon.world.region.build"));
        assert!(!state.has_permission("Builder", "carbon.worlds.build"));
        assert!(!state.has_permission("Builder", "carbon.world"));
        assert!(state
            .grant_permission("Builder", "!carbon.command.say")
            .unwrap());
        assert!(!state.has_permission("Builder", "carbon.command.stop"));
    }

    #[test]
    fn negative_permission_validation_rejects_malformed_rules() {
        for invalid in [
            "!",
            "!!*",
            "! *",
            "!bad..node",
            "!carbon.*.say",
            "!carbon.!say",
            "!carbon.say.",
        ] {
            assert_eq!(normalize_permission(invalid), None, "{invalid}");
        }
        assert_eq!(normalize_permission(&format!("!{}", "a".repeat(128))), None);
        for valid in [
            "!*",
            "!carbon.command.*",
            "!carbon.command.say",
            "carbon.some-node",
            "-legacy.node",
        ] {
            assert_eq!(normalize_permission(valid).as_deref(), Some(valid));
        }
        assert_eq!(
            normalize_permission(" !CARBON.COMMAND.SAY ").as_deref(),
            Some("!carbon.command.say")
        );
    }

    #[test]
    fn unsupported_plants_fan_out_and_stay_removed_after_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-plants-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let state = ServerState::with_operator_file(
            "world".into(),
            42,
            watch::channel(false).0,
            path.clone(),
        )
        .unwrap();
        let ground = BlockPosition {
            x: -8,
            y: 100,
            z: -8,
        };
        let plant = BlockPosition { y: 101, ..ground };
        for dimension in [
            DimensionKind::Overworld,
            DimensionKind::Nether,
            DimensionKind::End,
        ] {
            assert!(state.set_dimension_block(dimension, ground, BlockKind::Dirt));
            assert!(state.set_dimension_block(dimension, plant, BlockKind::Fern));
            assert!(state.set_dimension_block(dimension, ground, BlockKind::Stone));
            assert_eq!(state.dimension_block_at(dimension, plant), BlockKind::Air);
            assert!(state
                .block_changes_since(0)
                .iter()
                .any(|change| change.dimension == dimension
                    && change.position == plant
                    && change.kind == BlockKind::Air));
        }
        let retained = BlockPosition { x: -7, ..plant };
        state.set_block(BlockPosition { y: 100, ..retained }, BlockKind::Sand);
        state.set_block(retained, BlockKind::DeadBush);
        state.save().unwrap();
        let loaded =
            ServerState::with_operator_file("world".into(), 42, watch::channel(false).0, path)
                .unwrap();
        assert_eq!(loaded.block_at(retained), BlockKind::DeadBush);
        for dimension in [
            DimensionKind::Overworld,
            DimensionKind::Nether,
            DimensionKind::End,
        ] {
            assert_eq!(loaded.dimension_block_at(dimension, plant), BlockKind::Air);
        }
        for kind in [
            BlockKind::ShortGrass,
            BlockKind::Fern,
            BlockKind::DeadBush,
            BlockKind::Lava,
        ] {
            assert_eq!(parse_block_kind(block_kind_name(kind)), Some(kind));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nether_arrivals_stay_below_roof_and_ceiling_edits_survive_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-nether-roof-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let state = ServerState::with_operator_file(
            "world".into(),
            42,
            watch::channel(false).0,
            path.clone(),
        )
        .unwrap();
        let id = Uuid::new_v4();
        let mut player = test_player(id, "CavernTest");
        player.position = BlockPosition {
            x: 80,
            y: 65,
            z: 80,
        };
        assert!(state.add_player(player));
        assert!(state.change_player_dimension(id, DimensionKind::Nether));
        let position = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap()
            .position;
        assert_eq!((position.x, position.z), (10, 10));
        assert!(position.y < 83);
        assert_eq!(
            state.dimension_block_at(DimensionKind::Nether, position),
            BlockKind::Air
        );
        assert_eq!(
            state.dimension_block_at(
                DimensionKind::Nether,
                BlockPosition {
                    y: position.y + 1,
                    ..position
                }
            ),
            BlockKind::Air
        );
        assert_ne!(
            state.dimension_block_at(
                DimensionKind::Nether,
                BlockPosition {
                    y: position.y - 1,
                    ..position
                }
            ),
            BlockKind::Air
        );
        let hole = BlockPosition {
            x: 10,
            y: 120,
            z: 10,
        };
        assert_ne!(
            state.dimension_block_at(DimensionKind::Nether, hole),
            BlockKind::Air
        );
        assert!(state.set_dimension_block(DimensionKind::Nether, hole, BlockKind::Air));
        state.save().unwrap();
        let loaded =
            ServerState::with_operator_file("world".into(), 42, watch::channel(false).0, path)
                .unwrap();
        assert_eq!(
            loaded.dimension_block_at(DimensionKind::Nether, hole),
            BlockKind::Air
        );
        assert_eq!(
            loaded.dimension_block_at(DimensionKind::Nether, BlockPosition { y: 127, ..hole }),
            BlockKind::Bedrock
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn end_entry_over_void_uses_the_central_island() {
        let state = ServerState::new("world".into(), 42, watch::channel(false).0);
        let id = Uuid::new_v4();
        let mut player = test_player(id, "EndTest");
        player.position = BlockPosition {
            x: 160,
            y: 65,
            z: 0,
        };
        assert!(state.add_player(player));
        assert!(state.change_player_dimension(id, DimensionKind::End));
        let position = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap()
            .position;
        assert_eq!(
            position,
            BlockPosition {
                x: -8,
                y: 65,
                z: -8
            }
        );
        assert_eq!(
            state.dimension_block_at(DimensionKind::End, position),
            BlockKind::Air
        );
        assert_eq!(
            state.dimension_block_at(DimensionKind::End, BlockPosition { y: 64, ..position }),
            BlockKind::EndStone
        );
        assert!(state.change_player_dimension(id, DimensionKind::Overworld));
        let returned = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap()
            .position;
        assert_eq!(returned, BlockPosition { x: 0, y: 65, z: 0 });
        assert_ne!(state.block_at(returned), BlockKind::EndPortal);
        assert_eq!(
            state.block_at(BlockPosition { x: 8, y: 65, z: -8 }),
            BlockKind::EndPortal
        );
    }

    #[test]
    fn end_portal_edits_and_names_survive_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-end-portal-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let state = ServerState::with_operator_file(
            "world".into(),
            42,
            watch::channel(false).0,
            path.clone(),
        )
        .unwrap();
        let portal = BlockPosition { x: 8, y: 65, z: -8 };
        assert_eq!(state.block_at(portal), BlockKind::EndPortal);
        assert!(state.set_block(portal, BlockKind::Air));
        state.save().unwrap();
        let loaded =
            ServerState::with_operator_file("world".into(), 42, watch::channel(false).0, path)
                .unwrap();
        assert_eq!(loaded.block_at(portal), BlockKind::Air);
        assert_eq!(block_kind_name(BlockKind::EndPortal), "end_portal");
        assert_eq!(parse_block_kind("end_portal"), Some(BlockKind::EndPortal));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn negative_permissions_survive_restart_and_merge_case_aliases() {
        let directory = std::env::temp_dir().join(format!("carbon-denies-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        fs::write(
            directory.join("permissions.json"),
            br#"{"Helper":["*"],"HELPER":["!carbon.command.stop"]}"#,
        )
        .unwrap();
        let state = ServerState::with_operator_file(
            "world".into(),
            0,
            watch::channel(false).0,
            path.clone(),
        )
        .unwrap();
        assert!(state.has_permission("helper", "carbon.command.say"));
        assert!(!state.has_permission("helper", "carbon.command.stop"));
        assert!(state.grant_permission("Helper", "!carbon.world.*").unwrap());
        drop(state);
        let reloaded = ServerState::with_operator_file(
            "world".into(),
            0,
            watch::channel(false).0,
            path.clone(),
        )
        .unwrap();
        assert_eq!(
            reloaded.permission_nodes("helper"),
            vec!["!carbon.command.stop", "!carbon.world.*", "*"]
        );
        assert!(!reloaded.has_permission("helper", "carbon.command.stop"));
        assert!(!reloaded.has_permission("helper", "carbon.world.build"));
        assert!(reloaded
            .revoke_permission("helper", "!carbon.command.stop")
            .unwrap());
        drop(reloaded);
        let reloaded =
            ServerState::with_operator_file("world".into(), 0, watch::channel(false).0, path)
                .unwrap();
        assert!(reloaded.has_permission("helper", "carbon.command.stop"));
        assert!(!reloaded.has_permission("helper", "carbon.world.build"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bans_and_allowlist_survive_a_state_restart_case_insensitively() {
        let directory = std::env::temp_dir().join(format!("carbon-access-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let (shutdown, _) = watch::channel(false);
        let state =
            ServerState::with_operator_file("world".into(), 0, shutdown.clone(), path.clone())
                .unwrap();
        assert!(state.set_banned("Trouble", true).unwrap());
        assert!(!state.set_banned("trouble", true).unwrap());
        let expiry = unix_now() + 3_600;
        assert!(state
            .ban_player("Temporary", "Testing timed bans", Some(expiry))
            .unwrap());
        assert!(state.set_allowlisted("Friend", true).unwrap());
        assert!(state
            .grant_permission("Helper", "carbon.command.say")
            .unwrap());
        assert!(state.grant_permission("Helper", "carbon.world.*").unwrap());
        assert!(state.grant_permission("Helper", "bad..node").is_err());

        let reloaded = ServerState::with_operator_file("world".into(), 0, shutdown, path).unwrap();
        assert!(reloaded.is_banned("TROUBLE"));
        assert!(reloaded.is_allowlisted("friend"));
        assert!(reloaded.has_permission("helper", "carbon.command.say"));
        assert!(reloaded.has_permission("HELPER", "carbon.world.build"));
        assert!(!reloaded.has_permission("helper", "carbon.command.stop"));
        assert_eq!(
            reloaded.permission_nodes("helper"),
            vec!["carbon.command.say", "carbon.world.*"]
        );
        assert_eq!(reloaded.banned_players(), vec!["Temporary", "Trouble"]);
        let temporary = reloaded.active_ban("temporary").unwrap();
        assert_eq!(temporary.reason, "Testing timed bans");
        assert_eq!(temporary.expires_at_unix, Some(expiry));
        assert_eq!(reloaded.allowlisted_players(), vec!["Friend"]);
        assert!(reloaded.set_banned("trouble", false).unwrap());
        assert!(reloaded.set_allowlisted("FRIEND", false).unwrap());
        assert!(reloaded
            .revoke_permission("helper", "carbon.command.say")
            .unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn legacy_bans_load_and_expired_structured_bans_are_ignored() {
        let directory = std::env::temp_dir().join(format!("carbon-ban-migrate-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("banned-players.json"),
            br#"["Legacy",{"name":"Expired","reason":"Old","expires_at_unix":1}]"#,
        )
        .unwrap();
        let state = ServerState::with_operator_file(
            "world".into(),
            0,
            watch::channel(false).0,
            directory.join("operators.json"),
        )
        .unwrap();
        assert_eq!(
            state.active_ban("legacy").unwrap().reason,
            "Banned by an operator."
        );
        assert!(!state.is_banned("expired"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn moderation_audit_appends_survives_restart_and_ignores_a_torn_line() {
        let directory = std::env::temp_dir().join(format!("carbon-audit-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let operator_path = directory.join("operators.json");
        let state = ServerState::with_operator_file(
            "world".into(),
            0,
            watch::channel(false).0,
            operator_path.clone(),
        )
        .unwrap();
        state
            .record_moderation("console", "ban", Some("Target"), "Banned Target.")
            .unwrap();
        let audit_path = directory.join("moderation-audit.jsonl");
        OpenOptions::new()
            .append(true)
            .open(&audit_path)
            .unwrap()
            .write_all(b"{torn\n")
            .unwrap();

        let reloaded = ServerState::with_operator_file(
            "world".into(),
            0,
            watch::channel(false).0,
            operator_path,
        )
        .unwrap();
        let records = reloaded.moderation_records(10);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].actor, "console");
        assert_eq!(records[0].action, "ban");
        assert_eq!(records[0].target.as_deref(), Some("Target"));
        assert!(reloaded
            .record_moderation("console", "bad\naction", None, "detail")
            .is_err());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn inventory_stacks_and_consumes_items() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Builder".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::OakLog, 8));
        assert!(state.give_item(id, ItemKind::OakLog, 4));
        assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().count, 12);
        assert!(state.take_item(id, 0, 2));
        assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().count, 10);
        assert!(state.give_item(id, ItemKind::Apple, 1));
        assert!(state.set_selected_slot(id, 1));
        assert_eq!(
            state.player_equipment(id).unwrap().main_hand.unwrap().kind,
            ItemKind::Apple
        );
    }

    #[test]
    fn inventory_pickup_clicks_split_stacks_without_duplication() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Clicker".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::OakLog, 5));

        let cursor = state
            .click_player_inventory_slot(
                id,
                PlayerInventorySlot::Storage(0),
                InventoryCursor::default(),
                true,
            )
            .unwrap();
        assert_eq!(cursor.stack.unwrap().count, 3);
        assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().count, 2);
        let cursor = state
            .click_player_inventory_slot(id, PlayerInventorySlot::Storage(1), cursor, true)
            .unwrap();
        assert_eq!(cursor.stack.unwrap().count, 2);
        let inventory = state.inventory(id).unwrap();
        assert_eq!(inventory.slots[0].unwrap().count, 2);
        assert_eq!(inventory.slots[1].unwrap().count, 1);
    }

    #[test]
    fn armor_clicks_enforce_slot_compatibility() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Armorer".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::IronHelmet, 1));
        let cursor = state
            .click_player_inventory_slot(
                id,
                PlayerInventorySlot::Storage(0),
                InventoryCursor::default(),
                false,
            )
            .unwrap();
        let cursor = state
            .click_player_inventory_slot(id, PlayerInventorySlot::Armor(0), cursor, false)
            .unwrap();
        assert_eq!(cursor.stack.unwrap().kind, ItemKind::IronHelmet);
        assert_eq!(state.player_equipment(id).unwrap().armor[0], None);
        let cursor = state
            .click_player_inventory_slot(id, PlayerInventorySlot::Armor(3), cursor, false)
            .unwrap();
        assert_eq!(cursor, InventoryCursor::default());
        assert_eq!(
            state.player_equipment(id).unwrap().armor[3].unwrap().kind,
            ItemKind::IronHelmet
        );
    }

    #[test]
    fn recipes_consume_exact_ingredients_and_create_exact_outputs() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Crafter".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.give_item(id, ItemKind::OakLog, 1));
        assert!(state.craft(id, "planks"));
        let inventory = state.inventory(id).unwrap();
        assert_eq!(inventory.slots[0].unwrap().kind, ItemKind::OakPlanks);
        assert_eq!(inventory.slots[0].unwrap().count, 4);
        assert!(state.craft(id, "sticks"));
        let inventory = state.inventory(id).unwrap();
        assert_eq!(inventory.slots[0].unwrap().count, 2);
        assert_eq!(inventory.slots[1].unwrap().kind, ItemKind::Stick);
        assert_eq!(inventory.slots[1].unwrap().count, 4);
    }

    #[test]
    fn eating_consumes_one_item_and_restores_bounded_hunger() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Hungry".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        {
            let mut vitals = state.vitals.write().unwrap();
            vitals.get_mut(&id).unwrap().food = 17;
            vitals.get_mut(&id).unwrap().saturation = 0.0;
        }
        assert!(state.give_item(id, ItemKind::Apple, 2));
        assert!(state.consume_food(id, 0));
        assert_eq!(state.vitals(id).unwrap().food, 20);
        assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().count, 1);
        assert!(!state.consume_food(id, 0));
        assert_eq!(state.inventory(id).unwrap().slots[0].unwrap().count, 1);
    }

    #[test]
    fn rotten_flesh_applies_a_timed_hunger_penalty() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Hungry".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        state.vitals.write().unwrap().get_mut(&id).unwrap().food = 10;
        assert!(state.give_item(id, ItemKind::RottenFlesh, 1));
        assert!(state.consume_food(id, 0));
        assert_eq!(state.vitals(id).unwrap().food, 14);
        for _ in 0..80 {
            state.advance_tick();
        }
        assert_eq!(state.vitals(id).unwrap().food, 13);
    }

    #[test]
    fn nether_travel_scales_coordinates_in_both_directions() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 42, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Traveler".into(),
            world: "world".into(),
            position: BlockPosition {
                x: 800,
                y: 70,
                z: -400,
            },
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.change_player_dimension(id, DimensionKind::Nether));
        let nether = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap();
        assert_eq!((nether.position.x, nether.position.z), (100, -50));
        assert!(state.change_player_dimension(id, DimensionKind::Overworld));
        let overworld = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap();
        assert_eq!((overworld.position.x, overworld.position.z), (800, -400));
    }

    #[test]
    fn flint_and_steel_ignites_and_frame_damage_collapses_both_orientations() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 42, shutdown);
        for (dimension, anchor, along_x) in [
            (
                DimensionKind::Overworld,
                BlockPosition {
                    x: 200,
                    y: 80,
                    z: 200,
                },
                true,
            ),
            (
                DimensionKind::Nether,
                BlockPosition {
                    x: 200,
                    y: 80,
                    z: 200,
                },
                false,
            ),
        ] {
            let at = |along: i32, y: i32| BlockPosition {
                x: anchor.x + if along_x { along } else { 0 },
                y: anchor.y + y,
                z: anchor.z + if along_x { 0 } else { along },
            };
            for y in 1..=3 {
                for along in 1..=2 {
                    state.set_dimension_block(dimension, at(along, y), BlockKind::Air);
                }
            }
            for y in 0..=4 {
                for along in 0..=3 {
                    if along == 0 || along == 3 || y == 0 || y == 4 {
                        state.set_dimension_block(dimension, at(along, y), BlockKind::Obsidian);
                    }
                }
            }
            for y in 1..=3 {
                for along in 1..=2 {
                    assert_eq!(
                        state.dimension_block_at(dimension, at(along, y)),
                        BlockKind::Air
                    );
                }
            }
            assert!(state.ignite_nether_portal(dimension, at(1, 1)));
            for y in 1..=3 {
                for along in 1..=2 {
                    assert_eq!(
                        state.dimension_block_at(dimension, at(along, y)),
                        BlockKind::NetherPortal
                    );
                }
            }
            assert!(state.set_dimension_block(dimension, anchor, BlockKind::Air));
            for y in 1..=3 {
                for along in 1..=2 {
                    assert_eq!(
                        state.dimension_block_at(dimension, at(along, y)),
                        BlockKind::Air
                    );
                }
            }
        }
    }

    #[test]
    fn world_edits_inventory_and_tool_damage_survive_restart() {
        let directory = std::env::temp_dir().join(format!("carbon-save-{}", Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("operators.json");
        let id = Uuid::new_v4();
        let position = BlockPosition { x: 4, y: 70, z: 4 };
        let (shutdown, _) = watch::channel(false);
        let state =
            ServerState::with_operator_file("world".into(), 99, shutdown.clone(), path.clone())
                .unwrap();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Saver".into(),
            world: "world".into(),
            position,
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.set_block(position, BlockKind::CraftingTable));
        let nether_position = BlockPosition { x: 8, y: 50, z: 8 };
        assert!(state.set_dimension_block(
            DimensionKind::Nether,
            nether_position,
            BlockKind::Obsidian,
        ));
        assert!(state.give_item(id, ItemKind::WoodenPickaxe, 1));
        assert!(state.damage_item(id, 0, 7));
        assert!(state.give_item(id, ItemKind::WoodenSword, 1));
        assert!(state.set_selected_slot(id, 1));
        assert!(state.enchant_selected_weapon(id, 3));
        assert!(state.set_player_armor(
            id,
            [
                Some(ItemStack {
                    kind: ItemKind::IronBoots,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::IronLeggings,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::IronChestplate,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::IronHelmet,
                    count: 1,
                    damage: 0
                }),
            ],
        ));
        assert!(state.set_offhand(
            id,
            Some(ItemStack {
                kind: ItemKind::Shield,
                count: 1,
                damage: 12,
            }),
        ));
        assert!(state.change_player_dimension(id, DimensionKind::Nether));
        let saved_position = BlockPosition {
            x: 12,
            y: 55,
            z: -9,
        };
        assert!(state.update_player_position(id, saved_position));
        state.save().unwrap();
        state.save().unwrap();

        let reloaded = ServerState::with_operator_file("world".into(), 99, shutdown, path).unwrap();
        assert_eq!(reloaded.block_at(position), BlockKind::CraftingTable);
        assert_eq!(
            reloaded.dimension_block_at(DimensionKind::Nether, nether_position),
            BlockKind::Obsidian
        );
        assert_eq!(
            reloaded.inventory(id).unwrap().slots[0],
            Some(ItemStack {
                kind: ItemKind::WoodenPickaxe,
                count: 1,
                damage: 7,
            })
        );
        assert_eq!(reloaded.inventory(id).unwrap().sharpness_levels[1], 3);
        assert_eq!(
            reloaded.player_equipment(id).unwrap().armor[3],
            Some(ItemStack {
                kind: ItemKind::IronHelmet,
                count: 1,
                damage: 0,
            })
        );
        assert_eq!(
            reloaded.player_equipment(id).unwrap().off_hand,
            Some(ItemStack {
                kind: ItemKind::Shield,
                count: 1,
                damage: 12,
            })
        );
        assert!(reloaded.add_player(PlayerSnapshot {
            id,
            name: "Saver".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        let restored = reloaded
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap();
        assert_eq!(restored.world, DimensionKind::Nether.name());
        assert_eq!(restored.position, saved_position);
        fs::write(directory.join("world-save.json"), b"{broken").unwrap();
        let recovered = ServerState::with_operator_file(
            "world".into(),
            99,
            watch::channel(false).0,
            directory.join("operators.json"),
        )
        .unwrap();
        assert_eq!(recovered.block_at(position), BlockKind::CraftingTable);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn repair_world_clears_gameplay_state_and_rebuilds_seeded_dimensions() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 42, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "ResetMe".into(),
            world: DimensionKind::Nether.name().into(),
            position: BlockPosition {
                x: 40,
                y: 70,
                z: 40
            },
            game_mode: carbon_api::GameMode::Survival,
        }));
        let edited = BlockPosition {
            x: 30,
            y: 70,
            z: 30,
        };
        assert!(state.set_block(edited, BlockKind::CraftingTable));
        assert!(state.give_item(id, ItemKind::Diamond, 4));
        assert!(state.set_dimension_block(
            DimensionKind::Nether,
            BlockPosition { x: 8, y: 70, z: 8 },
            BlockKind::Chest,
        ));

        state.repair_world().unwrap();

        assert_ne!(state.block_at(edited), BlockKind::CraftingTable);
        assert!(state
            .inventory(id)
            .unwrap()
            .slots
            .iter()
            .all(Option::is_none));
        let player = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap();
        assert_eq!(parse_dimension(&player.world), DimensionKind::Overworld);
        assert_eq!(state.block_at(player.position), BlockKind::Air);
        assert!(state
            .disconnects_since(0)
            .iter()
            .any(|disconnect| disconnect.player_id == id));
        assert_eq!(state.worlds()[0].seed, 42);
        assert_eq!(state.worlds()[0].age_ticks, 0);
    }

    #[test]
    fn damage_death_and_respawn_update_player_vitals() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Survivor".into(),
            world: "world".into(),
            position: BlockPosition {
                x: -8,
                y: 65,
                z: -9
            },
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.damage_player(id, 20.0));
        assert_eq!(state.vitals(id).unwrap().health, 0.0);
        assert!(state.set_player_sprinting(id, true));
        assert!(state.set_player_blocking(id, true));
        assert!(state.respawn_player(id));
        assert_eq!(state.vitals(id).unwrap().health, 20.0);
        assert_eq!(
            state.player_combat_state(id),
            Some(PlayerCombatState::default())
        );
        let player = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap();
        assert_eq!(parse_dimension(&player.world), DimensionKind::Overworld);
        assert_eq!(state.block_at(player.position), BlockKind::Air);
        assert_eq!(
            state.block_at(BlockPosition {
                y: player.position.y + 1,
                ..player.position
            }),
            BlockKind::Air
        );
    }

    #[test]
    fn unsafe_join_positions_are_relocated_to_clear_supported_ground() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 42, shutdown);
        let id = Uuid::new_v4();
        state.saved_locations.write().unwrap().insert(
            id,
            SavedLocation {
                dimension: "world".into(),
                x: 40,
                y: 0,
                z: 40,
            },
        );
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Buried".into(),
            world: "world".into(),
            position: BlockPosition { x: 40, y: 0, z: 40 },
            game_mode: carbon_api::GameMode::Survival,
        }));
        let position = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap()
            .position;
        assert_eq!(state.block_at(position), BlockKind::Air);
        assert_eq!(
            state.block_at(BlockPosition {
                y: position.y + 1,
                ..position
            }),
            BlockKind::Air
        );
        assert!(!state
            .block_at(BlockPosition {
                y: position.y - 1,
                ..position
            })
            .is_passable());
    }

    #[test]
    fn precise_player_transforms_preserve_fractional_position_and_rotation() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Mover".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.update_player_transform(PlayerTransform {
            id,
            position: EntityPosition {
                x: 4.75,
                y: 65.125,
                z: -2.25,
            },
            yaw: 127.5,
            pitch: -18.25,
            on_ground: false,
            revision: 0,
        }));
        let transform = state
            .player_transforms()
            .into_iter()
            .find(|transform| transform.id == id)
            .unwrap();
        assert_eq!(transform.position.x, 4.75);
        assert_eq!((transform.yaw, transform.pitch), (127.5, -18.25));
        assert!(!transform.on_ground);
        let player = state
            .players()
            .into_iter()
            .find(|player| player.id == id)
            .unwrap();
        assert_eq!(player.position, BlockPosition { x: 4, y: 65, z: -3 });
    }

    #[test]
    fn player_combat_events_are_revisioned_for_multiplayer_fanout() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Fighter".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.swing_player(id, false));
        assert!(state.damage_player(id, 4.0));
        assert!(state.damage_player(id, 16.0));
        assert!(state.critical_hit_player(id));
        let events = state.player_events_since(0);
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].kind, PlayerEventKind::SwingMainArm);
        assert_eq!(events[1].kind, PlayerEventKind::Hurt);
        assert_eq!(events[2].kind, PlayerEventKind::Died);
        assert_eq!(events[3].kind, PlayerEventKind::CriticalHit);
        assert!(state.player_events_since(events[3].revision).is_empty());
    }

    #[test]
    fn iron_armor_reduces_combat_damage_and_knockback_is_revisioned() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let attacker = Uuid::new_v4();
        let target = Uuid::new_v4();
        for (id, name, x) in [(attacker, "Attacker", 0), (target, "Target", 1)] {
            assert!(state.add_player(PlayerSnapshot {
                id,
                name: name.into(),
                world: "world".into(),
                position: BlockPosition { x, y: 65, z: 0 },
                game_mode: carbon_api::GameMode::Survival,
            }));
        }
        assert!(state.set_player_armor(
            target,
            [
                Some(ItemStack {
                    kind: ItemKind::IronBoots,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::IronLeggings,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::IronChestplate,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::IronHelmet,
                    count: 1,
                    damage: 0
                }),
            ],
        ));
        assert!(state.damage_player_combat(target, 4.0));
        assert!((state.vitals(target).unwrap().health - 18.08).abs() < 0.001);
        assert!(state.knockback_player(attacker, target, 0.4));
        let impulses = state.player_impulses_since(0);
        assert_eq!(impulses.len(), 1);
        assert_eq!(impulses[0].player_id, target);
        assert!((impulses[0].velocity.x - 0.4).abs() < 0.000_1);
        assert_eq!(impulses[0].velocity.y, 0.4);
        assert!(state.player_impulses_since(impulses[0].revision).is_empty());
    }

    #[test]
    fn diamond_armor_applies_toughness_and_verified_durability() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "DiamondDefender".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.set_player_armor(
            id,
            [
                Some(ItemStack {
                    kind: ItemKind::DiamondBoots,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::DiamondLeggings,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::DiamondChestplate,
                    count: 1,
                    damage: 0
                }),
                Some(ItemStack {
                    kind: ItemKind::DiamondHelmet,
                    count: 1,
                    damage: 0
                }),
            ],
        ));
        assert!(state.damage_player_combat(id, 10.0));
        assert!((state.vitals(id).unwrap().health - 17.0).abs() < 0.001);
        assert_eq!(equipment_max_damage(ItemKind::DiamondPickaxe), 1_561);
        assert_eq!(equipment_max_damage(ItemKind::DiamondChestplate), 528);
    }

    #[test]
    fn raised_shields_block_only_attacks_from_the_front_and_take_damage() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let attacker = Uuid::new_v4();
        let target = Uuid::new_v4();
        for (id, name, z) in [(attacker, "Attacker", 101), (target, "Target", 100)] {
            assert!(state.add_player(PlayerSnapshot {
                id,
                name: name.into(),
                world: "world".into(),
                position: BlockPosition { x: 100, y: 65, z },
                game_mode: carbon_api::GameMode::Survival,
            }));
        }
        assert!(state.set_offhand(
            target,
            Some(ItemStack {
                kind: ItemKind::Shield,
                count: 1,
                damage: 0,
            }),
        ));
        assert!(state.set_player_blocking(target, true));
        for _ in 0..5 {
            state.advance_tick();
        }
        assert!(!state.attack_player(attacker, target, 4.0));
        assert_eq!(state.vitals(target).unwrap().health, 20.0);
        assert_eq!(
            state
                .player_equipment(target)
                .unwrap()
                .off_hand
                .unwrap()
                .damage,
            5
        );

        let mut transform = state
            .player_transforms()
            .into_iter()
            .find(|transform| transform.id == target)
            .unwrap();
        transform.yaw = 180.0;
        assert!(state.update_player_transform(transform));
        assert!(state.attack_player(attacker, target, 4.0));
        assert_eq!(state.vitals(target).unwrap().health, 16.0);
    }

    #[test]
    fn disabled_shields_cannot_be_raised_until_the_cooldown_expires() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Defender".into(),
            world: "world".into(),
            position: BlockPosition {
                x: 100,
                y: 65,
                z: 100
            },
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.disable_player_shield(id, 10));
        assert!(!state.set_player_blocking(id, true));
        assert_eq!(state.player_combat_state(id).unwrap().blocking_since, None);
        for _ in 0..10 {
            state.advance_tick();
        }
        assert!(state.set_player_blocking(id, true));
        assert!(state
            .player_combat_state(id)
            .unwrap()
            .blocking_since
            .is_some());
    }

    #[test]
    fn block_changes_are_revisioned_for_multiplayer_fanout() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let position = BlockPosition {
            x: -8,
            y: 64,
            z: -8,
        };
        assert!(state.set_block(position, BlockKind::Air));
        let changes = state.block_changes_since(0);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].position, position);
        assert_eq!(changes[0].kind, BlockKind::Air);
        assert!(state.block_changes_since(changes[0].revision).is_empty());
    }

    #[test]
    fn nearby_zombies_damage_players_on_the_server_tick() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        state.add_player(PlayerSnapshot {
            id,
            name: "Target".into(),
            world: "world".into(),
            position: BlockPosition {
                x: -13,
                y: 65,
                z: -13,
            },
            game_mode: carbon_api::GameMode::Survival,
        });
        for _ in 0..20 {
            state.advance_tick();
        }
        assert!(state.vitals(id).unwrap().health < 20.0);
    }

    #[test]
    fn chat_is_validated_revisioned_and_announces_presence() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Chatter".into(),
            world: "world".into(),
            position: BlockPosition { x: 0, y: 65, z: 0 },
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(state.publish_chat("Chatter", "hello"));
        assert!(!state.publish_chat("Chatter", ""));
        assert!(!state.publish_chat("Chatter", "bad\nline"));
        assert!(!state.publish_chat("Missing", "spoof"));
        assert!(!state.publish_chat("Chatter", &"x".repeat(257)));
        assert!(state.remove_player(id));

        let messages = state.chat_messages_since(0);
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].text, "Chatter joined the game");
        assert_eq!(messages[1].sender.as_deref(), Some("Chatter"));
        assert_eq!(messages[1].text, "hello");
        assert_eq!(messages[2].text, "Chatter left the game");
        assert!(state.chat_messages_since(messages[2].revision).is_empty());
    }

    #[test]
    fn disconnect_requests_are_validated_targeted_and_revisioned() {
        let (shutdown, _) = watch::channel(false);
        let state = ServerState::new("world".into(), 0, shutdown);
        let id = Uuid::new_v4();
        assert!(!state.request_disconnect(id, "not online"));
        assert!(state.add_player(PlayerSnapshot {
            id,
            name: "Target".into(),
            world: "world".into(),
            position: BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        assert!(!state.request_disconnect(id, ""));
        assert!(!state.request_disconnect(id, "bad\nreason"));
        assert!(!state.request_disconnect(id, &"x".repeat(257)));
        assert!(state.request_disconnect(id, "Testing moderation"));
        let requests = state.disconnects_since(0);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].player_id, id);
        assert_eq!(requests[0].reason, "Testing moderation");
        assert!(state.disconnects_since(requests[0].revision).is_empty());
    }
}
