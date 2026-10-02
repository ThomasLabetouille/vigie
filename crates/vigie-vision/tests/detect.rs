//! Détection réelle avec YOLOX-Nano sur une image de référence.
//!
//! Les boîtes attendues viennent de l'implémentation de référence (onnxruntime +
//! post-traitement Python de Megvii) sur la même image.

#![cfg(feature = "tract")]

use vigie_vision::{BBox, Detector, Frame, TractYolox};

const MODEL: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../models/yolox_nano.onnx");
const IMAGE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/dog.jpg");

fn frame() -> Frame {
    let img = image::open(IMAGE).expect("image de test").to_rgb8();
    Frame::new(img.width(), img.height(), img.into_raw()).expect("dimensions")
}

#[test]
fn finds_the_reference_objects() {
    let mut det = TractYolox::load(MODEL, 416).expect("modèle");
    let dets = det.detect(&frame()).expect("inférence");

    let expected = [
        (
            "dog",
            BBox {
                x0: 133.1,
                y0: 207.0,
                x1: 324.6,
                y1: 542.3,
            },
        ),
        (
            "bicycle",
            BBox {
                x0: 45.6,
                y0: 131.9,
                x1: 571.7,
                y1: 430.6,
            },
        ),
        (
            "car",
            BBox {
                x0: 466.7,
                y0: 78.2,
                x1: 691.5,
                y1: 171.5,
            },
        ),
    ];
    for (label, bbox) in expected {
        let found = dets
            .iter()
            .filter(|d| d.label() == label)
            .map(|d| d.bbox.iou(&bbox))
            .fold(0.0f32, f32::max);
        assert!(
            found > 0.9,
            "{label} : IoU {found:.2} avec la référence ({dets:?})"
        );
    }
}
