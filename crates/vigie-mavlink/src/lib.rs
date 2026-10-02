//! Pont MAVLink entre Vigie et l'autopilote (PX4).
//!
//! Deux parties bien séparées :
//! - [`apply`] et [`command_message`] : conversions pures MAVLink ↔ types Vigie,
//!   testables sans réseau ;
//! - [`run`] : la boucle async qui lit la liaison, publie l'état du véhicule
//!   sur un `watch` et envoie les commandes reçues sur un `mpsc`.

use std::sync::Arc;
use std::time::Duration;

use mavlink::dialects::common::{
    COMMAND_ACK_DATA, COMMAND_LONG_DATA, HEARTBEAT_DATA, MavAutopilot, MavCmd, MavComponent,
    MavLandedState, MavMessage, MavModeFlag, MavResult, MavState, MavType,
};
use mavlink::{AsyncMavConnection, MavHeader};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use vigie_core::GeoPoint;

/// Identité de Vigie sur le réseau MAVLink : même système que l'autopilote,
/// composant « onboard computer ».
pub const COMPONENT_ID: u8 = MavComponent::MAV_COMP_ID_ONBOARD_COMPUTER as u8;

/// Ce que Vigie sait du véhicule à un instant donné.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VehicleState {
    /// System ID de l'autopilote, appris au premier heartbeat.
    pub system_id: Option<u8>,
    pub armed: bool,
    /// Instant (ms depuis le démarrage de Vigie) où l'armement a été vu.
    pub armed_since_ms: u64,
    /// `None` tant que `EXTENDED_SYS_STATE` n'a pas été reçu.
    pub in_air: Option<bool>,
    pub position: Option<GeoPoint>,
    pub rel_alt_m: f32,
    pub battery_pct: Option<u8>,
    pub last_autopilot_ms: Option<u64>,
    /// Dernier heartbeat d'une station sol (QGroundControl ou la station Vigie).
    pub last_gcs_ms: Option<u64>,
    /// Mode de vol annoncé par PX4 dans son heartbeat.
    pub flight_mode: Option<FlightMode>,
}

/// Mode de vol PX4, décodé depuis le champ `custom_mode` du heartbeat
/// (octet 2 : mode principal, octet 3 : sous-mode des modes AUTO).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightMode {
    Manual,
    Altitude,
    Position,
    Stabilized,
    Acro,
    Offboard,
    Takeoff,
    Hold,
    Mission,
    ReturnToLaunch,
    Land,
    Other { main: u8, sub: u8 },
}

impl FlightMode {
    #[must_use]
    pub fn from_px4(custom_mode: u32) -> Self {
        let [_, _, main, sub] = custom_mode.to_le_bytes();
        match (main, sub) {
            (1, _) => Self::Manual,
            (2, _) => Self::Altitude,
            (3, _) => Self::Position,
            (5, _) => Self::Acro,
            (6, _) => Self::Offboard,
            (7, _) => Self::Stabilized,
            (4, 2) => Self::Takeoff,
            (4, 3) => Self::Hold,
            (4, 4) => Self::Mission,
            (4, 5) => Self::ReturnToLaunch,
            (4, 6 | 9) => Self::Land,
            (main, sub) => Self::Other { main, sub },
        }
    }
}

/// Commandes que Vigie peut envoyer à l'autopilote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    ReturnToLaunch,
    Land,
}

#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    #[error("connexion MAVLink impossible sur {addr}: {source}")]
    Connect {
        addr: String,
        #[source]
        source: std::io::Error,
    },
    #[error("lecture MAVLink: {0}")]
    Read(#[from] mavlink::error::MessageReadError),
}

/// Met à jour `state` avec un message reçu. Fonction pure (le temps est passé en paramètre).
pub fn apply(state: &mut VehicleState, header: &MavHeader, msg: &MavMessage, now_ms: u64) {
    match msg {
        MavMessage::HEARTBEAT(hb) if hb.mavtype == MavType::MAV_TYPE_GCS => {
            state.last_gcs_ms = Some(now_ms);
        }
        MavMessage::HEARTBEAT(hb) if hb.autopilot != MavAutopilot::MAV_AUTOPILOT_INVALID => {
            state.system_id = Some(header.system_id);
            state.last_autopilot_ms = Some(now_ms);
            let armed = hb
                .base_mode
                .contains(MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED);
            if armed && !state.armed {
                state.armed_since_ms = now_ms;
            }
            state.armed = armed;
            if hb.autopilot == MavAutopilot::MAV_AUTOPILOT_PX4
                && hb
                    .base_mode
                    .contains(MavModeFlag::MAV_MODE_FLAG_CUSTOM_MODE_ENABLED)
            {
                state.flight_mode = Some(FlightMode::from_px4(hb.custom_mode));
            }
        }
        MavMessage::GLOBAL_POSITION_INT(p) => {
            state.position = Some(GeoPoint::from_e7(p.lat, p.lon));
            state.rel_alt_m = p.relative_alt as f32 / 1000.0;
        }
        MavMessage::SYS_STATUS(s) => {
            // -1 = l'autopilote ne sait pas estimer la charge.
            state.battery_pct = u8::try_from(s.battery_remaining).ok();
        }
        MavMessage::EXTENDED_SYS_STATE(e) => {
            state.in_air = match e.landed_state {
                MavLandedState::MAV_LANDED_STATE_UNDEFINED => None,
                MavLandedState::MAV_LANDED_STATE_ON_GROUND => Some(false),
                _ => Some(true),
            };
        }
        _ => {}
    }
}

/// Construit le `COMMAND_LONG` correspondant à une commande Vigie.
#[must_use]
pub fn command_message(cmd: Command, target_system: u8) -> MavMessage {
    let command = match cmd {
        Command::ReturnToLaunch => MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH,
        // Param 5/6 à 0 : atterrissage à la position courante.
        Command::Land => MavCmd::MAV_CMD_NAV_LAND,
    };
    MavMessage::COMMAND_LONG(COMMAND_LONG_DATA {
        command,
        target_system,
        target_component: MavComponent::MAV_COMP_ID_AUTOPILOT1 as u8,
        ..COMMAND_LONG_DATA::default()
    })
}

fn heartbeat() -> MavMessage {
    MavMessage::HEARTBEAT(HEARTBEAT_DATA {
        custom_mode: 0,
        mavtype: MavType::MAV_TYPE_ONBOARD_CONTROLLER,
        autopilot: MavAutopilot::MAV_AUTOPILOT_INVALID,
        base_mode: MavModeFlag::empty(),
        system_status: MavState::MAV_STATE_ACTIVE,
        mavlink_version: 3,
    })
}

type Conn = Arc<dyn AsyncMavConnection<MavMessage> + Sync + Send>;

/// Boucle principale du pont. Rend la main sur erreur de connexion ou quand
/// le canal de commandes est fermé.
///
/// `addr` suit la syntaxe de la crate `mavlink` (`udpin:0.0.0.0:14540` pour
/// écouter le port « onboard » de PX4 SITL).
pub async fn run(
    addr: &str,
    epoch: Instant,
    state_tx: watch::Sender<VehicleState>,
    mut cmd_rx: mpsc::Receiver<Command>,
) -> Result<(), BridgeError> {
    let conn: Conn = Arc::from(mavlink::connect_async::<MavMessage>(addr).await.map_err(
        |source| BridgeError::Connect {
            addr: addr.to_owned(),
            source,
        },
    )?);
    tracing::info!(addr, "liaison MAVLink ouverte");

    let mut seq: u8 = 0;
    let mut hb_tick = tokio::time::interval(Duration::from_secs(1));
    let now_ms = || u64::try_from(epoch.elapsed().as_millis()).unwrap_or(u64::MAX);

    loop {
        tokio::select! {
            res = conn.recv() => {
                let (header, msg) = res?;
                if let MavMessage::COMMAND_ACK(ack) = &msg {
                    log_ack(ack);
                }
                let mut armed_change = None;
                let mut mode_change = None;
                state_tx.send_modify(|s| {
                    let (known, was_armed, old_mode) = (s.system_id.is_some(), s.armed, s.flight_mode);
                    apply(s, &header, &msg, now_ms());
                    if !known && let Some(sys) = s.system_id {
                        tracing::info!(system_id = sys, "autopilote détecté");
                    }
                    if s.armed != was_armed {
                        armed_change = Some(s.armed);
                    }
                    if s.flight_mode != old_mode {
                        mode_change = s.flight_mode;
                    }
                });
                if let Some(mode) = mode_change {
                    tracing::info!(?mode, "mode de vol PX4");
                }
                match armed_change {
                    Some(true) => tracing::info!("drone armé"),
                    Some(false) => tracing::info!("drone désarmé"),
                    None => {}
                }
            }
            _ = hb_tick.tick() => {
                let sys = state_tx.borrow().system_id;
                // Pas de heartbeat tant qu'on ne connaît pas le système : PX4 répond
                // à l'adresse source du premier paquet reçu, on ne parle qu'après.
                if let Some(sys) = sys {
                    send(&conn, sys, &mut seq, &heartbeat()).await;
                }
            }
            cmd = cmd_rx.recv() => {
                let Some(cmd) = cmd else { return Ok(()) };
                let Some(sys) = state_tx.borrow().system_id else {
                    tracing::warn!(?cmd, "commande ignorée : autopilote pas encore vu");
                    continue;
                };
                tracing::warn!(?cmd, "envoi de commande à l'autopilote");
                send(&conn, sys, &mut seq, &command_message(cmd, sys)).await;
            }
        }
    }
}

fn log_ack(ack: &COMMAND_ACK_DATA) {
    let watched = matches!(
        ack.command,
        MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH | MavCmd::MAV_CMD_NAV_LAND
    );
    if !watched {
        return;
    }
    if ack.result == MavResult::MAV_RESULT_ACCEPTED {
        tracing::info!(command = ?ack.command, "commande acceptée par PX4");
    } else {
        tracing::error!(command = ?ack.command, result = ?ack.result, "commande refusée par PX4");
    }
}

async fn send(conn: &Conn, system_id: u8, seq: &mut u8, msg: &MavMessage) {
    let header = MavHeader {
        system_id,
        component_id: COMPONENT_ID,
        sequence: *seq,
    };
    *seq = seq.wrapping_add(1);
    if let Err(e) = conn.send(&header, msg).await {
        tracing::error!(error = %e, "échec d'envoi MAVLink");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mavlink::dialects::common::{
        EXTENDED_SYS_STATE_DATA, GLOBAL_POSITION_INT_DATA, SYS_STATUS_DATA,
    };

    const AP: MavHeader = MavHeader {
        system_id: 1,
        component_id: 1,
        sequence: 0,
    };

    fn px4_heartbeat(armed: bool) -> MavMessage {
        MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_PX4,
            base_mode: if armed {
                MavModeFlag::MAV_MODE_FLAG_SAFETY_ARMED
            } else {
                MavModeFlag::empty()
            },
            ..HEARTBEAT_DATA::default()
        })
    }

    #[test]
    fn learns_system_id_and_arming_time() {
        let mut s = VehicleState::default();
        apply(&mut s, &AP, &px4_heartbeat(false), 100);
        assert_eq!(s.system_id, Some(1));
        assert!(!s.armed);
        apply(&mut s, &AP, &px4_heartbeat(true), 2_000);
        apply(&mut s, &AP, &px4_heartbeat(true), 3_000);
        assert!(s.armed);
        assert_eq!(
            s.armed_since_ms, 2_000,
            "l'instant d'armement ne doit pas glisser"
        );
    }

    #[test]
    fn gcs_heartbeat_is_tracked_separately() {
        let mut s = VehicleState::default();
        let gcs = MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            mavtype: MavType::MAV_TYPE_GCS,
            autopilot: MavAutopilot::MAV_AUTOPILOT_INVALID,
            ..HEARTBEAT_DATA::default()
        });
        let hdr = MavHeader {
            system_id: 255,
            component_id: 190,
            sequence: 0,
        };
        apply(&mut s, &hdr, &gcs, 500);
        assert_eq!(s.last_gcs_ms, Some(500));
        assert_eq!(
            s.system_id, None,
            "une GCS ne doit pas être prise pour l'autopilote"
        );
    }

    #[test]
    fn position_is_decoded_from_e7_and_mm() {
        let mut s = VehicleState::default();
        let msg = MavMessage::GLOBAL_POSITION_INT(GLOBAL_POSITION_INT_DATA {
            lat: 473_977_420,
            lon: 85_455_940,
            relative_alt: 12_345,
            ..GLOBAL_POSITION_INT_DATA::default()
        });
        apply(&mut s, &AP, &msg, 0);
        let p = s.position.expect("position");
        assert!((p.lat_deg - 47.397_742).abs() < 1e-7);
        assert!((s.rel_alt_m - 12.345).abs() < 1e-4);
    }

    #[test]
    fn unknown_battery_stays_unknown() {
        let mut s = VehicleState::default();
        let mk = |pct| {
            MavMessage::SYS_STATUS(SYS_STATUS_DATA {
                battery_remaining: pct,
                ..SYS_STATUS_DATA::default()
            })
        };
        apply(&mut s, &AP, &mk(42), 0);
        assert_eq!(s.battery_pct, Some(42));
        apply(&mut s, &AP, &mk(-1), 0);
        assert_eq!(s.battery_pct, None);
    }

    #[test]
    fn landed_state_maps_to_in_air() {
        let mut s = VehicleState::default();
        for (ls, expected) in [
            (MavLandedState::MAV_LANDED_STATE_UNDEFINED, None),
            (MavLandedState::MAV_LANDED_STATE_ON_GROUND, Some(false)),
            (MavLandedState::MAV_LANDED_STATE_TAKEOFF, Some(true)),
            (MavLandedState::MAV_LANDED_STATE_IN_AIR, Some(true)),
        ] {
            let msg = MavMessage::EXTENDED_SYS_STATE(EXTENDED_SYS_STATE_DATA {
                landed_state: ls,
                ..EXTENDED_SYS_STATE_DATA::default()
            });
            apply(&mut s, &AP, &msg, 0);
            assert_eq!(s.in_air, expected, "{ls:?}");
        }
    }

    #[test]
    fn decodes_px4_flight_modes() {
        let mode = |main: u32, sub: u32| FlightMode::from_px4((main << 16) | (sub << 24));
        assert_eq!(mode(4, 5), FlightMode::ReturnToLaunch);
        assert_eq!(mode(4, 3), FlightMode::Hold);
        assert_eq!(mode(4, 6), FlightMode::Land);
        assert_eq!(mode(3, 0), FlightMode::Position);
        assert_eq!(mode(4, 8), FlightMode::Other { main: 4, sub: 8 });
    }

    #[test]
    fn heartbeat_updates_flight_mode() {
        let mut s = VehicleState::default();
        let hb = MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_PX4,
            base_mode: MavModeFlag::MAV_MODE_FLAG_CUSTOM_MODE_ENABLED,
            custom_mode: (4 << 16) | (5 << 24),
            ..HEARTBEAT_DATA::default()
        });
        apply(&mut s, &AP, &hb, 0);
        assert_eq!(s.flight_mode, Some(FlightMode::ReturnToLaunch));
    }

    #[test]
    fn rtl_targets_autopilot() {
        let MavMessage::COMMAND_LONG(c) = command_message(Command::ReturnToLaunch, 7) else {
            panic!("COMMAND_LONG attendu");
        };
        assert_eq!(c.command, MavCmd::MAV_CMD_NAV_RETURN_TO_LAUNCH);
        assert_eq!(c.target_system, 7);
        assert_eq!(c.target_component, 1);
    }
}
