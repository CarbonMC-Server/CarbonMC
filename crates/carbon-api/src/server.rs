use uuid::Uuid;

use crate::{
    BiomeKind, BlockChange, BlockKind, BlockPlacement, BlockPosition, ChatMessage, ChestSnapshot,
    ChunkPosition, ChunkSnapshot, DimensionKind, FurnaceSlot, FurnaceSnapshot, InventoryCursor,
    ItemEntitySnapshot, ItemKind, ItemStack, MobKind, MobSnapshot, ModerationRecord, PlayerBan,
    PlayerCombatState, PlayerDisconnect, PlayerEquipment, PlayerEvent, PlayerImpulse,
    PlayerInventory, PlayerInventorySlot, PlayerSnapshot, PlayerTransform, PlayerVitals,
    StatusEffectKind, StatusEffectSnapshot, TerrainProfile, WorldSnapshot,
};

/// Thread-safe capabilities exposed to commands and extensions.
pub trait ServerApi: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn version(&self) -> &str;
    fn current_tick(&self) -> u64;
    fn players(&self) -> Vec<PlayerSnapshot>;
    fn add_player(&self, player: PlayerSnapshot) -> bool;
    fn remove_player(&self, id: Uuid) -> bool;
    fn disconnects_since(&self, revision: u64) -> Vec<PlayerDisconnect>;
    /// Queues a validated disconnect for an online player.
    fn request_disconnect(&self, id: Uuid, reason: &str) -> bool;
    fn update_player_position(&self, id: Uuid, position: BlockPosition) -> bool;
    fn player_transforms(&self) -> Vec<PlayerTransform>;
    fn update_player_transform(&self, transform: PlayerTransform) -> bool;
    fn player_events_since(&self, revision: u64) -> Vec<PlayerEvent>;
    fn swing_player(&self, id: Uuid, off_hand: bool) -> bool;
    fn critical_hit_player(&self, id: Uuid) -> bool;
    fn worlds(&self) -> Vec<WorldSnapshot>;
    fn chunks(&self) -> Vec<ChunkSnapshot>;
    fn chunk_surface(&self, position: ChunkPosition) -> Option<[i16; 256]>;
    fn ensure_chunk_surface(&self, position: ChunkPosition) -> [i16; 256];
    fn chunk_biome(&self, position: ChunkPosition) -> BiomeKind;
    fn chunk_terrain(&self, position: ChunkPosition) -> TerrainProfile;
    fn chunk_blocks(&self, position: ChunkPosition) -> Vec<BlockPlacement>;
    fn block_at(&self, position: BlockPosition) -> BlockKind;
    fn set_block(&self, position: BlockPosition, kind: BlockKind) -> bool;
    fn block_changes_since(&self, revision: u64) -> Vec<BlockChange>;
    fn change_player_dimension(&self, id: Uuid, dimension: DimensionKind) -> bool;
    fn ensure_dimension_chunk_surface(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> [i16; 256];
    fn dimension_chunk_biome(&self, dimension: DimensionKind, position: ChunkPosition)
        -> BiomeKind;
    fn dimension_chunk_terrain(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> TerrainProfile;
    fn dimension_chunk_blocks(
        &self,
        dimension: DimensionKind,
        position: ChunkPosition,
    ) -> Vec<BlockPlacement>;
    fn dimension_block_at(&self, dimension: DimensionKind, position: BlockPosition) -> BlockKind;
    fn set_dimension_block(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
        kind: BlockKind,
    ) -> bool;
    /// Activates a complete Nether portal frame containing `position`.
    fn ignite_nether_portal(&self, dimension: DimensionKind, position: BlockPosition) -> bool;
    fn furnace(&self, dimension: DimensionKind, position: BlockPosition)
        -> Option<FurnaceSnapshot>;
    fn click_furnace_slot(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
        slot: FurnaceSlot,
        cursor: InventoryCursor,
        right_click: bool,
    ) -> Option<InventoryCursor>;
    fn take_furnace_contents(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
    ) -> Vec<ItemStack>;
    fn chest(&self, dimension: DimensionKind, position: BlockPosition) -> Option<ChestSnapshot>;
    fn click_chest_slot(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
        slot: u8,
        cursor: InventoryCursor,
        right_click: bool,
    ) -> Option<InventoryCursor>;
    fn take_chest_contents(
        &self,
        dimension: DimensionKind,
        position: BlockPosition,
    ) -> Vec<ItemStack>;
    fn inventory(&self, id: Uuid) -> Option<PlayerInventory>;
    fn player_equipment(&self, id: Uuid) -> Option<PlayerEquipment>;
    fn set_selected_slot(&self, id: Uuid, slot: u8) -> bool;
    fn enchant_selected_weapon(&self, id: Uuid, sharpness_level: u8) -> bool;
    fn set_player_armor(&self, id: Uuid, armor: [Option<ItemStack>; 4]) -> bool;
    fn set_offhand(&self, id: Uuid, stack: Option<ItemStack>) -> bool;
    /// Applies one authoritative left/right pickup click and returns the new cursor.
    fn click_player_inventory_slot(
        &self,
        id: Uuid,
        slot: PlayerInventorySlot,
        cursor: InventoryCursor,
        right_click: bool,
    ) -> Option<InventoryCursor>;
    fn player_combat_state(&self, id: Uuid) -> Option<PlayerCombatState>;
    fn set_player_sprinting(&self, id: Uuid, sprinting: bool) -> bool;
    fn set_player_falling(&self, id: Uuid, falling: bool) -> bool;
    fn set_player_blocking(&self, id: Uuid, blocking: bool) -> bool;
    fn disable_player_shield(&self, id: Uuid, duration_ticks: u64) -> bool;
    fn give_item(&self, id: Uuid, kind: ItemKind, count: u8) -> bool;
    fn take_item(&self, id: Uuid, slot: usize, count: u8) -> bool;
    fn damage_item(&self, id: Uuid, slot: usize, amount: u16) -> bool;
    fn craft(&self, id: Uuid, recipe: &str) -> bool;
    fn consume_food(&self, id: Uuid, slot: usize) -> bool;
    /// Replaces one empty bucket in the selected storage slot with a milk bucket.
    fn fill_milk_bucket(&self, id: Uuid, slot: usize) -> bool;
    /// Drinks one milk bucket, returns an empty bucket, and clears all status effects.
    fn consume_milk(&self, id: Uuid, slot: usize) -> bool;
    fn vitals(&self, id: Uuid) -> Option<PlayerVitals>;
    fn status_effects(&self, id: Uuid) -> Vec<StatusEffectSnapshot>;
    fn apply_status_effect(
        &self,
        id: Uuid,
        kind: StatusEffectKind,
        amplifier: u8,
        duration_ticks: u32,
    ) -> bool;
    fn clear_status_effect(&self, id: Uuid, kind: StatusEffectKind) -> bool;
    fn damage_player(&self, id: Uuid, amount: f32) -> bool;
    fn damage_player_combat(&self, id: Uuid, amount: f32) -> bool;
    /// Applies directional PvP damage. Returns false when a ready shield blocks it.
    fn attack_player(&self, attacker: Uuid, target: Uuid, amount: f32) -> bool;
    fn knockback_player(&self, attacker: Uuid, target: Uuid, strength: f64) -> bool;
    fn player_impulses_since(&self, revision: u64) -> Vec<PlayerImpulse>;
    fn respawn_player(&self, id: Uuid) -> bool;
    fn mobs(&self) -> Vec<MobSnapshot>;
    fn spawn_mob(&self, kind: MobKind, position: BlockPosition) -> MobSnapshot;
    fn damage_mob(&self, entity_id: i32, amount: f32) -> Option<MobSnapshot>;
    fn dimension_mobs(&self, dimension: DimensionKind) -> Vec<MobSnapshot>;
    fn damage_dimension_mob(
        &self,
        dimension: DimensionKind,
        entity_id: i32,
        amount: f32,
    ) -> Option<MobSnapshot>;
    fn items(&self) -> Vec<ItemEntitySnapshot>;
    fn drop_item(&self, stack: crate::ItemStack, position: BlockPosition) -> ItemEntitySnapshot;
    fn dimension_items(&self, dimension: DimensionKind) -> Vec<ItemEntitySnapshot>;
    fn drop_dimension_item(
        &self,
        dimension: DimensionKind,
        stack: crate::ItemStack,
        position: BlockPosition,
    ) -> ItemEntitySnapshot;
    fn operators(&self) -> Vec<String>;
    fn is_operator(&self, name: &str) -> bool;
    /// Changes operator status and returns whether the stored state changed.
    fn set_operator(&self, name: &str, operator: bool) -> crate::Result<bool>;
    fn banned_players(&self) -> Vec<String>;
    fn bans(&self) -> Vec<PlayerBan>;
    fn active_ban(&self, name: &str) -> Option<PlayerBan>;
    fn is_banned(&self, name: &str) -> bool;
    /// Creates or replaces a permanent or expiring name ban.
    fn ban_player(
        &self,
        name: &str,
        reason: &str,
        expires_at_unix: Option<u64>,
    ) -> crate::Result<bool>;
    /// Changes ban status and returns whether the stored state changed.
    fn set_banned(&self, name: &str, banned: bool) -> crate::Result<bool>;
    fn allowlisted_players(&self) -> Vec<String>;
    fn is_allowlisted(&self, name: &str) -> bool;
    /// Changes allowlist membership and returns whether the stored state changed.
    fn set_allowlisted(&self, name: &str, allowed: bool) -> crate::Result<bool>;
    /// Explicit rules, including `!`-prefixed denies, in sorted order.
    fn permission_nodes(&self, name: &str) -> Vec<String>;
    /// Operators bypass rules; otherwise any matching deny overrides all grants.
    fn has_permission(&self, name: &str, permission: &str) -> bool;
    /// Adds a grant or `!`-prefixed deny rule; returns whether it was newly stored.
    fn grant_permission(&self, name: &str, permission: &str) -> crate::Result<bool>;
    /// Removes the exact rule (including its `!` prefix, if any).
    fn revoke_permission(&self, name: &str, permission: &str) -> crate::Result<bool>;
    fn moderation_records(&self, limit: usize) -> Vec<ModerationRecord>;
    fn record_moderation(
        &self,
        actor: &str,
        action: &str,
        target: Option<&str>,
        detail: &str,
    ) -> crate::Result<()>;
    fn chat_messages_since(&self, revision: u64) -> Vec<ChatMessage>;
    /// Publishes validated player chat. Returns false when rejected.
    fn publish_chat(&self, sender: &str, message: &str) -> bool;
    fn broadcast(&self, message: &str);
    fn request_shutdown(&self);
}
