//! Public, implementation-independent contracts for Carbon extensions.

mod command;
mod entity;
mod event;
mod extension;
mod player;
mod server;
mod world;

pub use command::{
    CommandContext, CommandHandler, CommandOutput, CommandRegistration, CommandSender,
};
pub use entity::{EntityPosition, ItemEntitySnapshot, MobAiState, MobKind, MobSnapshot};
pub use event::Event;
pub use extension::{Extension, ExtensionContext, ExtensionMetadata};
pub use player::{
    ChatMessage, ChestSnapshot, FurnaceSlot, FurnaceSnapshot, GameMode, InventoryCursor, ItemKind,
    ItemStack, ModerationRecord, PlayerBan, PlayerCombatState, PlayerDisconnect, PlayerEquipment,
    PlayerEvent, PlayerEventKind, PlayerImpulse, PlayerInventory, PlayerInventorySlot,
    PlayerSnapshot, PlayerTransform, PlayerVitals, StatusEffectKind, StatusEffectSnapshot,
};
pub use server::ServerApi;
pub use world::{
    BiomeKind, BlockChange, BlockKind, BlockPlacement, BlockPosition, ChunkPosition, ChunkSnapshot,
    DimensionKind, TerrainProfile, WorldSnapshot,
};

/// Error type exposed at the extension boundary.
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;
/// Result type exposed at the extension boundary.
pub type Result<T> = std::result::Result<T, Error>;
