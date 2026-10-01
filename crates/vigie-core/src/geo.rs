//! Conversions géographiques.
//!
//! On travaille dans un repère local plan (est, nord) centré sur un point d'origine,
//! avec une projection équirectangulaire. L'erreur reste sous le mètre jusqu'à
//! environ 10 km de l'origine aux latitudes européennes, ce qui couvre largement
//! une zone de vol de drone léger. Au-delà, il faudrait passer en ENU via ECEF.

use libm::{cos, sqrtf};

/// Rayon terrestre moyen (WGS-84, sphère équivalente), en mètres.
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Position géographique en degrés décimaux (WGS-84).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GeoPoint {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

impl GeoPoint {
    #[must_use]
    pub const fn new(lat_deg: f64, lon_deg: f64) -> Self {
        Self { lat_deg, lon_deg }
    }

    /// Depuis les entiers MAVLink (degrés × 1e7).
    #[must_use]
    pub fn from_e7(lat_e7: i32, lon_e7: i32) -> Self {
        Self::new(f64::from(lat_e7) * 1e-7, f64::from(lon_e7) * 1e-7)
    }
}

/// Vecteur 2D dans le repère local, en mètres. `x` = est, `y` = nord.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn dot(self, o: Self) -> f32 {
        self.x * o.x + self.y * o.y
    }

    #[must_use]
    pub fn norm(self) -> f32 {
        sqrtf(self.dot(self))
    }
}

impl core::ops::Sub for Vec2 {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.x - o.x, self.y - o.y)
    }
}

/// Repère local tangent centré sur `origin`.
#[derive(Debug, Clone, Copy)]
pub struct LocalFrame {
    origin: GeoPoint,
    /// Mètres par degré de longitude à la latitude de l'origine (précalculé).
    m_per_deg_lon: f64,
}

impl LocalFrame {
    const M_PER_DEG_LAT: f64 = EARTH_RADIUS_M * core::f64::consts::PI / 180.0;

    #[must_use]
    pub fn new(origin: GeoPoint) -> Self {
        let m_per_deg_lon = Self::M_PER_DEG_LAT * cos(origin.lat_deg.to_radians());
        Self {
            origin,
            m_per_deg_lon,
        }
    }

    #[must_use]
    pub fn origin(&self) -> GeoPoint {
        self.origin
    }

    /// Projette un point géographique dans le repère local.
    #[must_use]
    pub fn to_local(&self, p: GeoPoint) -> Vec2 {
        let east = (p.lon_deg - self.origin.lon_deg) * self.m_per_deg_lon;
        let north = (p.lat_deg - self.origin.lat_deg) * Self::M_PER_DEG_LAT;
        Vec2::new(east as f32, north as f32)
    }

    /// Opération inverse de [`Self::to_local`].
    #[must_use]
    pub fn to_geo(&self, v: Vec2) -> GeoPoint {
        GeoPoint::new(
            self.origin.lat_deg + f64::from(v.y) / Self::M_PER_DEG_LAT,
            self.origin.lon_deg + f64::from(v.x) / self.m_per_deg_lon,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Home par défaut de PX4 SITL (Zurich).
    const HOME: GeoPoint = GeoPoint::new(47.397_742, 8.545_594);

    #[test]
    fn origin_maps_to_zero() {
        let f = LocalFrame::new(HOME);
        assert_eq!(f.to_local(HOME), Vec2::default());
    }

    #[test]
    fn one_millidegree_north_is_about_111m() {
        let f = LocalFrame::new(HOME);
        let v = f.to_local(GeoPoint::new(HOME.lat_deg + 0.001, HOME.lon_deg));
        assert!((v.y - 111.19).abs() < 0.1, "y = {}", v.y);
        assert!(v.x.abs() < 1e-3);
    }

    #[test]
    fn east_is_shrunk_by_latitude() {
        let f = LocalFrame::new(HOME);
        let v = f.to_local(GeoPoint::new(HOME.lat_deg, HOME.lon_deg + 0.001));
        // 111.19 * cos(47.4°) ≈ 75.3 m
        assert!((v.x - 75.3).abs() < 0.2, "x = {}", v.x);
    }

    #[test]
    fn from_e7_matches_mavlink_encoding() {
        let p = GeoPoint::from_e7(473_977_420, 85_455_940);
        assert!((p.lat_deg - HOME.lat_deg).abs() < 1e-9);
        assert!((p.lon_deg - HOME.lon_deg).abs() < 1e-9);
    }

    proptest::proptest! {
        #[test]
        fn roundtrip_within_10km(dx in -10_000f32..10_000f32, dy in -10_000f32..10_000f32) {
            let f = LocalFrame::new(HOME);
            let v = Vec2::new(dx, dy);
            let back = f.to_local(f.to_geo(v));
            proptest::prop_assert!((back - v).norm() < 0.05);
        }
    }
}
