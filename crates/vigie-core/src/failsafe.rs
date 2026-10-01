//! Moniteur de failsafes.
//!
//! Machine à états qui reçoit, à chaque tick, un instantané des entrées
//! ([`Inputs`]) et renvoie au plus une [`Action`] à exécuter.
//!
//! # Règles
//!
//! Évaluées dans cet ordre de priorité (la première qui s'applique gagne) :
//!
//! | # | Condition                                          | État cible | Action émise          |
//! |---|----------------------------------------------------|------------|-----------------------|
//! | 1 | drone désarmé                                      | `Nominal`  | `None` (réinitialise) |
//! | 2 | batterie ≤ `battery_critical_pct`                  | `Landing`  | `Land(BatteryCritical)` |
//! | 3 | geofence franchie                                  | `Rtl`      | `ReturnToLaunch(FenceBreach)` |
//! | 4 | lien sol perdu depuis > `link_timeout_ms`          | `Rtl`      | `ReturnToLaunch(LinkLost)` |
//! | 5 | batterie ≤ `battery_low_pct`                       | `Warning`  | `Warn(BatteryLow)` |
//! | 6 | marge geofence < `fence_warn_margin_m`             | `Warning`  | `Warn(FenceProximity)` |
//! | 7 | sinon                                              | inchangé   | `None` |
//!
//! # Invariants
//!
//! - **Escalade seulement.** En vol, l'ordre de sévérité est
//!   `Nominal < Warning < Rtl < Landing`. On ne redescend jamais tout seul :
//!   un `Rtl` reste `Rtl` même si le lien revient. Seul le désarmement remet à `Nominal`.
//! - **Pas de rafale de commandes.** Une action n'est émise qu'au moment où l'état
//!   change. Deux ticks consécutifs dans la même situation → le second renvoie `None`.
//! - **Donnée absente = donnée inconnue, pas donnée saine.** `battery_pct == None`
//!   ne déclenche rien ; `last_link_ms == None` (aucun heartbeat reçu depuis
//!   l'armement) est traité comme un lien perdu dès que `link_timeout_ms` est
//!   écoulé depuis `armed_since_ms`.
//! - **Fonction pure du passé.** Aucune horloge interne : seul `now_ms` fait foi.

use crate::geofence::FenceStatus;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FailsafeConfig {
    pub battery_low_pct: u8,
    pub battery_critical_pct: u8,
    pub link_timeout_ms: u64,
    pub fence_warn_margin_m: f32,
}

impl Default for FailsafeConfig {
    fn default() -> Self {
        Self {
            battery_low_pct: 30,
            battery_critical_pct: 15,
            link_timeout_ms: 5_000,
            fence_warn_margin_m: 20.0,
        }
    }
}

/// Instantané des entrées à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    pub now_ms: u64,
    pub armed: bool,
    /// Instant de l'armement, pour juger un lien jamais établi.
    pub armed_since_ms: u64,
    /// Dernier heartbeat reçu de la station sol.
    pub last_link_ms: Option<u64>,
    pub battery_pct: Option<u8>,
    pub fence: FenceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Nominal,
    Warning,
    Rtl,
    Landing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    BatteryLow,
    BatteryCritical,
    FenceProximity,
    FenceBreach,
    LinkLost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Warn(Reason),
    ReturnToLaunch(Reason),
    Land(Reason),
}

#[derive(Debug, Clone)]
pub struct FailsafeMonitor {
    cfg: FailsafeConfig,
    state: State,
}

impl FailsafeMonitor {
    #[must_use]
    pub fn new(cfg: FailsafeConfig) -> Self {
        Self {
            cfg,
            state: State::Nominal,
        }
    }

    #[must_use]
    pub fn state(&self) -> State {
        self.state
    }

    #[must_use]
    pub fn config(&self) -> &FailsafeConfig {
        &self.cfg
    }

    /// Évalue les entrées et renvoie l'action à exécuter (voir les règles en tête de module).
    ///
    /// À implémenter. Les tests de `tests/failsafe.rs` décrivent le comportement attendu.
    #[must_use]
    pub fn update(&mut self, inputs: &Inputs) -> Action {
        let _ = inputs;
        todo!("implémenter la machine à états décrite en tête de module")
    }
}
