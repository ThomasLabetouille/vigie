//! Détection d'objets embarquée pour Vigie.
//!
//! Le modèle est YOLOX-Nano (Megvii, licence Apache-2.0), exécuté avec `tract`,
//! un moteur d'inférence ONNX écrit en Rust pur : pas de bibliothèque C++ à
//! embarquer, ce qui simplifie la cross-compilation pour la cible Yocto.
//!
//! La chaîne se découpe en trois étapes, dont deux sont des fonctions pures
//! testées sans modèle :
//!
//! 1. [`preprocess::letterbox`] : image RGB → tenseur d'entrée du réseau ;
//! 2. inférence ([`Detector`]) ;
//! 3. [`yolox::decode`] puis [`yolox::nms`] : sortie brute → boîtes dans
//!    le repère de l'image d'origine.

pub mod coco;
pub mod preprocess;
pub mod yolox;

#[cfg(feature = "tract")]
mod tract_detector;
#[cfg(feature = "tract")]
pub use tract_detector::{TractYolox, VisionError};

/// Image RGB 8 bits, pixels rangés ligne par ligne.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Frame {
    /// Renvoie `None` si la taille du tampon ne correspond pas aux dimensions.
    #[must_use]
    pub fn new(width: u32, height: u32, rgb: Vec<u8>) -> Option<Self> {
        let expected = (width as usize) * (height as usize) * 3;
        (rgb.len() == expected).then_some(Self { width, height, rgb })
    }

    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = ((y as usize) * (self.width as usize) + x as usize) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// Boîte englobante en pixels de l'image d'origine (coins haut-gauche et bas-droite).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl BBox {
    #[must_use]
    pub fn area(&self) -> f32 {
        (self.x1 - self.x0).max(0.0) * (self.y1 - self.y0).max(0.0)
    }

    /// Intersection sur union, dans [0, 1].
    #[must_use]
    pub fn iou(&self, o: &Self) -> f32 {
        let iw = (self.x1.min(o.x1) - self.x0.max(o.x0)).max(0.0);
        let ih = (self.y1.min(o.y1) - self.y0.max(o.y0)).max(0.0);
        let inter = iw * ih;
        let union = self.area() + o.area() - inter;
        if union > 0.0 { inter / union } else { 0.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    /// Indice de classe COCO (voir [`coco::NAMES`]).
    pub class: usize,
    pub score: f32,
    pub bbox: BBox,
}

impl Detection {
    #[must_use]
    pub fn label(&self) -> &'static str {
        coco::NAMES.get(self.class).copied().unwrap_or("?")
    }
}

/// Un détecteur d'objets, quel que soit le moteur d'inférence.
pub trait Detector {
    type Error: std::error::Error;
    fn detect(&mut self, frame: &Frame) -> Result<Vec<Detection>, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rejects_wrong_buffer_size() {
        assert!(Frame::new(2, 2, vec![0; 12]).is_some());
        assert!(Frame::new(2, 2, vec![0; 11]).is_none());
    }

    #[test]
    fn iou_basics() {
        let a = BBox {
            x0: 0.0,
            y0: 0.0,
            x1: 10.0,
            y1: 10.0,
        };
        let b = BBox {
            x0: 5.0,
            y0: 0.0,
            x1: 15.0,
            y1: 10.0,
        };
        let far = BBox {
            x0: 20.0,
            y0: 20.0,
            x1: 30.0,
            y1: 30.0,
        };
        assert!((a.iou(&a) - 1.0).abs() < 1e-6);
        assert!((a.iou(&b) - 50.0 / 150.0).abs() < 1e-6);
        assert!(a.iou(&far).abs() < f32::EPSILON);
    }
}
