//! Mise au format d'entrée de YOLOX.
//!
//! YOLOX attend une image carrée, en BGR, valeurs 0–255 en `f32`, rangée en
//! CHW (canal, ligne, colonne). L'image est réduite en gardant ses proportions,
//! collée en haut à gauche, et le reste est rempli de gris 114, comme dans le
//! prétraitement de référence de Megvii.

use crate::Frame;

/// Valeur de remplissage du letterbox (référence YOLOX).
pub const PAD_VALUE: f32 = 114.0;

/// Tenseur d'entrée prêt pour le réseau.
#[derive(Debug, Clone)]
pub struct Letterboxed {
    /// Côté du carré d'entrée (416 pour YOLOX-Nano).
    pub size: usize,
    /// Facteur appliqué à l'image d'origine : `taille_réseau = taille_origine × scale`.
    pub scale: f32,
    /// Données CHW BGR, longueur `3 × size × size`.
    pub chw: Vec<f32>,
}

/// Redimensionne `frame` dans un carré `size × size` (interpolation bilinéaire).
#[must_use]
// Les conversions f32 → entier portent sur des valeurs positives (bornées par clamp).
#[allow(clippy::cast_sign_loss)]
pub fn letterbox(frame: &Frame, size: usize) -> Letterboxed {
    let (w, h) = (frame.width as f32, frame.height as f32);
    let scale = (size as f32 / w).min(size as f32 / h);
    let nw = ((w * scale) as usize).min(size);
    let nh = ((h * scale) as usize).min(size);

    let plane = size * size;
    let mut chw = vec![PAD_VALUE; 3 * plane];

    for y in 0..nh {
        // Centre du pixel de destination ramené dans l'image source.
        let sy = ((y as f32 + 0.5) / scale - 0.5).clamp(0.0, h - 1.0);
        let (y0, fy) = (sy.floor() as u32, sy.fract());
        let y1 = (y0 + 1).min(frame.height - 1);
        for x in 0..nw {
            let sx = ((x as f32 + 0.5) / scale - 0.5).clamp(0.0, w - 1.0);
            let (x0, fx) = (sx.floor() as u32, sx.fract());
            let x1 = (x0 + 1).min(frame.width - 1);

            let (p00, p10) = (frame.pixel(x0, y0), frame.pixel(x1, y0));
            let (p01, p11) = (frame.pixel(x0, y1), frame.pixel(x1, y1));
            for c in 0..3 {
                let top = f32::from(p00[c]) * (1.0 - fx) + f32::from(p10[c]) * fx;
                let bottom = f32::from(p01[c]) * (1.0 - fx) + f32::from(p11[c]) * fx;
                let v = top * (1.0 - fy) + bottom * fy;
                // RGB → BGR : le canal 0 du tenseur reçoit le bleu.
                chw[(2 - c) * plane + y * size + x] = v;
            }
        }
    }

    Letterboxed { size, scale, chw }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Frame {
        let data = rgb
            .iter()
            .copied()
            .cycle()
            .take((w * h * 3) as usize)
            .collect();
        Frame::new(w, h, data).expect("taille cohérente")
    }

    #[test]
    fn wide_image_is_padded_below() {
        let f = solid(800, 400, [10, 20, 30]);
        let lb = letterbox(&f, 416);
        assert!((lb.scale - 0.52).abs() < 1e-6);
        let plane = 416 * 416;
        // Ligne 0 : image, en BGR.
        assert!((lb.chw[0] - 30.0).abs() < 1e-3, "B");
        assert!((lb.chw[plane] - 20.0).abs() < 1e-3, "G");
        assert!((lb.chw[2 * plane] - 10.0).abs() < 1e-3, "R");
        // Ligne 300 (> 400 × 0.52 = 208) : remplissage.
        assert!((lb.chw[300 * 416] - PAD_VALUE).abs() < f32::EPSILON);
    }

    #[test]
    fn tall_image_is_padded_right() {
        let f = solid(100, 200, [255, 0, 0]);
        let lb = letterbox(&f, 416);
        assert!((lb.scale - 2.08).abs() < 1e-6);
        let plane = 416 * 416;
        assert!((lb.chw[2 * plane + 10 * 416 + 100] - 255.0).abs() < 1e-3);
        assert!((lb.chw[2 * plane + 10 * 416 + 300] - PAD_VALUE).abs() < f32::EPSILON);
    }

    proptest::proptest! {
        /// Une image unie reste unie après redimensionnement (pas de débordement d'indice
        /// ni de valeur inventée en bord d'image).
        #[test]
        fn solid_colour_is_preserved(w in 1u32..900, h in 1u32..900, v in 0u8..=255) {
            let lb = letterbox(&solid(w, h, [v, v, v]), 64);
            for x in &lb.chw {
                proptest::prop_assert!((x - f32::from(v)).abs() < 1e-3 || (x - PAD_VALUE).abs() < f32::EPSILON);
            }
        }
    }
}
