use async_trait::async_trait;
use carbon_api::{
    BlockPosition, CommandContext, CommandHandler, CommandOutput, CommandRegistration, ItemKind,
    MobKind, Result, StatusEffectKind,
};

use super::CommandRegistry;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn register(registry: &CommandRegistry) -> anyhow::Result<()> {
    registry.register(CommandRegistration::new(
        "help",
        "Lists available commands",
        "help",
        Help(registry.clone()),
    ))?;
    registry.register(CommandRegistration::new(
        "list",
        "Lists connected players",
        "list",
        List,
    ))?;
    registry.register(
        CommandRegistration::new(
            "give",
            "Adds an item stack to an online player's inventory",
            "give <player> <item> [count]",
            Give,
        )
        .requires_permission("carbon.command.give"),
    )?;
    registry.register(
        CommandRegistration::new("say", "Broadcasts a server message", "say <message>", Say)
            .requires_permission("carbon.command.say"),
    )?;
    registry.register(CommandRegistration::new(
        "version",
        "Displays the Carbon version",
        "version",
        Version,
    ))?;
    registry.register(CommandRegistration::new(
        "chunks",
        "Shows the generated chunk cache",
        "chunks",
        Chunks,
    ))?;
    registry.register(CommandRegistration::new(
        "mobs",
        "Lists simulated mobs and their AI states",
        "mobs",
        Mobs,
    ))?;
    registry.register(
        CommandRegistration::new(
            "spawnmob",
            "Spawns a prototype mob",
            "spawnmob <cow|pig|zombie> [x y z]",
            SpawnMob,
        )
        .requires_permission("carbon.command.spawnmob"),
    )?;
    registry.register(
        CommandRegistration::new("op", "Grants server operator status", "op <player>", Op)
            .requires_permission("carbon.command.op"),
    )?;
    registry.register(
        CommandRegistration::new(
            "deop",
            "Revokes server operator status",
            "deop <player>",
            Deop,
        )
        .requires_permission("carbon.command.deop"),
    )?;
    registry.register(
        CommandRegistration::new("ban", "Bans a player name", "ban <player> [reason]", Ban)
            .requires_permission("carbon.command.ban"),
    )?;
    registry.register(
        CommandRegistration::new(
            "tempban",
            "Temporarily bans a player name",
            "tempban <player> <duration> [reason]",
            TempBan,
        )
        .requires_permission("carbon.command.tempban"),
    )?;
    registry.register(
        CommandRegistration::new(
            "kick",
            "Disconnects an online player",
            "kick <player> [reason]",
            Kick,
        )
        .requires_permission("carbon.command.kick"),
    )?;
    registry.register(
        CommandRegistration::new("pardon", "Removes a player ban", "pardon <player>", Pardon)
            .requires_permission("carbon.command.pardon"),
    )?;
    registry.register(
        CommandRegistration::new("banlist", "Lists banned player names", "banlist", BanList)
            .requires_permission("carbon.command.banlist"),
    )?;
    registry.register(
        CommandRegistration::new(
            "allowlist",
            "Manages the server allowlist",
            "allowlist <add|remove|list> [player]",
            Allowlist,
        )
        .requires_permission("carbon.command.allowlist"),
    )?;
    registry.register(
        CommandRegistration::new(
            "audit",
            "Shows recent moderation actions",
            "audit [count]",
            Audit,
        )
        .requires_permission("carbon.command.audit"),
    )?;
    registry.register(
        CommandRegistration::new(
            "grantperm",
            "Grants a player permission node",
            "grantperm <player> <permission>",
            GrantPermission,
        )
        .requires_permission("carbon.command.permissions"),
    )?;
    registry.register(
        CommandRegistration::new(
            "revokeperm",
            "Revokes a player permission node",
            "revokeperm <player> <permission>",
            RevokePermission,
        )
        .requires_permission("carbon.command.permissions"),
    )?;
    registry.register(
        CommandRegistration::new(
            "permissions",
            "Lists a player's permission nodes",
            "permissions <player>",
            Permissions,
        )
        .requires_permission("carbon.command.permissions"),
    )?;
    registry.register(
        CommandRegistration::new(
            "effect",
            "Applies or clears a player's status effects",
            "effect <give <player> <effect> <seconds> [level]|clear <player> [effect]>",
            Effect,
        )
        .requires_permission("carbon.command.effect"),
    )?;
    registry.register(
        CommandRegistration::new("stop", "Stops the server cleanly", "stop", Stop)
            .requires_permission("carbon.command.stop"),
    )?;
    Ok(())
}

struct Help(CommandRegistry);

#[async_trait]
impl CommandHandler for Help {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let lines = self
            .0
            .descriptions_for(&context.sender, context.server.as_ref())
            .into_iter()
            .map(|(name, description, usage)| format!("{name} — {description} (usage: {usage})"))
            .collect();
        Ok(CommandOutput { lines })
    }
}

struct List;

#[async_trait]
impl CommandHandler for List {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let players = context.server.players();
        if players.is_empty() {
            return Ok(CommandOutput::message("There are no connected players."));
        }
        let names = players
            .into_iter()
            .map(|player| player.name)
            .collect::<Vec<_>>();
        Ok(CommandOutput::message(format!(
            "Players: {}",
            names.join(", ")
        )))
    }
}

struct Say;

#[async_trait]
impl CommandHandler for Say {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        if context.arguments.is_empty() {
            return Ok(CommandOutput::message("Usage: say <message>"));
        }
        let message = context.arguments.join(" ");
        context.server.broadcast(&format!("[Server] {message}"));
        Ok(CommandOutput::message("Message broadcast."))
    }
}

struct Version;

#[async_trait]
impl CommandHandler for Version {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        Ok(CommandOutput::message(format!(
            "{} {} (original Rust implementation)",
            context.server.name(),
            context.server.version()
        )))
    }
}

struct Chunks;

#[async_trait]
impl CommandHandler for Chunks {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let chunks = context.server.chunks();
        Ok(CommandOutput::message(format!(
            "{} generated chunks loaded ({} block columns).",
            chunks.len(),
            chunks.len() * 16 * 16
        )))
    }
}

struct Mobs;

#[async_trait]
impl CommandHandler for Mobs {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let mobs = context.server.mobs();
        if mobs.is_empty() {
            return Ok(CommandOutput::message("There are no simulated mobs."));
        }
        let lines = mobs
            .into_iter()
            .map(|mob| {
                format!(
                    "#{} {} at {:.1}, {:.1}, {:.1} ({:?})",
                    mob.entity_id,
                    mob.kind.as_str(),
                    mob.position.x,
                    mob.position.y,
                    mob.position.z,
                    mob.ai_state
                )
            })
            .collect();
        Ok(CommandOutput { lines })
    }
}

struct SpawnMob;

#[async_trait]
impl CommandHandler for SpawnMob {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(kind) = context
            .arguments
            .first()
            .and_then(|kind| parse_mob_kind(kind))
        else {
            return Ok(CommandOutput::message(
                "Usage: spawnmob <cow|pig|zombie> [x y z]",
            ));
        };
        let position = if context.arguments.len() == 4 {
            let parsed = context.arguments[1..]
                .iter()
                .map(|value| value.parse::<i32>())
                .collect::<std::result::Result<Vec<_>, _>>();
            let Ok(coordinates) = parsed else {
                return Ok(CommandOutput::message("Coordinates must be whole numbers."));
            };
            BlockPosition {
                x: coordinates[0],
                y: coordinates[1],
                z: coordinates[2],
            }
        } else if context.arguments.len() == 1 {
            BlockPosition {
                x: -8,
                y: 65,
                z: -8,
            }
        } else {
            return Ok(CommandOutput::message(
                "Usage: spawnmob <cow|pig|zombie> [x y z]",
            ));
        };
        let mob = context.server.spawn_mob(kind, position);
        Ok(CommandOutput::message(format!(
            "Spawned {} as entity #{}.",
            mob.kind.as_str(),
            mob.entity_id
        )))
    }
}

fn parse_mob_kind(value: &str) -> Option<MobKind> {
    match value.to_ascii_lowercase().as_str() {
        "cow" => Some(MobKind::Cow),
        "pig" => Some(MobKind::Pig),
        "zombie" => Some(MobKind::Zombie),
        _ => None,
    }
}

struct Give;

#[async_trait]
impl CommandHandler for Give {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        if !(2..=3).contains(&context.arguments.len()) {
            return Ok(CommandOutput::message(
                "Usage: give <player> <item> [count]",
            ));
        }
        let Some(player) = context
            .server
            .players()
            .into_iter()
            .find(|player| player.name.eq_ignore_ascii_case(&context.arguments[0]))
        else {
            return Ok(CommandOutput::message("That player is not online."));
        };
        let Some(kind) = parse_item_kind(&context.arguments[1]) else {
            return Ok(CommandOutput::message("Unknown item name."));
        };
        let count = if let Some(value) = context.arguments.get(2) {
            match value.parse::<u8>() {
                Ok(count) if (1..=64).contains(&count) => count,
                _ => return Ok(CommandOutput::message("Count must be between 1 and 64.")),
            }
        } else {
            1
        };
        context.server.give_item(player.id, kind, count);
        Ok(CommandOutput::message(format!(
            "Gave {} {} x{}.",
            player.name,
            kind.as_str(),
            count
        )))
    }
}

fn parse_item_kind(value: &str) -> Option<ItemKind> {
    match value.to_ascii_lowercase().as_str() {
        "stone" => Some(ItemKind::Stone),
        "dirt" => Some(ItemKind::Dirt),
        "oak_log" | "log" => Some(ItemKind::OakLog),
        "oak_planks" | "planks" => Some(ItemKind::OakPlanks),
        "stick" => Some(ItemKind::Stick),
        "apple" => Some(ItemKind::Apple),
        "bucket" => Some(ItemKind::Bucket),
        "milk_bucket" | "milk" => Some(ItemKind::MilkBucket),
        _ => None,
    }
}

struct Op;

#[async_trait]
impl CommandHandler for Op {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(name) = single_player_name(&context.arguments) else {
            return Ok(CommandOutput::message("Usage: op <player>"));
        };
        if context.server.set_operator(name, true)? {
            Ok(CommandOutput::message(format!(
                "Made {name} a server operator."
            )))
        } else {
            Ok(CommandOutput::message(format!(
                "{name} is already a server operator."
            )))
        }
    }
}

struct Deop;

#[async_trait]
impl CommandHandler for Deop {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(name) = single_player_name(&context.arguments) else {
            return Ok(CommandOutput::message("Usage: deop <player>"));
        };
        if context.server.set_operator(name, false)? {
            Ok(CommandOutput::message(format!(
                "Revoked {name}'s operator status."
            )))
        } else {
            Ok(CommandOutput::message(format!(
                "{name} is not a server operator."
            )))
        }
    }
}

fn single_player_name(arguments: &[String]) -> Option<&str> {
    let [name] = arguments else {
        return None;
    };
    let valid = (1..=16).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    valid.then_some(name)
}

struct Stop;

struct Effect;

#[async_trait]
impl CommandHandler for Effect {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let usage =
            "Usage: effect <give <player> <effect> <seconds> [level]|clear <player> [effect]>";
        let Some(action) = context.arguments.first() else {
            return Ok(CommandOutput::message(usage));
        };
        let Some(name) = context.arguments.get(1) else {
            return Ok(CommandOutput::message(usage));
        };
        let Some(player) = context
            .server
            .players()
            .into_iter()
            .find(|player| player.name.eq_ignore_ascii_case(name))
        else {
            return Ok(CommandOutput::message("That player is not online."));
        };

        if action.eq_ignore_ascii_case("clear") {
            if context.arguments.len() == 2 {
                let effects = context.server.status_effects(player.id);
                for effect in &effects {
                    context.server.clear_status_effect(player.id, effect.kind);
                }
                return Ok(CommandOutput::message(format!(
                    "Cleared {} effect(s) from {}.",
                    effects.len(),
                    player.name
                )));
            }
            if context.arguments.len() == 3 {
                let Some(kind) = parse_status_effect(&context.arguments[2]) else {
                    return Ok(CommandOutput::message("Unknown status effect."));
                };
                let message = if context.server.clear_status_effect(player.id, kind) {
                    format!("Cleared {} from {}.", kind.as_str(), player.name)
                } else {
                    format!("{} does not have {}.", player.name, kind.as_str())
                };
                return Ok(CommandOutput::message(message));
            }
            return Ok(CommandOutput::message(usage));
        }

        if !action.eq_ignore_ascii_case("give") || !(4..=5).contains(&context.arguments.len()) {
            return Ok(CommandOutput::message(usage));
        }
        let Some(kind) = parse_status_effect(&context.arguments[2]) else {
            return Ok(CommandOutput::message("Unknown status effect."));
        };
        let Ok(seconds) = context.arguments[3].parse::<u32>() else {
            return Ok(CommandOutput::message(
                "Seconds must be between 1 and 3600.",
            ));
        };
        if !(1..=3_600).contains(&seconds) {
            return Ok(CommandOutput::message(
                "Seconds must be between 1 and 3600.",
            ));
        }
        let level = match context.arguments.get(4) {
            Some(value) => match value.parse::<u8>() {
                Ok(level @ 1..=5) => level,
                _ => return Ok(CommandOutput::message("Level must be between 1 and 5.")),
            },
            None => 1,
        };
        context
            .server
            .apply_status_effect(player.id, kind, level - 1, seconds.saturating_mul(20));
        Ok(CommandOutput::message(format!(
            "Applied {} {} to {} for {} second(s).",
            kind.as_str(),
            level,
            player.name,
            seconds
        )))
    }
}

fn parse_status_effect(value: &str) -> Option<StatusEffectKind> {
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

struct Ban;

#[async_trait]
impl CommandHandler for Ban {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(name) = context
            .arguments
            .first()
            .filter(|name| single_player_name(std::slice::from_ref(name)).is_some())
        else {
            return Ok(CommandOutput::message("Usage: ban <player> [reason]"));
        };
        let reason = if context.arguments.len() > 1 {
            context.arguments[1..].join(" ")
        } else {
            "Banned by an operator.".into()
        };
        if !valid_disconnect_reason(&reason) {
            return Ok(CommandOutput::message(
                "Ban reason must contain 1 to 256 printable characters.",
            ));
        }
        let changed = context.server.ban_player(name, &reason, None)?;
        let disconnected = context
            .server
            .players()
            .into_iter()
            .find(|player| player.name.eq_ignore_ascii_case(name))
            .is_some_and(|player| {
                context
                    .server
                    .request_disconnect(player.id, &format!("You are banned: {reason}"))
            });
        let message = match (changed, disconnected) {
            (true, true) => format!("Banned {name} and disconnected them."),
            (true, false) => format!("Banned {name}."),
            (false, true) => format!("{name} was already banned and has been disconnected."),
            (false, false) => format!("{name} is already banned."),
        };
        Ok(CommandOutput::message(message))
    }
}

struct TempBan;

#[async_trait]
impl CommandHandler for TempBan {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let [name, duration, rest @ ..] = context.arguments.as_slice() else {
            return Ok(CommandOutput::message(
                "Usage: tempban <player> <duration> [reason]",
            ));
        };
        if single_player_name(std::slice::from_ref(name)).is_none() {
            return Ok(CommandOutput::message(
                "Usage: tempban <player> <duration> [reason]",
            ));
        }
        let Some(seconds) = parse_ban_duration(duration) else {
            return Ok(CommandOutput::message(
                "Duration must be 1s to 365d, using s, m, h, d, or w.",
            ));
        };
        let reason = if rest.is_empty() {
            "Temporarily banned by an operator.".into()
        } else {
            rest.join(" ")
        };
        if !valid_disconnect_reason(&reason) {
            return Ok(CommandOutput::message(
                "Ban reason must contain 1 to 256 printable characters.",
            ));
        }
        let expires = unix_time().saturating_add(seconds);
        context.server.ban_player(name, &reason, Some(expires))?;
        let disconnected = context
            .server
            .players()
            .into_iter()
            .find(|player| player.name.eq_ignore_ascii_case(name))
            .is_some_and(|player| {
                context.server.request_disconnect(
                    player.id,
                    &format!(
                        "Temporarily banned for {}: {reason}",
                        format_duration(seconds)
                    ),
                )
            });
        let suffix = if disconnected {
            " and disconnected them"
        } else {
            ""
        };
        Ok(CommandOutput::message(format!(
            "Temporarily banned {name} for {}{suffix}.",
            format_duration(seconds)
        )))
    }
}

pub(super) fn parse_ban_duration(value: &str) -> Option<u64> {
    let normalized = value.to_ascii_lowercase();
    let (number, multiplier) = [
        ("s", 1_u64),
        ("m", 60),
        ("h", 3_600),
        ("d", 86_400),
        ("w", 604_800),
    ]
    .into_iter()
    .find_map(|(suffix, multiplier)| {
        normalized
            .strip_suffix(suffix)
            .map(|number| (number, multiplier))
    })?;
    let number = number.parse::<u64>().ok()?;
    let seconds = number.checked_mul(multiplier)?;
    (1..=31_536_000).contains(&seconds).then_some(seconds)
}

fn format_duration(seconds: u64) -> String {
    if seconds % 604_800 == 0 {
        format!("{}w", seconds / 604_800)
    } else if seconds % 86_400 == 0 {
        format!("{}d", seconds / 86_400)
    } else if seconds % 3_600 == 0 {
        format!("{}h", seconds / 3_600)
    } else if seconds % 60 == 0 {
        format!("{}m", seconds / 60)
    } else {
        format!("{seconds}s")
    }
}

fn unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

struct Kick;

#[async_trait]
impl CommandHandler for Kick {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(name) = context
            .arguments
            .first()
            .filter(|name| single_player_name(std::slice::from_ref(name)).is_some())
        else {
            return Ok(CommandOutput::message("Usage: kick <player> [reason]"));
        };
        let reason = if context.arguments.len() > 1 {
            context.arguments[1..].join(" ")
        } else {
            "Kicked by an operator.".into()
        };
        if !valid_disconnect_reason(&reason) {
            return Ok(CommandOutput::message(
                "Kick reason must contain 1 to 256 printable characters.",
            ));
        }
        let Some(player) = context
            .server
            .players()
            .into_iter()
            .find(|player| player.name.eq_ignore_ascii_case(name))
        else {
            return Ok(CommandOutput::message("That player is not online."));
        };
        if !context.server.request_disconnect(player.id, &reason) {
            return Ok(CommandOutput::message("Could not queue the disconnect."));
        }
        Ok(CommandOutput::message(format!(
            "Kicked {}: {reason}",
            player.name
        )))
    }
}

fn valid_disconnect_reason(reason: &str) -> bool {
    !reason.is_empty() && reason.chars().count() <= 256 && !reason.chars().any(char::is_control)
}

struct Pardon;

#[async_trait]
impl CommandHandler for Pardon {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(name) = single_player_name(&context.arguments) else {
            return Ok(CommandOutput::message("Usage: pardon <player>"));
        };
        let message = if context.server.set_banned(name, false)? {
            format!("Pardoned {name}.")
        } else {
            format!("{name} is not banned.")
        };
        Ok(CommandOutput::message(message))
    }
}

struct BanList;

#[async_trait]
impl CommandHandler for BanList {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        if !context.arguments.is_empty() {
            return Ok(CommandOutput::message("Usage: banlist"));
        }
        let bans = context.server.bans();
        if bans.is_empty() {
            return Ok(CommandOutput::message("There are no banned players."));
        }
        let now = unix_time();
        let mut lines = vec!["Banned players:".into()];
        lines.extend(bans.into_iter().map(|ban| {
            let expiry = ban.expires_at_unix.map_or_else(
                || "permanent".into(),
                |expires| format!("{} remaining", format_duration(expires.saturating_sub(now))),
            );
            format!("{} — {} ({expiry})", ban.name, ban.reason)
        }));
        Ok(CommandOutput { lines })
    }
}

struct Allowlist;

struct Audit;

struct GrantPermission;

#[async_trait]
impl CommandHandler for GrantPermission {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let [name, permission] = context.arguments.as_slice() else {
            return Ok(CommandOutput::message(
                "Usage: grantperm <player> <permission>",
            ));
        };
        if single_player_name(std::slice::from_ref(name)).is_none() {
            return Ok(CommandOutput::message(
                "Usage: grantperm <player> <permission>",
            ));
        }
        let changed = context.server.grant_permission(name, permission)?;
        Ok(CommandOutput::message(if changed {
            format!("Granted {permission} to {name}.")
        } else {
            format!("{name} already has {permission}.")
        }))
    }
}

struct RevokePermission;

#[async_trait]
impl CommandHandler for RevokePermission {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let [name, permission] = context.arguments.as_slice() else {
            return Ok(CommandOutput::message(
                "Usage: revokeperm <player> <permission>",
            ));
        };
        if single_player_name(std::slice::from_ref(name)).is_none() {
            return Ok(CommandOutput::message(
                "Usage: revokeperm <player> <permission>",
            ));
        }
        let changed = context.server.revoke_permission(name, permission)?;
        Ok(CommandOutput::message(if changed {
            format!("Revoked {permission} from {name}.")
        } else {
            format!("{name} does not have {permission}.")
        }))
    }
}

struct Permissions;

#[async_trait]
impl CommandHandler for Permissions {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let Some(name) = single_player_name(&context.arguments) else {
            return Ok(CommandOutput::message("Usage: permissions <player>"));
        };
        let nodes = context.server.permission_nodes(name);
        Ok(CommandOutput::message(if nodes.is_empty() {
            format!("{name} has no explicit permission nodes.")
        } else {
            format!("{name}'s permissions: {}", nodes.join(", "))
        }))
    }
}

#[async_trait]
impl CommandHandler for Audit {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        let count = match context.arguments.as_slice() {
            [] => 20,
            [value] => match value.parse::<usize>() {
                Ok(count) if (1..=100).contains(&count) => count,
                _ => return Ok(CommandOutput::message("Count must be between 1 and 100.")),
            },
            _ => return Ok(CommandOutput::message("Usage: audit [count]")),
        };
        let records = context.server.moderation_records(count);
        if records.is_empty() {
            return Ok(CommandOutput::message("The moderation audit is empty."));
        }
        Ok(CommandOutput {
            lines: records
                .into_iter()
                .map(|record| {
                    let target = record.target.unwrap_or_else(|| "-".into());
                    format!(
                        "{} {} {} {} — {}",
                        record.timestamp_unix, record.actor, record.action, target, record.detail
                    )
                })
                .collect(),
        })
    }
}

#[async_trait]
impl CommandHandler for Allowlist {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        match context.arguments.as_slice() {
            [action] if action.eq_ignore_ascii_case("list") => {
                let names = context.server.allowlisted_players();
                Ok(CommandOutput::message(if names.is_empty() {
                    "The allowlist is empty.".into()
                } else {
                    format!("Allowlisted players: {}", names.join(", "))
                }))
            }
            [action, name]
                if (action.eq_ignore_ascii_case("add")
                    || action.eq_ignore_ascii_case("remove"))
                    && single_player_name(std::slice::from_ref(name)).is_some() =>
            {
                let add = action.eq_ignore_ascii_case("add");
                let changed = context.server.set_allowlisted(name, add)?;
                let message = match (add, changed) {
                    (true, true) => format!("Added {name} to the allowlist."),
                    (true, false) => format!("{name} is already allowlisted."),
                    (false, true) => format!("Removed {name} from the allowlist."),
                    (false, false) => format!("{name} is not allowlisted."),
                };
                Ok(CommandOutput::message(message))
            }
            _ => Ok(CommandOutput::message(
                "Usage: allowlist <add|remove|list> [player]",
            )),
        }
    }
}

#[async_trait]
impl CommandHandler for Stop {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        context.server.request_shutdown();
        Ok(CommandOutput::message("Shutdown requested."))
    }
}
