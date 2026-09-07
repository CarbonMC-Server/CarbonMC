use crate::PlayerSnapshot;

/// Versionable high-level events delivered to extensions.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Event {
    ServerStarted,
    ServerStopping,
    Tick {
        number: u64,
    },
    PlayerJoined {
        player: PlayerSnapshot,
    },
    PlayerLeft {
        player: PlayerSnapshot,
        reason: String,
    },
}
