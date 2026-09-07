use uuid::Uuid;

/// Mob families implemented by Carbon's prototype simulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobKind {
    Cow,
    Pig,
    Zombie,
}

impl MobKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cow => "cow",
            Self::Pig => "pig",
            Self::Zombie => "zombie",
        }
    }
}

/// High-level behavior currently selected by a mob's AI controller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MobAiState {
    Idle,
    Wandering,
    Chasing,
}

/// A precise position used by simulated entities and protocol updates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityPosition {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

/// Read-only mob state exposed to extensions and network sessions.
#[derive(Clone, Debug, PartialEq)]
pub struct MobSnapshot {
    pub entity_id: i32,
    pub id: Uuid,
    pub kind: MobKind,
    pub position: EntityPosition,
    pub velocity: EntityPosition,
    pub yaw: f32,
    pub ai_state: MobAiState,
    pub health: f32,
    pub on_fire: bool,
}

#[derive(Clone, Debug)]
pub struct ItemEntitySnapshot {
    pub entity_id: i32,
    pub id: Uuid,
    pub stack: crate::ItemStack,
    pub position: EntityPosition,
    pub velocity: EntityPosition,
    pub age_ticks: u64,
}
