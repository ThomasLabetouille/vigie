//! Geofence polygonale avec plafond d'altitude.
//!
//! Le polygone est stocké dans le repère local (voir [`crate::geo`]) dans un
//! `heapless::Vec` : taille bornée connue à la compilation, pas d'allocateur.

use crate::geo::{GeoPoint, LocalFrame, Vec2};
use heapless::Vec;

/// Nombre maximal de sommets. 32 suffit pour une zone dessinée à la main.
pub const MAX_VERTICES: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceError {
    /// Moins de 3 sommets.
    TooFewVertices,
    /// Plus de [`MAX_VERTICES`] sommets.
    TooManyVertices,
    /// Aire quasi nulle (sommets alignés ou confondus).
    Degenerate,
    /// Deux arêtes non adjacentes se croisent.
    SelfIntersecting,
    /// Plafond négatif ou nul.
    InvalidCeiling,
}

impl core::fmt::Display for FenceError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let msg = match self {
            Self::TooFewVertices => "il faut au moins 3 sommets",
            Self::TooManyVertices => "trop de sommets",
            Self::Degenerate => "polygone dégénéré (aire nulle)",
            Self::SelfIntersecting => "polygone auto-sécant",
            Self::InvalidCeiling => "plafond d'altitude invalide",
        };
        f.write_str(msg)
    }
}

impl core::error::Error for FenceError {}

/// Type de sortie de zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceBreach {
    Horizontal,
    Ceiling,
}

/// Résultat d'un contrôle de position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FenceStatus {
    /// Dans la zone. `margin_m` = distance à la limite la plus proche
    /// (bord horizontal ou plafond).
    Inside {
        margin_m: f32,
    },
    Breach(FenceBreach),
}

#[derive(Debug, Clone)]
pub struct Geofence {
    frame: LocalFrame,
    vertices: Vec<Vec2, MAX_VERTICES>,
    ceiling_m: f32,
}

impl Geofence {
    /// Construit une geofence à partir de sommets géographiques, projetés dans `frame`.
    ///
    /// Le sens de parcours (horaire ou anti-horaire) est indifférent.
    pub fn new(
        frame: LocalFrame,
        vertices: &[GeoPoint],
        ceiling_m: f32,
    ) -> Result<Self, FenceError> {
        if vertices.len() < 3 {
            return Err(FenceError::TooFewVertices);
        }
        if ceiling_m.is_nan() || ceiling_m <= 0.0 {
            return Err(FenceError::InvalidCeiling);
        }
        let mut local = Vec::new();
        for v in vertices {
            local
                .push(frame.to_local(*v))
                .map_err(|_| FenceError::TooManyVertices)?;
        }
        if signed_area(&local).abs() < 1.0 {
            return Err(FenceError::Degenerate);
        }
        if is_self_intersecting(&local) {
            return Err(FenceError::SelfIntersecting);
        }
        Ok(Self {
            frame,
            vertices: local,
            ceiling_m,
        })
    }

    #[must_use]
    pub fn frame(&self) -> &LocalFrame {
        &self.frame
    }

    /// Contrôle une position. `rel_alt_m` est l'altitude relative au point de décollage.
    #[must_use]
    pub fn check(&self, pos: GeoPoint, rel_alt_m: f32) -> FenceStatus {
        let p = self.frame.to_local(pos);
        if !self.contains(p) {
            return FenceStatus::Breach(FenceBreach::Horizontal);
        }
        if rel_alt_m > self.ceiling_m {
            return FenceStatus::Breach(FenceBreach::Ceiling);
        }
        let margin_m = self.distance_to_edge(p).min(self.ceiling_m - rel_alt_m);
        FenceStatus::Inside { margin_m }
    }

    /// Test point-dans-polygone par lancer de rayon (règle pair/impair).
    #[must_use]
    pub fn contains(&self, p: Vec2) -> bool {
        let mut inside = false;
        for (a, b) in edges(&self.vertices) {
            if (a.y > p.y) != (b.y > p.y) {
                let x_cross = a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if p.x < x_cross {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Distance minimale de `p` au contour du polygone.
    #[must_use]
    pub fn distance_to_edge(&self, p: Vec2) -> f32 {
        edges(&self.vertices)
            .map(|(a, b)| point_segment_distance(p, a, b))
            .fold(f32::INFINITY, f32::min)
    }
}

fn edges(v: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    v.iter().copied().zip(v.iter().copied().cycle().skip(1))
}

fn signed_area(v: &[Vec2]) -> f32 {
    edges(v).map(|(a, b)| a.x * b.y - b.x * a.y).sum::<f32>() / 2.0
}

fn point_segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let proj = Vec2::new(a.x + t * ab.x, a.y + t * ab.y);
    (p - proj).norm()
}

fn cross(o: Vec2, a: Vec2, b: Vec2) -> f32 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

fn segments_cross(p1: Vec2, p2: Vec2, q1: Vec2, q2: Vec2) -> bool {
    let d1 = cross(q1, q2, p1);
    let d2 = cross(q1, q2, p2);
    let d3 = cross(p1, p2, q1);
    let d4 = cross(p1, p2, q2);
    (d1 > 0.0) != (d2 > 0.0) && (d3 > 0.0) != (d4 > 0.0)
}

/// O(n²), acceptable avec n ≤ 32 et appelé une seule fois à la construction.
fn is_self_intersecting(v: &[Vec2]) -> bool {
    let n = v.len();
    for i in 0..n {
        for j in (i + 1)..n {
            // Arêtes adjacentes : elles partagent un sommet, on les ignore.
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            if segments_cross(v[i], v[(i + 1) % n], v[j], v[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: GeoPoint = GeoPoint::new(47.397_742, 8.545_594);

    /// Carré de 200 m × 200 m centré sur HOME, plafond 120 m.
    fn square() -> Geofence {
        let f = LocalFrame::new(HOME);
        let c = [
            (-100.0, -100.0),
            (100.0, -100.0),
            (100.0, 100.0),
            (-100.0, 100.0),
        ]
        .map(|(x, y)| f.to_geo(Vec2::new(x, y)));
        Geofence::new(f, &c, 120.0).expect("carré valide")
    }

    fn at(fence: &Geofence, x: f32, y: f32) -> GeoPoint {
        fence.frame().to_geo(Vec2::new(x, y))
    }

    #[test]
    fn center_is_inside_with_100m_margin() {
        let g = square();
        match g.check(HOME, 10.0) {
            FenceStatus::Inside { margin_m } => assert!((margin_m - 100.0).abs() < 0.1),
            FenceStatus::Breach(b) => panic!("{b:?}"),
        }
    }

    #[test]
    fn ceiling_margin_wins_when_closer() {
        let g = square();
        match g.check(HOME, 115.0) {
            FenceStatus::Inside { margin_m } => assert!((margin_m - 5.0).abs() < 0.01),
            FenceStatus::Breach(b) => panic!("{b:?}"),
        }
    }

    #[test]
    fn outside_is_horizontal_breach() {
        let g = square();
        assert_eq!(
            g.check(at(&g, 150.0, 0.0), 10.0),
            FenceStatus::Breach(FenceBreach::Horizontal)
        );
    }

    #[test]
    fn above_ceiling_is_ceiling_breach() {
        let g = square();
        assert_eq!(
            g.check(HOME, 121.0),
            FenceStatus::Breach(FenceBreach::Ceiling)
        );
    }

    #[test]
    fn concave_notch_is_outside() {
        // Forme en U : l'encoche centrale est hors zone.
        let f = LocalFrame::new(HOME);
        let u = [
            (-100.0, -100.0),
            (100.0, -100.0),
            (100.0, 100.0),
            (50.0, 100.0),
            (50.0, -50.0),
            (-50.0, -50.0),
            (-50.0, 100.0),
            (-100.0, 100.0),
        ]
        .map(|(x, y)| f.to_geo(Vec2::new(x, y)));
        let g = Geofence::new(f, &u, 120.0).expect("U valide");
        assert!(!g.contains(Vec2::new(0.0, 50.0)));
        assert!(g.contains(Vec2::new(0.0, -75.0)));
        assert!(g.contains(Vec2::new(75.0, 50.0)));
    }

    #[test]
    fn rejects_bad_polygons() {
        let f = LocalFrame::new(HOME);
        let geo = |pts: &[(f32, f32)]| -> heapless::Vec<GeoPoint, 8> {
            pts.iter()
                .map(|&(x, y)| f.to_geo(Vec2::new(x, y)))
                .collect()
        };
        assert_eq!(
            Geofence::new(f, &geo(&[(0.0, 0.0), (1.0, 1.0)]), 100.0).unwrap_err(),
            FenceError::TooFewVertices
        );
        assert_eq!(
            Geofence::new(f, &geo(&[(0.0, 0.0), (50.0, 0.0), (100.0, 0.0)]), 100.0).unwrap_err(),
            FenceError::Degenerate
        );
        // Nœud papillon.
        assert_eq!(
            Geofence::new(
                f,
                &geo(&[(0.0, 0.0), (100.0, 100.0), (100.0, 0.0), (0.0, 50.0)]),
                100.0
            )
            .unwrap_err(),
            FenceError::SelfIntersecting
        );
        assert_eq!(
            Geofence::new(f, &geo(&[(0.0, 0.0), (100.0, 0.0), (0.0, 100.0)]), 0.0).unwrap_err(),
            FenceError::InvalidCeiling
        );
    }

    proptest::proptest! {
        /// Dans le carré, la marge horizontale vaut exactement la distance au bord le plus proche.
        #[test]
        fn margin_matches_analytic_distance(x in -99f32..99f32, y in -99f32..99f32) {
            let g = square();
            let expected = (100.0 - x.abs()).min(100.0 - y.abs());
            let got = g.distance_to_edge(Vec2::new(x, y));
            proptest::prop_assert!((got - expected).abs() < 1e-2);
        }

        #[test]
        fn far_points_are_always_outside(x in 101f32..10_000f32, y in -10_000f32..10_000f32) {
            let g = square();
            proptest::prop_assert!(!g.contains(Vec2::new(x, y)));
            proptest::prop_assert!(!g.contains(Vec2::new(-x, y)));
        }
    }
}
