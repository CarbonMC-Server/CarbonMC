use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use carbon_api::{CommandSender, Event, ServerApi};
use carbon_config::CarbonConfig;
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, watch},
    time::{self, MissedTickBehavior},
};
use tracing::{info, warn};

use crate::{
    commands::{register_builtins, CommandRegistry},
    extensions::ExtensionManager,
    network,
    state::ServerState,
    NAME, VERSION,
};

pub struct CarbonServer {
    config: CarbonConfig,
}

impl CarbonServer {
    #[must_use]
    pub fn new(config: CarbonConfig) -> Self {
        Self { config }
    }

    pub async fn run(self) -> anyhow::Result<()> {
        self.config.validate()?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let state = Arc::new(ServerState::with_operator_file(
            self.config.world.name.clone(),
            self.config.world.seed,
            shutdown_tx,
            PathBuf::from("operators.json"),
        )?);
        let api: Arc<dyn ServerApi> = state.clone();

        let commands = CommandRegistry::default();
        register_builtins(&commands)?;

        let mut extensions = ExtensionManager::default();
        extensions.register(hello_carbon::HelloCarbon);
        extensions.load_all(&commands).await?;

        // Bind before background tasks start so duplicate instances fail immediately.
        let listener = network::bind(&self.config.server).await?;
        let (event_tx, mut event_rx) = mpsc::channel(128);
        let network_task = tokio::spawn(network::serve(
            listener,
            self.config.server.clone(),
            Arc::clone(&api),
            shutdown_rx.clone(),
        ));
        let tick_task = tokio::spawn(tick_loop(
            self.config.server.ticks_per_second,
            Arc::clone(&state),
            event_tx,
            shutdown_rx.clone(),
        ));
        let console_task = tokio::spawn(console_loop(
            commands,
            Arc::clone(&state),
            shutdown_rx.clone(),
        ));

        let signal_api = Arc::clone(&api);
        let signal_task = tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                info!("interrupt received; requesting shutdown");
                signal_api.request_shutdown();
            }
        });

        info!(server = NAME, version = VERSION, "server started");
        extensions.emit(&Event::ServerStarted).await;

        let mut shutdown = shutdown_rx;
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                event = event_rx.recv() => {
                    if let Some(event) = event {
                        extensions.emit(&event).await;
                    }
                }
            }
        }

        info!("server stopping");
        extensions.emit(&Event::ServerStopping).await;
        extensions.disable_all().await;
        // Drain connection cleanup (including inventory cursors) before the final snapshot.
        network_task.await.context("network task panicked")??;
        tick_task.await.context("tick task panicked")?;
        console_task.await.context("console task panicked")?;
        state
            .save()
            .context("failed to save world during shutdown")?;

        signal_task.abort();
        info!("server stopped cleanly");
        Ok(())
    }
}

async fn tick_loop(
    ticks_per_second: u16,
    state: Arc<ServerState>,
    events: mpsc::Sender<Event>,
    mut shutdown: watch::Receiver<bool>,
) {
    let period = Duration::from_secs_f64(1.0 / f64::from(ticks_per_second));
    let mut interval = time::interval(period);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let started = Instant::now();
                let tick = state.advance_tick();
                let _ = events.try_send(Event::Tick { number: tick });
                let elapsed = started.elapsed();
                if elapsed > period {
                    warn!(tick, elapsed_ms = elapsed.as_secs_f64() * 1000.0, "tick exceeded its time budget");
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}

async fn console_loop(
    commands: CommandRegistry,
    server: Arc<ServerState>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut repair_confirmation = false;
    loop {
        tokio::select! {
            result = lines.next_line() => {
                match result {
                    Ok(Some(line)) => {
                        let answer = line.trim().to_ascii_lowercase();
                        if repair_confirmation {
                            match answer.as_str() {
                                "y" | "yes" => {
                                    repair_confirmation = false;
                                    match server.repair_world() {
                                        Ok(()) => info!(target: "carbon::command", "World reset complete. Connected players are being disconnected; they may rejoin immediately."),
                                        Err(error) => warn!(target: "carbon::command", %error, "World reset failed"),
                                    }
                                }
                                "n" | "no" => {
                                    repair_confirmation = false;
                                    info!(target: "carbon::command", "World reset cancelled.");
                                }
                                _ => info!(target: "carbon::command", "Confirm reset world? [y/n]"),
                            }
                            continue;
                        }
                        if answer.trim_start_matches('/') == "repair" {
                            repair_confirmation = true;
                            warn!(target: "carbon::command", "Confirm reset world? [y/n]");
                            continue;
                        }
                        let command_server: Arc<dyn ServerApi> = server.clone();
                        let output = commands.execute(&line, CommandSender::Console, command_server).await;
                        for line in output.lines {
                            info!(target: "carbon::command", "{line}");
                        }
                    }
                    Ok(None) => break,
                    Err(error) => {
                        warn!(%error, "could not read console input");
                        break;
                    }
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
        }
    }
}
