#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkPosition {
    pub x: i32,
    pub z: i32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BlockKind {
    Air,
    Water,
    Lava,
    Bedrock,
    Stone,
    CoalOre,
    IronOre,
    CopperOre,
    GoldOre,
    RedstoneOre,
    LapisOre,
    DiamondOre,
    Cobblestone,
    Dirt,
    Grass,
    ShortGrass,
    Fern,
    DeadBush,
    Sand,
    Gravel,
    Sandstone,
    SnowBlock,
    Netherrack,
    SoulSand,
    Basalt,
    EndStone,
    Obsidian,
    StoneBricks,
    OakLog,
    OakPlanks,
    OakLeaves,
    BirchLog,
    BirchLeaves,
    SpruceLog,
    SpruceLeaves,
    CraftingTable,
    Furnace,
    Chest,
    NetherPortal,
    EndPortal,
}

impl BlockKind {
    #[must_use]
    pub fn is_surface_plant(self) -> bool {
        matches!(self, Self::ShortGrass | Self::Fern | Self::DeadBush)
    }

    #[must_use]
    pub fn is_replaceable(self) -> bool {
        self == Self::Air || self.is_surface_plant()
    }

    #[must_use]
    pub fn is_passable(self) -> bool {
        self.is_replaceable() || matches!(self, Self::NetherPortal | Self::EndPortal)
    }

    #[must_use]
    pub fn plant_survives_on(self, support: Self) -> bool {
        match self {
            Self::ShortGrass | Self::Fern => matches!(support, Self::Grass | Self::Dirt),
            Self::DeadBush => matches!(support, Self::Sand | Self::Dirt | Self::Grass),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BiomeKind {
    Plains,
    Forest,
    BirchForest,
    Taiga,
    Registry(u8),
}

impl BiomeKind {
    pub const COUNT: usize = 66;

    #[must_use]
    pub fn from_protocol_id(id: u8) -> Self {
        match id {
            4 => Self::BirchForest,
            21 => Self::Forest,
            40 => Self::Plains,
            56 => Self::Taiga,
            value => Self::Registry(value.min(65)),
        }
    }

    #[must_use]
    pub fn protocol_id(self) -> i32 {
        match self {
            Self::BirchForest => 4,
            Self::Forest => 21,
            Self::Plains => 40,
            Self::Taiga => 56,
            Self::Registry(id) => i32::from(id),
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        const NAMES: [&str; BiomeKind::COUNT] = [
            "badlands",
            "bamboo_jungle",
            "basalt_deltas",
            "beach",
            "birch_forest",
            "cherry_grove",
            "cold_ocean",
            "crimson_forest",
            "dark_forest",
            "deep_cold_ocean",
            "deep_dark",
            "deep_frozen_ocean",
            "deep_lukewarm_ocean",
            "deep_ocean",
            "desert",
            "dripstone_caves",
            "end_barrens",
            "end_highlands",
            "end_midlands",
            "eroded_badlands",
            "flower_forest",
            "forest",
            "frozen_ocean",
            "frozen_peaks",
            "frozen_river",
            "grove",
            "ice_spikes",
            "jagged_peaks",
            "jungle",
            "lukewarm_ocean",
            "lush_caves",
            "mangrove_swamp",
            "meadow",
            "mushroom_fields",
            "nether_wastes",
            "ocean",
            "old_growth_birch_forest",
            "old_growth_pine_taiga",
            "old_growth_spruce_taiga",
            "pale_garden",
            "plains",
            "river",
            "savanna",
            "savanna_plateau",
            "small_end_islands",
            "snowy_beach",
            "snowy_plains",
            "snowy_slopes",
            "snowy_taiga",
            "soul_sand_valley",
            "sparse_jungle",
            "stony_peaks",
            "stony_shore",
            "sulfur_caves",
            "sunflower_plains",
            "swamp",
            "taiga",
            "the_end",
            "the_void",
            "warm_ocean",
            "warped_forest",
            "windswept_forest",
            "windswept_gravelly_hills",
            "windswept_hills",
            "windswept_savanna",
            "wooded_badlands",
        ];
        NAMES[usize::try_from(self.protocol_id())
            .unwrap_or(40)
            .min(NAMES.len() - 1)]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DimensionKind {
    #[default]
    Overworld,
    Nether,
    End,
}

impl DimensionKind {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Overworld => "minecraft:overworld",
            Self::Nether => "minecraft:the_nether",
            Self::End => "minecraft:the_end",
        }
    }

    #[must_use]
    pub fn type_id(self) -> i32 {
        match self {
            Self::Overworld => 0,
            Self::Nether => 3,
            Self::End => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockPlacement {
    pub position: BlockPosition,
    pub kind: BlockKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockChange {
    pub revision: u64,
    pub dimension: DimensionKind,
    pub position: BlockPosition,
    pub kind: BlockKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainProfile {
    pub surface: BlockKind,
    pub filler: BlockKind,
    pub foundation: BlockKind,
    pub bedrock: BlockKind,
}

/// Compact read-only information about one generated chunk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkSnapshot {
    pub position: ChunkPosition,
    pub dimension: DimensionKind,
    pub biome: BiomeKind,
    pub min_surface_y: i32,
    pub max_surface_y: i32,
    pub revision: u64,
}

/// Read-only world data safe to expose to extensions.
#[derive(Clone, Debug)]
pub struct WorldSnapshot {
    pub name: String,
    pub seed: i64,
    pub age_ticks: u64,
    pub player_count: usize,
}
