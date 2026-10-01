//! Spécification exécutable du moniteur de failsafes.

use vigie_core::{
    Action, FailsafeConfig, FailsafeMonitor, FenceBreach, FenceStatus, Inputs, Reason, State,
};

const SAFE: FenceStatus = FenceStatus::Inside { margin_m: 100.0 };

fn nominal(now_ms: u64) -> Inputs {
    Inputs {
        now_ms,
        armed: true,
        armed_since_ms: 0,
        last_link_ms: Some(now_ms),
        battery_pct: Some(80),
        fence: SAFE,
    }
}

fn monitor() -> FailsafeMonitor {
    FailsafeMonitor::new(FailsafeConfig::default())
}

#[test]
fn nominal_flight_does_nothing() {
    let mut m = monitor();
    for t in (0..10_000).step_by(100) {
        assert_eq!(m.update(&nominal(t)), Action::None);
    }
    assert_eq!(m.state(), State::Nominal);
}

#[test]
fn disarmed_never_acts() {
    let mut m = monitor();
    let i = Inputs {
        armed: false,
        battery_pct: Some(1),
        last_link_ms: None,
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..nominal(60_000)
    };
    assert_eq!(m.update(&i), Action::None);
    assert_eq!(m.state(), State::Nominal);
}

#[test]
fn fence_breach_triggers_rtl_once() {
    let mut m = monitor();
    let i = Inputs {
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..nominal(1_000)
    };
    assert_eq!(m.update(&i), Action::ReturnToLaunch(Reason::FenceBreach));
    assert_eq!(m.state(), State::Rtl);
    // Même situation au tick suivant : pas de nouvelle commande.
    assert_eq!(m.update(&Inputs { now_ms: 1_100, ..i }), Action::None);
}

#[test]
fn ceiling_breach_also_triggers_rtl() {
    let mut m = monitor();
    let i = Inputs {
        fence: FenceStatus::Breach(FenceBreach::Ceiling),
        ..nominal(1_000)
    };
    assert_eq!(m.update(&i), Action::ReturnToLaunch(Reason::FenceBreach));
}

#[test]
fn link_loss_after_timeout_triggers_rtl() {
    let mut m = monitor();
    let mut i = nominal(10_000);
    i.last_link_ms = Some(6_000); // 4 s : encore dans le délai
    assert_eq!(m.update(&i), Action::None);
    i.now_ms = 11_001; // 5,001 s
    assert_eq!(m.update(&i), Action::ReturnToLaunch(Reason::LinkLost));
}

#[test]
fn link_never_established_counts_from_arming() {
    let mut m = monitor();
    let mut i = Inputs {
        armed_since_ms: 2_000,
        last_link_ms: None,
        ..nominal(4_000)
    };
    assert_eq!(m.update(&i), Action::None);
    i.now_ms = 7_001;
    assert_eq!(m.update(&i), Action::ReturnToLaunch(Reason::LinkLost));
}

#[test]
fn rtl_is_sticky_when_link_comes_back() {
    let mut m = monitor();
    let mut i = nominal(10_000);
    i.last_link_ms = Some(0);
    assert_eq!(m.update(&i), Action::ReturnToLaunch(Reason::LinkLost));
    assert_eq!(m.update(&nominal(10_100)), Action::None);
    assert_eq!(m.state(), State::Rtl);
}

#[test]
fn critical_battery_lands_and_beats_fence_breach() {
    let mut m = monitor();
    let i = Inputs {
        battery_pct: Some(10),
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..nominal(1_000)
    };
    assert_eq!(m.update(&i), Action::Land(Reason::BatteryCritical));
    assert_eq!(m.state(), State::Landing);
}

#[test]
fn rtl_escalates_to_landing() {
    let mut m = monitor();
    let breach = Inputs {
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..nominal(1_000)
    };
    assert_eq!(
        m.update(&breach),
        Action::ReturnToLaunch(Reason::FenceBreach)
    );
    let crit = Inputs {
        battery_pct: Some(15),
        ..breach
    };
    assert_eq!(m.update(&crit), Action::Land(Reason::BatteryCritical));
    assert_eq!(m.state(), State::Landing);
}

#[test]
fn landing_never_downgrades_to_rtl() {
    let mut m = monitor();
    let crit = Inputs {
        battery_pct: Some(5),
        ..nominal(1_000)
    };
    let _ = m.update(&crit);
    let breach = Inputs {
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..crit
    };
    assert_eq!(m.update(&breach), Action::None);
    assert_eq!(m.state(), State::Landing);
}

#[test]
fn low_battery_warns_once() {
    let mut m = monitor();
    let i = Inputs {
        battery_pct: Some(30),
        ..nominal(1_000)
    };
    assert_eq!(m.update(&i), Action::Warn(Reason::BatteryLow));
    assert_eq!(m.state(), State::Warning);
    assert_eq!(m.update(&i), Action::None);
}

#[test]
fn fence_proximity_warns() {
    let mut m = monitor();
    let i = Inputs {
        fence: FenceStatus::Inside { margin_m: 19.9 },
        ..nominal(1_000)
    };
    assert_eq!(m.update(&i), Action::Warn(Reason::FenceProximity));
}

#[test]
fn warning_escalates_to_rtl() {
    let mut m = monitor();
    let near = Inputs {
        fence: FenceStatus::Inside { margin_m: 5.0 },
        ..nominal(1_000)
    };
    assert_eq!(m.update(&near), Action::Warn(Reason::FenceProximity));
    let out = Inputs {
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..near
    };
    assert_eq!(m.update(&out), Action::ReturnToLaunch(Reason::FenceBreach));
}

#[test]
fn unknown_battery_is_not_an_alarm() {
    let mut m = monitor();
    let i = Inputs {
        battery_pct: None,
        ..nominal(1_000)
    };
    assert_eq!(m.update(&i), Action::None);
}

#[test]
fn disarm_resets_to_nominal() {
    let mut m = monitor();
    let breach = Inputs {
        fence: FenceStatus::Breach(FenceBreach::Horizontal),
        ..nominal(1_000)
    };
    let _ = m.update(&breach);
    assert_eq!(m.state(), State::Rtl);
    let _ = m.update(&Inputs {
        armed: false,
        ..breach
    });
    assert_eq!(m.state(), State::Nominal);
    // Réarmé dans la même situation : le failsafe se redéclenche.
    assert_eq!(
        m.update(&breach),
        Action::ReturnToLaunch(Reason::FenceBreach)
    );
}
