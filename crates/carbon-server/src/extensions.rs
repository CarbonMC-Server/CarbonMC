use carbon_api::{Event, Extension, ExtensionContext};
use tracing::{error, info};

use crate::commands::CommandRegistry;

#[derive(Default)]
pub struct ExtensionManager {
    extensions: Vec<Box<dyn Extension>>,
}

impl ExtensionManager {
    pub fn register(&mut self, extension: impl Extension) {
        self.extensions.push(Box::new(extension));
    }

    pub async fn load_all(&mut self, commands: &CommandRegistry) -> anyhow::Result<()> {
        for extension in &mut self.extensions {
            let metadata = extension.metadata();
            let mut context = ExtensionContext::default();
            extension.on_load(&mut context).await.map_err(|error| {
                anyhow::anyhow!("extension {} failed to load: {error}", metadata.name)
            })?;
            for command in context.take_commands() {
                commands.register(command)?;
            }
            extension.on_enable().await.map_err(|error| {
                anyhow::anyhow!("extension {} failed to enable: {error}", metadata.name)
            })?;
            info!(extension = %metadata.name, version = %metadata.version, "extension enabled");
        }
        Ok(())
    }

    pub async fn emit(&mut self, event: &Event) {
        for extension in &mut self.extensions {
            if let Err(error) = extension.on_event(event).await {
                error!(extension = %extension.metadata().name, %error, "extension event handler failed");
            }
        }
    }

    pub async fn disable_all(&mut self) {
        for extension in self.extensions.iter_mut().rev() {
            if let Err(error) = extension.on_disable().await {
                error!(extension = %extension.metadata().name, %error, "extension failed to disable cleanly");
            }
        }
    }
}
