use std::path::Path;

use anyhow::Context;
use serde::Deserialize;
use vigie_core::{FailsafeConfig, GeoPoint, Geofence, LocalFrame};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub mavlink: MavlinkConfig,
    pub failsafe: FailsafeSection,
    pub geofence: GeofenceSection,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MavlinkConfig {
    pub address: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FailsafeSection {
    pub battery_low_pct: u8,
    pub battery_critical_pct: u8,
    pub link_timeout_ms: u64,
    pub fence_warn_margin_m: f32,
}

impl From<FailsafeSection> for FailsafeConfig {
    fn from(s: FailsafeSection) -> Self {
        Self {
            battery_low_pct: s.battery_low_pct,
            battery_critical_pct: s.battery_critical_pct,
            link_timeout_ms: s.link_timeout_ms,
            fence_warn_margin_m: s.fence_warn_margin_m,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeofenceSection {
    pub ceiling_m: f32,
    /// Sommets `[latitude, longitude]` en degrés décimaux.
    pub vertices: Vec<[f64; 2]>,
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let cfg: Self = toml::from_str(text)?;
        anyhow::ensure!(
            cfg.failsafe.battery_critical_pct < cfg.failsafe.battery_low_pct,
            "battery_critical_pct doit être inférieur à battery_low_pct"
        );
        Ok(cfg)
    }

    /// Construit la geofence. Le repère local est centré sur le premier sommet.
    pub fn geofence(&self) -> anyhow::Result<Geofence> {
        let pts: Vec<GeoPoint> = self
            .geofence
            .vertices
            .iter()
            .map(|[lat, lon]| GeoPoint::new(*lat, *lon))
            .collect();
        let origin = *pts.first().context("geofence sans sommet")?;
        Geofence::new(LocalFrame::new(origin), &pts, self.geofence.ceiling_m)
            .map_err(|e| anyhow::anyhow!("geofence invalide : {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../config/vigie.toml");

    #[test]
    fn sample_config_is_valid() {
        let cfg = Config::parse(SAMPLE).expect("config d'exemple");
        let fence = cfg.geofence().expect("geofence d'exemple");
        // Le home PX4 SITL doit être dans la zone d'exemple.
        let home = GeoPoint::new(47.397_742, 8.545_594);
        assert!(matches!(
            fence.check(home, 0.0),
            vigie_core::FenceStatus::Inside { .. }
        ));
    }

    #[test]
    fn rejects_inverted_battery_thresholds() {
        let bad = SAMPLE.replace("battery_critical_pct = 15", "battery_critical_pct = 40");
        assert!(Config::parse(&bad).is_err());
    }
}
