pub mod commands;
mod data_lock;
mod extensions;
pub mod network;
mod runtime;
mod state;
mod world;

pub use runtime::CarbonServer;

pub const NAME: &str = "Carbon";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
