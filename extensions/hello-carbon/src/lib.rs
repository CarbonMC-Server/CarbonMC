use async_trait::async_trait;
use carbon_api::{
    CommandContext, CommandHandler, CommandOutput, CommandRegistration, Extension,
    ExtensionContext, ExtensionMetadata, Result,
};

/// Minimal extension demonstrating lifecycle and command registration.
pub struct HelloCarbon;

#[async_trait]
impl Extension for HelloCarbon {
    fn metadata(&self) -> ExtensionMetadata {
        ExtensionMetadata::new("hello-carbon", env!("CARGO_PKG_VERSION"))
            .with_description("Example Carbon extension")
    }

    async fn on_load(&mut self, context: &mut ExtensionContext) -> Result<()> {
        context.register_command(CommandRegistration::new(
            "hello",
            "Greets the command sender",
            "hello",
            HelloCommand,
        ));
        Ok(())
    }
}

struct HelloCommand;

#[async_trait]
impl CommandHandler for HelloCommand {
    async fn execute(&self, context: CommandContext) -> Result<CommandOutput> {
        Ok(CommandOutput::message(format!(
            "Hello from Carbon at tick {}!",
            context.server.current_tick()
        )))
    }
}
