use std::{env, path::PathBuf};

use anyhow::{bail, Context};
use carbon_config::CarbonConfig;
use carbon_server::CarbonServer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let arguments = Arguments::parse()?;
    let config = CarbonConfig::load(&arguments.config)
        .with_context(|| format!("failed to load {}", arguments.config.display()))?;

    if arguments.check {
        println!("Configuration is valid: {}", arguments.config.display());
        return Ok(());
    }

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.logging.level))
        .context("invalid logging filter")?;
    tracing_subscriber::fmt().with_env_filter(filter).init();

    CarbonServer::new(config).run().await
}

struct Arguments {
    config: PathBuf,
    check: bool,
}

impl Arguments {
    fn parse() -> anyhow::Result<Self> {
        let mut config = PathBuf::from("Carbon.toml");
        let mut check = false;
        let mut args = env::args().skip(1);
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--config" | "-c" => {
                    config = args
                        .next()
                        .map(PathBuf::from)
                        .context("--config requires a path")?;
                }
                "--check" => check = true,
                "--help" | "-h" => {
                    println!("Carbon server\n\nUsage: carbon [--config <path>] [--check]");
                    std::process::exit(0);
                }
                unknown => bail!("unknown argument: {unknown}"),
            }
        }
        Ok(Self { config, check })
    }
}
