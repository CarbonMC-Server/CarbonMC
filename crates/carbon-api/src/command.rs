use std::sync::Arc;

use async_trait::async_trait;

use crate::{Result, ServerApi};

/// Origin of a command invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandSender {
    Console,
    Player { name: String },
}

/// Immutable data made available to a command handler.
pub struct CommandContext {
    pub sender: CommandSender,
    pub label: String,
    pub arguments: Vec<String>,
    pub server: Arc<dyn ServerApi>,
}

/// Lines returned to the command sender.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CommandOutput {
    pub lines: Vec<String>,
}

impl CommandOutput {
    #[must_use]
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            lines: vec![message.into()],
        }
    }
}

#[async_trait]
pub trait CommandHandler: Send + Sync + 'static {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput>;
}

/// A command contributed by the server or an extension.
pub struct CommandRegistration {
    pub name: String,
    pub description: String,
    pub usage: String,
    pub operator_only: bool,
    pub permission: Option<String>,
    pub handler: Arc<dyn CommandHandler>,
}

impl CommandRegistration {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        usage: impl Into<String>,
        handler: impl CommandHandler,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            usage: usage.into(),
            operator_only: false,
            permission: None,
            handler: Arc::new(handler),
        }
    }

    /// Restricts this command to the console and players granted operator status.
    #[must_use]
    pub const fn operator_only(mut self) -> Self {
        self.operator_only = true;
        self
    }

    /// Requires one permission node; operators and the console bypass checks.
    #[must_use]
    pub fn requires_permission(mut self, permission: impl Into<String>) -> Self {
        self.permission = Some(permission.into());
        self
    }
}
