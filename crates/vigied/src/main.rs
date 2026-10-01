//! `vigied` : démon de supervision qui tourne sur le companion computer.
//!
//! Lit l'état du véhicule via MAVLink, évalue geofence et failsafes à 10 Hz,
//! et envoie RTL / LAND à l'autopilote quand une règle se déclenche.

mod config;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use vigie_core::{Action, FailsafeMonitor, FenceStatus, Geofence, Inputs};
use vigie_mavlink::{Command, VehicleState};

#[derive(Debug, Parser)]
#[command(version, about)]
struct Cli {
    /// Fichier de configuration TOML.
    #[arg(short, long, default_value = "/etc/vigie/vigie.toml")]
    config: PathBuf,
    /// Évalue et journalise les décisions sans envoyer de commande à l'autopilote.
    #[arg(long)]
    dry_run: bool,
}

const TICK: Duration = Duration::from_millis(100);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    let cfg = config::Config::load(&cli.config)
        .with_context(|| format!("lecture de {}", cli.config.display()))?;

    let fence = cfg.geofence()?;
    let monitor = FailsafeMonitor::new(cfg.failsafe.into());
    tracing::info!(
        vertices = cfg.geofence.vertices.len(),
        ceiling_m = cfg.geofence.ceiling_m,
        dry_run = cli.dry_run,
        "vigied démarre"
    );

    let epoch = Instant::now();
    let (state_tx, state_rx) = watch::channel(VehicleState::default());
    let (cmd_tx, cmd_rx) = mpsc::channel(8);

    let bridge = tokio::spawn({
        let addr = cfg.mavlink.address.clone();
        async move { vigie_mavlink::run(&addr, epoch, state_tx, cmd_rx).await }
    });
    let supervisor = tokio::spawn(supervise(
        epoch,
        fence,
        monitor,
        state_rx,
        cmd_tx,
        cli.dry_run,
    ));

    tokio::select! {
        r = bridge => r?.context("pont MAVLink")?,
        r = supervisor => r?,
        _ = tokio::signal::ctrl_c() => tracing::info!("arrêt demandé"),
    }
    Ok(())
}

async fn supervise(
    epoch: Instant,
    fence: Geofence,
    mut monitor: FailsafeMonitor,
    state_rx: watch::Receiver<VehicleState>,
    cmd_tx: mpsc::Sender<Command>,
    dry_run: bool,
) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let now_ms = u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        let s = state_rx.borrow().clone();

        let fence_status = match s.position {
            Some(p) => fence.check(p, s.rel_alt_m),
            // Pas encore de fix : rien à juger côté geofence.
            None => FenceStatus::Inside {
                margin_m: f32::INFINITY,
            },
        };
        let inputs = Inputs {
            now_ms,
            armed: s.armed,
            armed_since_ms: s.armed_since_ms,
            last_link_ms: s.last_gcs_ms,
            battery_pct: s.battery_pct,
            fence: fence_status,
        };

        let cmd = match monitor.update(&inputs) {
            Action::None => None,
            Action::Warn(reason) => {
                tracing::warn!(?reason, "alerte");
                None
            }
            Action::ReturnToLaunch(reason) => {
                tracing::error!(?reason, "failsafe : retour au point de décollage");
                Some(Command::ReturnToLaunch)
            }
            Action::Land(reason) => {
                tracing::error!(?reason, "failsafe : atterrissage immédiat");
                Some(Command::Land)
            }
        };
        if let Some(cmd) = cmd
            && !dry_run
            && cmd_tx.send(cmd).await.is_err()
        {
            tracing::error!("pont MAVLink arrêté, supervision interrompue");
            return;
        }
    }
}
