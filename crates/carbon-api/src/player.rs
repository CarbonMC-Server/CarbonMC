use uuid::Uuid;

use crate::{BlockPosition, EntityPosition};

/// One server-authored chat line in Carbon's bounded in-memory history.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChatMessage {
    pub revision: u64,
    pub sender: Option<String>,
    pub text: String,
}

/// A revisioned request to close one connected player's play session.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerDisconnect {
    pub revision: u64,
    pub player_id: Uuid,
    pub reason: String,
}

/// Public view of an active name ban. `expires_at_unix == None` is permanent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerBan {
    pub name: String,
    pub reason: String,
    pub expires_at_unix: Option<u64>,
}

/// One durable administrative action recorded by Carbon.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModerationRecord {
    pub timestamp_unix: u64,
    pub actor: String,
    pub action: String,
    pub target: Option<String>,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StatusEffectKind {
    Speed,
    Slowness,
    Strength,
    Regeneration,
    Resistance,
    FireResistance,
    Hunger,
    Poison,
}

impl StatusEffectKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Speed => "speed",
            Self::Slowness => "slowness",
            Self::Strength => "strength",
            Self::Regeneration => "regeneration",
            Self::Resistance => "resistance",
            Self::FireResistance => "fire_resistance",
            Self::Hunger => "hunger",
            Self::Poison => "poison",
        }
    }

    #[must_use]
    pub const fn protocol_id(self) -> i32 {
        match self {
            Self::Speed => 0,
            Self::Slowness => 1,
            Self::Strength => 4,
            Self::Regeneration => 9,
            Self::Resistance => 10,
            Self::FireResistance => 11,
            Self::Hunger => 16,
            Self::Poison => 18,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StatusEffectSnapshot {
    pub kind: StatusEffectKind,
    pub amplifier: u8,
    pub remaining_ticks: u32,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemKind {
    Stone,
    Cobblestone,
    Dirt,
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
    BirchLog,
    SpruceLog,
    OakPlanks,
    Stick,
    Apple,
    CraftingTable,
    Furnace,
    Chest,
    IronIngot,
    CopperIngot,
    GoldIngot,
    Flint,
    FlintAndSteel,
    Bucket,
    MilkBucket,
    WoodenPickaxe,
    WoodenAxe,
    WoodenShovel,
    WoodenSword,
    StonePickaxe,
    StoneAxe,
    StoneShovel,
    StoneSword,
    IronPickaxe,
    IronAxe,
    IronShovel,
    IronSword,
    DiamondPickaxe,
    DiamondAxe,
    DiamondShovel,
    DiamondSword,
    Shield,
    RawBeef,
    Porkchop,
    CookedBeef,
    CookedPorkchop,
    RottenFlesh,
    Coal,
    Charcoal,
    RawIron,
    RawCopper,
    RawGold,
    Redstone,
    LapisLazuli,
    Diamond,
    IronHelmet,
    IronChestplate,
    IronLeggings,
    IronBoots,
    DiamondHelmet,
    DiamondChestplate,
    DiamondLeggings,
    DiamondBoots,
}

impl ItemKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stone => "stone",
            Self::Cobblestone => "cobblestone",
            Self::Dirt => "dirt",
            Self::Sand => "sand",
            Self::Gravel => "gravel",
            Self::Sandstone => "sandstone",
            Self::SnowBlock => "snow_block",
            Self::Netherrack => "netherrack",
            Self::SoulSand => "soul_sand",
            Self::Basalt => "basalt",
            Self::EndStone => "end_stone",
            Self::Obsidian => "obsidian",
            Self::StoneBricks => "stone_bricks",
            Self::OakLog => "oak_log",
            Self::BirchLog => "birch_log",
            Self::SpruceLog => "spruce_log",
            Self::OakPlanks => "oak_planks",
            Self::Stick => "stick",
            Self::Apple => "apple",
            Self::CraftingTable => "crafting_table",
            Self::Furnace => "furnace",
            Self::Chest => "chest",
            Self::IronIngot => "iron_ingot",
            Self::CopperIngot => "copper_ingot",
            Self::GoldIngot => "gold_ingot",
            Self::Flint => "flint",
            Self::FlintAndSteel => "flint_and_steel",
            Self::Bucket => "bucket",
            Self::MilkBucket => "milk_bucket",
            Self::WoodenPickaxe => "wooden_pickaxe",
            Self::WoodenAxe => "wooden_axe",
            Self::WoodenShovel => "wooden_shovel",
            Self::WoodenSword => "wooden_sword",
            Self::StonePickaxe => "stone_pickaxe",
            Self::StoneAxe => "stone_axe",
            Self::StoneShovel => "stone_shovel",
            Self::StoneSword => "stone_sword",
            Self::IronPickaxe => "iron_pickaxe",
            Self::IronAxe => "iron_axe",
            Self::IronShovel => "iron_shovel",
            Self::IronSword => "iron_sword",
            Self::DiamondPickaxe => "diamond_pickaxe",
            Self::DiamondAxe => "diamond_axe",
            Self::DiamondShovel => "diamond_shovel",
            Self::DiamondSword => "diamond_sword",
            Self::Shield => "shield",
            Self::RawBeef => "beef",
            Self::Porkchop => "porkchop",
            Self::CookedBeef => "cooked_beef",
            Self::CookedPorkchop => "cooked_porkchop",
            Self::RottenFlesh => "rotten_flesh",
            Self::Coal => "coal",
            Self::Charcoal => "charcoal",
            Self::RawIron => "raw_iron",
            Self::RawCopper => "raw_copper",
            Self::RawGold => "raw_gold",
            Self::Redstone => "redstone",
            Self::LapisLazuli => "lapis_lazuli",
            Self::Diamond => "diamond",
            Self::IronHelmet => "iron_helmet",
            Self::IronChestplate => "iron_chestplate",
            Self::IronLeggings => "iron_leggings",
            Self::IronBoots => "iron_boots",
            Self::DiamondHelmet => "diamond_helmet",
            Self::DiamondChestplate => "diamond_chestplate",
            Self::DiamondLeggings => "diamond_leggings",
            Self::DiamondBoots => "diamond_boots",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemStack {
    pub kind: ItemKind,
    pub count: u8,
    pub damage: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerInventory {
    pub slots: [Option<ItemStack>; 36],
    pub sharpness_levels: [u8; 36],
    pub revision: u64,
}

/// Item carried by the mouse while the player has an inventory screen open.
///
/// Enchantment metadata is kept beside the stack until Carbon has a general
/// data-component model for items.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InventoryCursor {
    pub stack: Option<ItemStack>,
    pub sharpness_level: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FurnaceSlot {
    Input,
    Fuel,
    Output,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FurnaceSnapshot {
    pub input: Option<ItemStack>,
    pub fuel: Option<ItemStack>,
    pub output: Option<ItemStack>,
    pub burn_remaining: u16,
    pub burn_total: u16,
    pub cook_progress: u16,
    pub cook_total: u16,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChestSnapshot {
    pub slots: [Option<ItemStack>; 27],
    pub sharpness_levels: [u8; 27],
    pub revision: u64,
}

/// A slot in the raw 26.2 player inventory rather than a menu/window slot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlayerInventorySlot {
    Storage(u8),
    /// Feet, legs, chest, and head.
    Armor(u8),
    OffHand,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerEquipment {
    pub main_hand: Option<ItemStack>,
    pub main_hand_sharpness: u8,
    pub off_hand: Option<ItemStack>,
    /// Feet, legs, chest, and head, matching the protocol's equipment order.
    pub armor: [Option<ItemStack>; 4],
    pub selected_slot: u8,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlayerCombatState {
    pub sprinting: bool,
    pub blocking_since: Option<u64>,
    pub shield_disabled_until: u64,
    pub falling: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerImpulse {
    pub revision: u64,
    pub player_id: Uuid,
    pub velocity: EntityPosition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerVitals {
    pub health: f32,
    pub food: u8,
    pub saturation: f32,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerTransform {
    pub id: Uuid,
    pub position: EntityPosition,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerEventKind {
    SwingMainArm,
    SwingOffHand,
    Hurt,
    Died,
    CriticalHit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerEvent {
    pub revision: u64,
    pub player_id: Uuid,
    pub kind: PlayerEventKind,
}

impl Default for PlayerVitals {
    fn default() -> Self {
        Self {
            health: 20.0,
            food: 20,
            saturation: 5.0,
            revision: 0,
        }
    }
}

impl Default for PlayerInventory {
    fn default() -> Self {
        Self {
            slots: [None; 36],
            sharpness_levels: [0; 36],
            revision: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

/// Read-only player data safe to pass across API boundaries.
#[derive(Clone, Debug)]
pub struct PlayerSnapshot {
    pub id: Uuid,
    pub name: String,
    pub world: String,
    pub position: BlockPosition,
    pub game_mode: GameMode,
}
