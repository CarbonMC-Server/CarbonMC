use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use carbon_api::{CommandContext, CommandOutput, CommandRegistration, CommandSender, ServerApi};

mod builtins;

#[derive(Clone, Default)]
pub struct CommandRegistry {
    commands: Arc<RwLock<HashMap<String, Arc<CommandRegistration>>>>,
}

impl CommandRegistry {
    pub fn register(&self, command: CommandRegistration) -> anyhow::Result<()> {
        let name = normalize_label(&command.name);
        if name.is_empty() || name.contains(char::is_whitespace) {
            anyhow::bail!("command name must be one non-empty word");
        }
        let mut commands = self
            .commands
            .write()
            .unwrap_or_else(|error| error.into_inner());
        if commands.contains_key(&name) {
            anyhow::bail!("command '{name}' is already registered");
        }
        commands.insert(name, Arc::new(command));
        Ok(())
    }

    pub fn descriptions(&self) -> Vec<(String, String, String)> {
        self.filtered_descriptions(|_| true)
    }

    pub fn descriptions_for(
        &self,
        sender: &CommandSender,
        server: &dyn ServerApi,
    ) -> Vec<(String, String, String)> {
        self.filtered_descriptions(|command| command_allowed(command, sender, server))
    }

    fn filtered_descriptions(
        &self,
        include: impl Fn(&CommandRegistration) -> bool,
    ) -> Vec<(String, String, String)> {
        let commands = self
            .commands
            .read()
            .unwrap_or_else(|error| error.into_inner());
        let mut descriptions: Vec<_> = commands
            .values()
            .filter(|command| include(command))
            .map(|command| {
                (
                    command.name.clone(),
                    command.description.clone(),
                    command.usage.clone(),
                )
            })
            .collect();
        descriptions.sort_by(|left, right| left.0.cmp(&right.0));
        descriptions
    }

    pub async fn execute(
        &self,
        input: &str,
        sender: CommandSender,
        server: Arc<dyn ServerApi>,
    ) -> CommandOutput {
        let words = match parse_words(input.trim().trim_start_matches('/')) {
            Ok(words) => words,
            Err(message) => return CommandOutput::message(message),
        };
        let Some((label, arguments)) = words.split_first() else {
            return CommandOutput::default();
        };
        let command = {
            self.commands
                .read()
                .unwrap_or_else(|error| error.into_inner())
                .get(&normalize_label(label))
                .cloned()
        };
        let Some(command) = command else {
            return CommandOutput::message(format!(
                "Unknown command '{label}'. Type 'help' for available commands."
            ));
        };
        let actor = match &sender {
            CommandSender::Console => "console".to_owned(),
            CommandSender::Player { name } => name.clone(),
        };
        let target = arguments.first().cloned();
        if !command_allowed(&command, &sender, server.as_ref()) {
            if let Err(error) = server.record_moderation(
                &actor,
                &format!("denied:{}", command.name),
                target.as_deref(),
                "Permission denied.",
            ) {
                tracing::warn!(%error, "could not write moderation audit record");
            }
            return CommandOutput::message("You do not have permission to use this command.");
        }
        let audited = command.operator_only || command.permission.is_some();
        let action = command.name.clone();
        let context = CommandContext {
            sender,
            label: label.clone(),
            arguments: arguments.to_vec(),
            server: Arc::clone(&server),
        };
        let output = match command.handler.execute(context).await {
            Ok(output) => output,
            Err(error) => CommandOutput::message(format!("Command failed: {error}")),
        };
        if audited {
            let detail = if output.lines.is_empty() {
                "Command completed without output.".into()
            } else {
                output.lines.join(" | ")
            };
            if let Err(error) =
                server.record_moderation(&actor, &action, target.as_deref(), &detail)
            {
                tracing::warn!(%error, "could not write moderation audit record");
            }
        }
        output
    }
}

pub fn register_builtins(registry: &CommandRegistry) -> anyhow::Result<()> {
    builtins::register(registry)
}

fn command_allowed(
    command: &CommandRegistration,
    sender: &CommandSender,
    server: &dyn ServerApi,
) -> bool {
    match sender {
        CommandSender::Console => true,
        CommandSender::Player { name } => {
            (!command.operator_only || server.is_operator(name))
                && command
                    .permission
                    .as_deref()
                    .map_or(true, |permission| server.has_permission(name, permission))
        }
    }
}

fn normalize_label(label: &str) -> String {
    label.to_ascii_lowercase()
}

fn parse_words(input: &str) -> Result<Vec<String>, &'static str> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            word.push(character);
            escaped = false;
        } else {
            match character {
                '\\' => escaped = true,
                '"' => quoted = !quoted,
                character if character.is_whitespace() && !quoted => {
                    if !word.is_empty() {
                        words.push(std::mem::take(&mut word));
                    }
                }
                _ => word.push(character),
            }
        }
    }
    if escaped || quoted {
        return Err("Unclosed quote or escape in command.");
    }
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ServerState;
    use tokio::sync::watch;

    #[test]
    fn parser_preserves_quoted_arguments() {
        assert_eq!(
            parse_words("say \"hello world\" now"),
            Ok(vec!["say".into(), "hello world".into(), "now".into()])
        );
    }

    #[tokio::test]
    async fn help_visibility_tracks_permissions_without_auditing_hidden_commands() {
        struct Probe;
        #[async_trait::async_trait]
        impl carbon_api::CommandHandler for Probe {
            async fn execute(&self, _: CommandContext) -> carbon_api::Result<CommandOutput> {
                Ok(CommandOutput::default())
            }
        }
        let server: Arc<dyn ServerApi> =
            Arc::new(ServerState::new("world".into(), 0, watch::channel(false).0));
        let registry = CommandRegistry::default();
        register_builtins(&registry).unwrap();
        registry
            .register(
                CommandRegistration::new(
                    "operator_probe",
                    "Restricted test command",
                    "operator_probe",
                    Probe,
                )
                .operator_only(),
            )
            .unwrap();
        let sender = CommandSender::Player {
            name: "Helper".into(),
        };
        let visible = |label: &str| {
            registry
                .descriptions_for(&sender, server.as_ref())
                .iter()
                .any(|(name, _, _)| name == label)
        };
        assert!(visible("help"));
        assert!(!visible("say"));
        assert!(!visible("operator_probe"));
        let audit_count = server.moderation_records(100).len();
        let help = registry
            .execute("help", sender.clone(), Arc::clone(&server))
            .await;
        assert!(help.lines.iter().any(|line| line.starts_with("help ")));
        assert!(!help.lines.iter().any(|line| line.starts_with("say ")));
        assert_eq!(server.moderation_records(100).len(), audit_count);
        server
            .grant_permission("helper", "carbon.command.*")
            .unwrap();
        assert!(visible("say"));
        assert!(!visible("operator_probe"));
        server
            .grant_permission("helper", "!carbon.command.say")
            .unwrap();
        assert!(!visible("say"));
        server
            .revoke_permission("helper", "!carbon.command.say")
            .unwrap();
        assert!(visible("say"));
        server
            .revoke_permission("helper", "carbon.command.*")
            .unwrap();
        assert!(!visible("say"));
        server.grant_permission("helper", "!*").unwrap();
        server.set_operator("Helper", true).unwrap();
        assert!(visible("say"));
        assert!(visible("operator_probe"));
        server.set_operator("Helper", false).unwrap();
        assert!(!visible("operator_probe"));
        assert_eq!(
            registry.descriptions_for(&CommandSender::Console, server.as_ref()),
            registry.descriptions()
        );
        let names: Vec<_> = registry
            .descriptions_for(&sender, server.as_ref())
            .into_iter()
            .map(|row| row.0)
            .collect();
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn parser_rejects_unclosed_quotes() {
        assert!(parse_words("say \"hello").is_err());
    }

    #[test]
    fn temporary_ban_durations_are_bounded_and_normalized() {
        assert_eq!(builtins::parse_ban_duration("30s"), Some(30));
        assert_eq!(builtins::parse_ban_duration("10M"), Some(600));
        assert_eq!(builtins::parse_ban_duration("2h"), Some(7_200));
        assert_eq!(builtins::parse_ban_duration("7d"), Some(604_800));
        assert_eq!(builtins::parse_ban_duration("2w"), Some(1_209_600));
        assert_eq!(builtins::parse_ban_duration("0s"), None);
        assert_eq!(builtins::parse_ban_duration("366d"), None);
        assert_eq!(builtins::parse_ban_duration("forever"), None);
        assert_eq!(builtins::parse_ban_duration("🔥"), None);
    }

    #[tokio::test]
    async fn operator_commands_grant_case_insensitive_admin_permissions() {
        let (shutdown, _) = watch::channel(false);
        let concrete = Arc::new(ServerState::new("world".into(), 0, shutdown));
        let server: Arc<dyn ServerApi> = concrete.clone();
        let registry = CommandRegistry::default();
        register_builtins(&registry).unwrap();

        let denied = registry
            .execute(
                "spawnmob cow",
                CommandSender::Player {
                    name: "CarbonTest".into(),
                },
                Arc::clone(&server),
            )
            .await;
        assert_eq!(
            denied,
            CommandOutput::message("You do not have permission to use this command.")
        );

        let granted = registry
            .execute("op CarbonTest", CommandSender::Console, Arc::clone(&server))
            .await;
        assert_eq!(
            granted,
            CommandOutput::message("Made CarbonTest a server operator.")
        );
        assert!(server.is_operator("carbontest"));

        let initial_mobs = server.mobs().len();
        let allowed = registry
            .execute(
                "spawnmob cow",
                CommandSender::Player {
                    name: "CARBONTEST".into(),
                },
                Arc::clone(&server),
            )
            .await;
        assert!(allowed.lines[0].starts_with("Spawned cow"));
        assert_eq!(server.mobs().len(), initial_mobs + 1);
    }

    #[tokio::test]
    async fn granular_permissions_delegate_only_the_granted_command() {
        let (shutdown, _) = watch::channel(false);
        let concrete = Arc::new(ServerState::new("world".into(), 0, shutdown));
        let server: Arc<dyn ServerApi> = concrete;
        let registry = CommandRegistry::default();
        register_builtins(&registry).unwrap();
        let helper = CommandSender::Player {
            name: "Helper".into(),
        };

        let denied = registry
            .execute("spawnmob cow", helper.clone(), Arc::clone(&server))
            .await;
        assert_eq!(
            denied,
            CommandOutput::message("You do not have permission to use this command.")
        );
        registry
            .execute(
                "grantperm Helper carbon.command.spawnmob",
                CommandSender::Console,
                Arc::clone(&server),
            )
            .await;
        let allowed = registry
            .execute("spawnmob cow", helper.clone(), Arc::clone(&server))
            .await;
        assert!(allowed.lines[0].starts_with("Spawned cow"));
        let still_denied = registry
            .execute("stop", helper.clone(), Arc::clone(&server))
            .await;
        assert_eq!(
            still_denied,
            CommandOutput::message("You do not have permission to use this command.")
        );
        let cannot_escalate = registry
            .execute("grantperm Helper *", helper, Arc::clone(&server))
            .await;
        assert_eq!(
            cannot_escalate,
            CommandOutput::message("You do not have permission to use this command.")
        );
    }

    #[tokio::test]
    async fn negative_permissions_block_commands_and_self_escalation_with_audit() {
        let server: Arc<dyn ServerApi> =
            Arc::new(ServerState::new("world".into(), 0, watch::channel(false).0));
        let registry = CommandRegistry::default();
        register_builtins(&registry).unwrap();
        let helper = CommandSender::Player {
            name: "Helper".into(),
        };
        for node in [
            "carbon.command.*",
            "!carbon.command.spawnmob",
            "!carbon.command.permissions",
        ] {
            registry
                .execute(
                    &format!("grantperm Helper {node}"),
                    CommandSender::Console,
                    Arc::clone(&server),
                )
                .await;
        }
        let listed = registry
            .execute(
                "permissions Helper",
                CommandSender::Console,
                Arc::clone(&server),
            )
            .await;
        assert!(listed.lines[0].contains("!carbon.command.spawnmob"));
        let initial_mobs = server.mobs().len();
        for input in [
            "spawnmob cow",
            "grantperm Helper *",
            "revokeperm Helper !carbon.command.permissions",
        ] {
            assert_eq!(
                registry
                    .execute(input, helper.clone(), Arc::clone(&server))
                    .await,
                CommandOutput::message("You do not have permission to use this command.")
            );
        }
        assert_eq!(server.mobs().len(), initial_mobs);
        assert!(server
            .moderation_records(20)
            .iter()
            .any(|record| record.action == "denied:spawnmob"));
        registry
            .execute(
                "revokeperm Helper !carbon.command.spawnmob",
                CommandSender::Console,
                Arc::clone(&server),
            )
            .await;
        let allowed = registry
            .execute("spawnmob cow", helper.clone(), Arc::clone(&server))
            .await;
        assert!(allowed.lines[0].starts_with("Spawned cow"));
        server.grant_permission("Helper", "!*").unwrap();
        server.set_operator("Helper", true).unwrap();
        assert!(registry
            .execute("spawnmob cow", helper, Arc::clone(&server))
            .await
            .lines[0]
            .starts_with("Spawned cow"));
        server.grant_permission("console", "!*").unwrap();
        assert!(registry
            .execute("spawnmob cow", CommandSender::Console, server)
            .await
            .lines[0]
            .starts_with("Spawned cow"));
    }

    #[tokio::test]
    async fn access_control_commands_validate_and_update_names() {
        let (shutdown, _) = watch::channel(false);
        let concrete = Arc::new(ServerState::new("world".into(), 0, shutdown));
        let server: Arc<dyn ServerApi> = concrete;
        let registry = CommandRegistry::default();
        register_builtins(&registry).unwrap();

        assert_eq!(
            registry
                .execute("ban Bad-Name", CommandSender::Console, Arc::clone(&server))
                .await,
            CommandOutput::message("Usage: ban <player> [reason]")
        );
        registry
            .execute("ban Griefer", CommandSender::Console, Arc::clone(&server))
            .await;
        registry
            .execute(
                "allowlist add Builder",
                CommandSender::Console,
                Arc::clone(&server),
            )
            .await;
        assert!(server.is_banned("griefer"));
        assert!(server.is_allowlisted("BUILDER"));
        let listed = registry
            .execute("banlist", CommandSender::Console, Arc::clone(&server))
            .await;
        assert_eq!(
            listed,
            CommandOutput {
                lines: vec![
                    "Banned players:".into(),
                    "Griefer — Banned by an operator. (permanent)".into(),
                ],
            }
        );

        let temporary = registry
            .execute(
                "tempban Visitor 2h Repeated spam",
                CommandSender::Console,
                Arc::clone(&server),
            )
            .await;
        assert_eq!(
            temporary,
            CommandOutput::message("Temporarily banned Visitor for 2h.")
        );
        let visitor = server.active_ban("visitor").unwrap();
        assert_eq!(visitor.reason, "Repeated spam");
        assert!(visitor.expires_at_unix.is_some());
    }

    #[tokio::test]
    async fn kick_and_ban_queue_live_disconnects() {
        let (shutdown, _) = watch::channel(false);
        let concrete = Arc::new(ServerState::new("world".into(), 0, shutdown));
        let id = uuid::Uuid::new_v4();
        assert!(concrete.add_player(carbon_api::PlayerSnapshot {
            id,
            name: "Target".into(),
            world: "world".into(),
            position: carbon_api::BlockPosition::default(),
            game_mode: carbon_api::GameMode::Survival,
        }));
        let server: Arc<dyn ServerApi> = concrete;
        let registry = CommandRegistry::default();
        register_builtins(&registry).unwrap();

        let kicked = registry
            .execute(
                "kick target Testing live kick",
                CommandSender::Console,
                Arc::clone(&server),
            )
            .await;
        assert_eq!(
            kicked,
            CommandOutput::message("Kicked Target: Testing live kick")
        );
        assert_eq!(server.disconnects_since(0)[0].player_id, id);

        let banned = registry
            .execute("ban TARGET", CommandSender::Console, Arc::clone(&server))
            .await;
        assert_eq!(
            banned,
            CommandOutput::message("Banned TARGET and disconnected them.")
        );
        assert!(server.is_banned("target"));
        assert_eq!(server.disconnects_since(0).len(), 2);
        let records = server.moderation_records(10);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].action, "kick");
        assert_eq!(records[1].action, "ban");

        let audit = registry
            .execute("audit 2", CommandSender::Console, Arc::clone(&server))
            .await;
        assert_eq!(audit.lines.len(), 2);
        assert!(audit.lines[0].contains(" kick target "));
        assert!(audit.lines[1].contains(" ban TARGET "));
        assert_eq!(
            server.moderation_records(10).last().unwrap().action,
            "audit"
        );
    }
}
