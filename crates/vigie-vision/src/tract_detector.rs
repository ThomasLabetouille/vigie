//! Détecteur YOLOX exécuté avec `tract`.

use std::path::Path;
use std::time::{Duration, Instant};

use tract_onnx::prelude::*;

use crate::{Detection, Detector, Frame, preprocess, yolox};

#[derive(Debug, thiserror::Error)]
pub enum VisionError {
    #[error("chargement du modèle : {0}")]
    Load(String),
    #[error("inférence : {0}")]
    Inference(String),
    #[error("sortie du modèle inattendue : {0}")]
    Output(String),
}

type Plan = std::sync::Arc<TypedRunnableModel>;

/// YOLOX (entrée carrée `size × size`) exécuté sur CPU par `tract`.
pub struct TractYolox {
    plan: Plan,
    size: usize,
    pub min_score: f32,
    pub max_iou: f32,
    last_timing: Timing,
}

impl std::fmt::Debug for TractYolox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TractYolox")
            .field("size", &self.size)
            .field("min_score", &self.min_score)
            .field("max_iou", &self.max_iou)
            .finish_non_exhaustive()
    }
}

/// Durée de chaque étape de la dernière détection.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timing {
    pub preprocess: Duration,
    pub inference: Duration,
    pub postprocess: Duration,
}

impl TractYolox {
    /// Charge et optimise le modèle. `size` est le côté de l'entrée (416 pour YOLOX-Nano).
    pub fn load(path: impl AsRef<Path>, size: usize) -> Result<Self, VisionError> {
        let load = |e: TractError| VisionError::Load(format!("{e:#}"));
        let plan = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(load)?
            .with_input_fact(0, f32::fact([1, 3, size, size]).into())
            .map_err(load)?
            .into_optimized()
            .map_err(load)?
            .into_runnable()
            .map_err(load)?;
        Ok(Self {
            plan,
            size,
            min_score: 0.3,
            max_iou: 0.45,
            last_timing: Timing::default(),
        })
    }

    #[must_use]
    pub fn last_timing(&self) -> Timing {
        self.last_timing
    }
}

impl Detector for TractYolox {
    type Error = VisionError;

    fn detect(&mut self, frame: &Frame) -> Result<Vec<Detection>, VisionError> {
        let t0 = Instant::now();
        let lb = preprocess::letterbox(frame, self.size);
        let input = Tensor::from_shape(&[1, 3, self.size, self.size], &lb.chw)
            .map_err(|e| VisionError::Inference(format!("{e:#}")))?;

        let t1 = Instant::now();
        let outputs = self
            .plan
            .run(tvec!(input.into()))
            .map_err(|e| VisionError::Inference(format!("{e:#}")))?;

        let t2 = Instant::now();
        let out_err = |e: TractError| VisionError::Output(format!("{e:#}"));
        let view = outputs
            .first()
            .ok_or_else(|| VisionError::Output("aucune sortie".into()))?
            .try_as_plain_ram()
            .map_err(out_err)?;
        let raw = view.as_slice::<f32>().map_err(out_err)?;
        if raw.len() != yolox::rows_for(self.size) * (5 + yolox::NUM_CLASSES) {
            return Err(VisionError::Output(format!("{} valeurs", raw.len())));
        }
        let dets = yolox::nms(
            yolox::decode(raw, self.size, lb.scale, self.min_score),
            self.max_iou,
        );

        self.last_timing = Timing {
            preprocess: t1 - t0,
            inference: t2 - t1,
            postprocess: t2.elapsed(),
        };
        Ok(dets)
    }
}
