use std::{fs, net::SocketAddr, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
#[derive(Default)]
pub struct CarbonConfig {
    pub server: ServerConfig,
    pub logging: LoggingConfig,
    pub world: WorldConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub motd: String,
    pub max_players: u32,
    pub ticks_per_second: u16,
    pub view_distance: u8,
    pub online_mode: bool,
    pub allowlist_enabled: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:25565".parse().expect("default address is valid"),
            motd: "A Carbon server".into(),
            max_players: 20,
            ticks_per_second: 20,
            view_distance: 8,
            online_mode: true,
            allowlist_enabled: false,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LoggingConfig {
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldConfig {
    pub name: String,
    pub seed: i64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            name: "world".into(),
            seed: 0,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not read configuration at {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("configuration is not valid TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Validation(String),
}

impl CarbonConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let config: Self = toml::from_str(&contents)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=1000).contains(&self.server.ticks_per_second) {
            return Err(ConfigError::Validation(
                "server.ticks_per_second must be between 1 and 1000".into(),
            ));
        }
        if !(2..=32).contains(&self.server.view_distance) {
            return Err(ConfigError::Validation(
                "server.view_distance must be between 2 and 32".into(),
            ));
        }
        if self.server.max_players == 0 {
            return Err(ConfigError::Validation(
                "server.max_players must be at least 1".into(),
            ));
        }
        if self.server.motd.chars().count() > 256 {
            return Err(ConfigError::Validation(
                "server.motd cannot exceed 256 characters".into(),
            ));
        }
        if self.world.name.trim().is_empty() {
            return Err(ConfigError::Validation("world.name cannot be empty".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_an_unreasonable_tick_rate() {
        let mut config = CarbonConfig::default();
        config.server.ticks_per_second = 0;
        assert!(matches!(config.validate(), Err(ConfigError::Validation(_))));
    }

    #[test]
    fn fills_defaults_for_empty_document() {
        let config: CarbonConfig = toml::from_str("").expect("empty TOML uses defaults");
        assert_eq!(config.server.max_players, 20);
        assert!(!config.server.allowlist_enabled);
    }

    #[test]
    fn rejects_a_zero_player_capacity() {
        let mut config = CarbonConfig::default();
        config.server.max_players = 0;
        assert!(matches!(config.validate(), Err(ConfigError::Validation(_))));
    }
}
