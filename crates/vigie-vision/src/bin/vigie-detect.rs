//! `vigie-detect <modèle.onnx> <image>` : lance la détection sur une image et
//! affiche les objets trouvés et le temps de chaque étape.

use std::process::ExitCode;

use vigie_vision::{Detector, Frame, TractYolox};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, model, image] = args.as_slice() else {
        eprintln!("usage : vigie-detect <modèle.onnx> <image>");
        return ExitCode::FAILURE;
    };
    match run(model, image) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(model: &str, image: &str) -> Result<(), Box<dyn std::error::Error>> {
    let img = image::open(image)?.to_rgb8();
    let frame = Frame::new(img.width(), img.height(), img.into_raw())
        .ok_or("dimensions d'image incohérentes")?;

    let mut det = TractYolox::load(model, 416)?;
    // Première exécution : allocation des tampons, non représentative.
    let _ = det.detect(&frame)?;
    let dets = det.detect(&frame)?;
    let t = det.last_timing();

    for d in &dets {
        let b = d.bbox;
        println!(
            "{:<12} {:.2}  [{:.0}, {:.0}, {:.0}, {:.0}]",
            d.label(),
            d.score,
            b.x0,
            b.y0,
            b.x1,
            b.y1
        );
    }
    println!(
        "prétraitement {:.1} ms · inférence {:.1} ms · post-traitement {:.1} ms",
        t.preprocess.as_secs_f64() * 1e3,
        t.inference.as_secs_f64() * 1e3,
        t.postprocess.as_secs_f64() * 1e3
    );
    Ok(())
}
