# Modèles

| Fichier | Source | Licence | SHA-256 |
|---|---|---|---|
| `yolox_nano.onnx` | [YOLOX 0.1.1rc0](https://github.com/Megvii-BaseDetection/YOLOX/releases/tag/0.1.1rc0), Megvii | Apache-2.0 | `c789161ed43c8269fcd4e67c67eeeb4e80c622da2eb296a20bc6007bd18a0b7d` |

Entrée `images` 1×3×416×416 (BGR, valeurs 0–255), sortie `output` 1×3549×85. Choix expliqué dans
[l'ADR 0002](../docs/adr/0002-vision.md).

L'image de test `crates/vigie-vision/tests/data/dog.jpg` vient du dépôt YOLOX (`assets/dog.jpg`,
Apache-2.0).
