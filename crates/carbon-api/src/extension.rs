use async_trait::async_trait;

use crate::{CommandRegistration, Event, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
}

impl ExtensionMetadata {
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: String::new(),
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

/// Registration surface available while an extension is loaded.
#[derive(Default)]
pub struct ExtensionContext {
    commands: Vec<CommandRegistration>,
}

impl ExtensionContext {
    pub fn register_command(&mut self, command: CommandRegistration) {
        self.commands.push(command);
    }

    #[doc(hidden)]
    #[must_use]
    pub fn take_commands(&mut self) -> Vec<CommandRegistration> {
        std::mem::take(&mut self.commands)
    }
}

/// Lifecycle contract for a statically linked Carbon extension.
#[async_trait]
pub trait Extension: Send + Sync + 'static {
    fn metadata(&self) -> ExtensionMetadata;

    async fn on_load(&mut self, _context: &mut ExtensionContext) -> Result<()> {
        Ok(())
    }

    async fn on_enable(&mut self) -> Result<()> {
        Ok(())
    }

    async fn on_event(&mut self, _event: &Event) -> Result<()> {
        Ok(())
    }

    async fn on_disable(&mut self) -> Result<()> {
        Ok(())
    }
}
