//! Décodage de la sortie brute de YOLOX et suppression des doublons.
//!
//! Le réseau produit une ligne par cellule de trois grilles (pas de 8, 16 et
//! 32 pixels), soit 52² + 26² + 13² = 3549 lignes pour une entrée de 416.
//! Chaque ligne contient `[dx, dy, log w, log h, objectness, 80 × score de classe]`,
//! objectness et scores déjà passés par une sigmoïde.

use crate::{BBox, Detection};

/// Pas des trois grilles de YOLOX.
pub const STRIDES: [usize; 3] = [8, 16, 32];
/// Nombre de classes COCO.
pub const NUM_CLASSES: usize = 80;
const ROW: usize = 5 + NUM_CLASSES;

/// Nombre de lignes attendues en sortie pour une entrée carrée de côté `size`.
#[must_use]
pub fn rows_for(size: usize) -> usize {
    STRIDES.iter().map(|s| (size / s) * (size / s)).sum()
}

/// Transforme la sortie brute en détections dans le repère de l'image d'origine.
///
/// `scale` est le facteur du letterbox ; seules les détections dont le score
/// (objectness × classe) atteint `min_score` sont gardées. Renvoie une liste vide
/// si `output` n'a pas la taille attendue pour `size`.
#[must_use]
pub fn decode(output: &[f32], size: usize, scale: f32, min_score: f32) -> Vec<Detection> {
    if output.len() != rows_for(size) * ROW || scale <= 0.0 {
        return Vec::new();
    }
    let mut dets = Vec::new();
    let mut rows = output.chunks_exact(ROW);
    for stride in STRIDES {
        let cells = size / stride;
        for gy in 0..cells {
            for gx in 0..cells {
                let Some(row) = rows.next() else {
                    return dets;
                };
                let (class, cls_score) =
                    row[5..]
                        .iter()
                        .copied()
                        .enumerate()
                        .fold(
                            (0, f32::MIN),
                            |best, (i, v)| if v > best.1 { (i, v) } else { best },
                        );
                let score = row[4] * cls_score;
                if score < min_score {
                    continue;
                }
                let step = stride as f32;
                let cx = (row[0] + gx as f32) * step;
                let cy = (row[1] + gy as f32) * step;
                let w = row[2].exp() * step;
                let h = row[3].exp() * step;
                dets.push(Detection {
                    class,
                    score,
                    bbox: BBox {
                        x0: (cx - w / 2.0) / scale,
                        y0: (cy - h / 2.0) / scale,
                        x1: (cx + w / 2.0) / scale,
                        y1: (cy + h / 2.0) / scale,
                    },
                });
            }
        }
    }
    dets
}

/// Suppression des non-maxima, classe par classe : parmi des boîtes de même
/// classe qui se recouvrent au-delà de `max_iou`, on garde la plus sûre.
/// Le résultat est trié par score décroissant.
#[must_use]
pub fn nms(mut dets: Vec<Detection>, max_iou: f32) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::with_capacity(dets.len());
    for d in dets {
        let overlaps = kept
            .iter()
            .any(|k| k.class == d.class && k.bbox.iou(&d.bbox) > max_iou);
        if !overlaps {
            kept.push(d);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: usize = 416;

    fn empty_output() -> Vec<f32> {
        vec![0.0; rows_for(SIZE) * ROW]
    }

    fn set_row(out: &mut [f32], row: usize, vals: [f32; 5], class: usize, cls_score: f32) {
        let r = &mut out[row * ROW..(row + 1) * ROW];
        r[..5].copy_from_slice(&vals);
        r[5 + class] = cls_score;
    }

    #[test]
    fn row_count_for_416() {
        assert_eq!(rows_for(416), 3549);
    }

    #[test]
    fn decodes_cell_on_each_grid() {
        let mut out = empty_output();
        // Grille 8 : cellule (gx=3, gy=2) → ligne 2*52+3.
        set_row(&mut out, 2 * 52 + 3, [0.5, 0.5, 0.0, 0.0, 1.0], 16, 0.9);
        // Grille 32 : première cellule, juste après les 52² + 26² lignes.
        set_row(
            &mut out,
            52 * 52 + 26 * 26,
            [0.0, 0.0, 1.0f32.ln(), 2.0f32.ln(), 0.8],
            0,
            1.0,
        );

        let dets = decode(&out, SIZE, 0.5, 0.3);
        assert_eq!(dets.len(), 2);

        let dog = dets.iter().find(|d| d.class == 16).expect("chien");
        // centre (3.5×8, 2.5×8) = (28, 20), taille 8×8, puis ÷0.5.
        assert_eq!(
            dog.bbox,
            BBox {
                x0: 48.0,
                y0: 32.0,
                x1: 64.0,
                y1: 48.0
            }
        );
        assert!((dog.score - 0.9).abs() < 1e-6);
        assert_eq!(dog.label(), "dog");

        let person = dets.iter().find(|d| d.class == 0).expect("personne");
        // centre (0, 0), w = 32, h = 64, puis ÷0.5.
        assert_eq!(
            person.bbox,
            BBox {
                x0: -32.0,
                y0: -64.0,
                x1: 32.0,
                y1: 64.0
            }
        );
    }

    #[test]
    fn score_is_objectness_times_class() {
        let mut out = empty_output();
        set_row(&mut out, 0, [0.0, 0.0, 0.0, 0.0, 0.5], 2, 0.5);
        assert!(decode(&out, SIZE, 1.0, 0.3).is_empty(), "0.25 < 0.3");
        assert_eq!(decode(&out, SIZE, 1.0, 0.2).len(), 1);
    }

    #[test]
    fn wrong_output_size_gives_nothing() {
        assert!(decode(&[0.0; 10], SIZE, 1.0, 0.0).is_empty());
    }

    #[test]
    fn nms_keeps_best_per_class() {
        let b = |x0: f32| BBox {
            x0,
            y0: 0.0,
            x1: x0 + 10.0,
            y1: 10.0,
        };
        let d = |class, score, x0| Detection {
            class,
            score,
            bbox: b(x0),
        };
        let kept = nms(
            vec![
                d(0, 0.6, 1.0),
                d(0, 0.9, 0.0),
                d(1, 0.5, 0.0),
                d(0, 0.7, 50.0),
            ],
            0.45,
        );
        let summary: Vec<_> = kept.iter().map(|k| (k.class, k.score)).collect();
        // 0.6 recouvre 0.9 (même classe) → supprimé ; la classe 1 est indépendante.
        assert_eq!(summary, vec![(0, 0.9), (0, 0.7), (1, 0.5)]);
    }
}
