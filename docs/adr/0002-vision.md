# ADR 0002 — Détection d'objets embarquée

- Statut : accepté
- Date : 2026-10-02

## Contexte

La semaine 2 ajoute une détection d'objets sur le flux caméra du drone simulé, avec une
contrainte qui pèse plus que la précision : le binaire doit se cross-compiler pour une image
Yocto aarch64 sans traîner de bibliothèque C++ volumineuse, et tenir moins de 100 ms par image
sur CPU.

## Décisions

### Modèle : YOLOX-Nano

| Option | Licence | Taille | Remarque |
|---|---|---|---|
| **YOLOX-Nano** (Megvii) | Apache-2.0 | 3,6 Mo, entrée 416 | retenu |
| YOLOv8n / v11n (Ultralytics) | AGPL-3.0 | 6 Mo | licence incompatible avec un produit fermé |
| SSD MobileNet v2 | Apache-2.0 | 17 Mo | moins précis à taille comparable |

L'AGPL d'Ultralytics impose de publier le code de tout produit qui embarque le modèle : rédhibitoire
pour un équipement de défense. YOLOX-Nano a une licence Apache-2.0, n'utilise que dix opérateurs
ONNX simples (`Conv`, `Resize`, `Concat`, `Sigmoid`…) et tient sur 3,6 Mo. Le fichier est versionné
dans `models/` (SHA-256 `c789161e…0b7d`).

### Moteur : tract plutôt qu'ONNX Runtime

`tract` est écrit en Rust pur : il se compile avec le reste du projet pour n'importe quelle cible
Rust, sans bibliothèque partagée à fournir dans l'image. ONNX Runtime (crate `ort`) est plus rapide
mais demande soit de télécharger une bibliothèque précompilée (absente pour certaines cibles), soit
de la compiler dans Yocto, ce qui coûte cher en temps de build et en taille d'image.

Mesuré sur l'image de référence (768 × 576), en release, sur le CPU de la machine de CI :
prétraitement 3,5 ms, inférence 46 ms, post-traitement 0,3 ms. Le budget de 100 ms est tenu ; si
la cible ARM ne le tient pas, `Detector` est un trait et un backend `ort` peut s'ajouter sans
toucher au reste.

Conséquence : `tract` 0.23 exige Rust 1.91, donc la MSRV du workspace passe de 1.88 à 1.91.
Yocto 6.0 fournit Rust 1.94.1 ; pas d'impact sur la cible.

### Découpage

```
Frame (RGB) ──letterbox──▶ tenseur 1×3×416×416 (BGR, 0–255) ──tract──▶ 3549×85
                                                                         │
            Vec<Detection> ◀──nms (IoU 0,45)── decode (grilles 8/16/32, score ≥ 0,3)
```

Le prétraitement et le décodage sont des fonctions pures testées sans modèle, y compris avec des
tests de propriétés. Un test d'intégration exécute le vrai modèle sur une image de référence et
compare les boîtes à celles de l'implémentation officielle (onnxruntime + post-traitement Python de
Megvii) : IoU > 0,9 exigée. La feature `tract` est optionnelle pour que le cœur de vision compile
sans moteur d'inférence.

### Source d'images

Le plugin `GstCameraSystem` de PX4 diffuse la caméra du modèle `x500_mono_cam` en RTP/H.264 sur
UDP 5600. Vigie le lira avec GStreamer (`udpsrc ! rtph264depay ! avdec_h264 ! videoconvert ! appsink`)
derrière une feature dédiée, pour que la CI n'ait pas besoin des bibliothèques GStreamer.

## Conséquences

- Les classes COCO ne sont pas spécialisées drone : une personne vue du dessus à 30 m est mal
  détectée. Suffisant pour la démonstration de la chaîne ; un réentraînement sur un jeu de données
  aérien (VisDrone) serait l'étape suivante pour un usage réel.
- Le modèle versionné (3,6 Mo) alourdit le dépôt mais rend les tests reproductibles sans
  téléchargement.
