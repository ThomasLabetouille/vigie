# Planning

Légende : **[T]** Thomas, **[C]** Claude (squelette, revue), **[T+C]** à deux.

## Semaine 1 — Supervision MAVLink contre PX4 SITL

- [x] [C] Workspace Cargo, lints, CI
- [x] [C] `vigie-core` : projection locale, geofence (+ tests de propriétés)
- [x] [C] `vigie-mavlink` : décodage pur + boucle async
- [x] [C] `vigied` : config TOML, superviseur 10 Hz, `--dry-run`
- [x] [C] Spécification exécutable des failsafes (`vigie-core/tests/failsafe.rs`)
- [x] [T] Environnement WSL2 + PX4 SITL (`docs/setup-wsl2.md`)
- [x] [C] Implémenter `FailsafeMonitor::update` (les 15 tests passent)
- [x] [T] Premier vol SITL : décollage, sortie de geofence, vérifier le RTL dans les logs
- [x] [T+C] Vérifier que le heartbeat QGC arrive bien sur le port 14540 (forwarding PX4)
- [x] Journal du mode de vol PX4 et des accusés de réception (`COMMAND_ACK`)
- [ ] Vidéo de démo du vol de sortie de zone
- [ ] [T+C] Test d'intégration scripté : un binaire `vigie-scenario` qui arme, décolle,
      part en ligne droite vers l'extérieur de la zone et vérifie que le mode passe en RTL

## Semaine 2 — Vision embarquée

- [x] [C] ADR 0002 : YOLOX-Nano (Apache-2.0) plutôt que YOLOv8 (AGPL), `tract` plutôt qu'`ort`
- [x] [C] Crate `vigie-vision` : letterbox, décodage YOLOX, NMS, détecteur `tract`,
      test d'intégration contre la référence onnxruntime, outil `vigie-detect`
- [ ] [C] Source d'images : flux RTP/H.264 du plugin GstCameraSystem (UDP 5600) via GStreamer
- [ ] [T] Règle de décision liée à la vision (ex. personne détectée sous le drone → refus
      d'atterrir, ou cible suivie perdue → maintien de position)
- [ ] [T+C] Mesure de latence par étape (`tracing` spans), budget < 100 ms par image en CPU

## Semaine 3 — Yocto

- [ ] [C] `meta-vigie` : `layer.conf`, recette `vigied` (classe cargo), service systemd,
      `vigie.toml` dans `/etc/vigie`
- [ ] [T] Image `vigie-image` pour `qemuarm64`, rootfs en lecture seule, utilisateur dédié
      sans privilèges, durcissement systemd (`ProtectSystem=strict`, `NoNewPrivileges`, …)
- [ ] [T] SBOM SPDX (`create-spdx`) et rapport `cve-check`
- [ ] [T+C] `vigied` dans QEMU qui parle au SITL de l'hôte (réseau QEMU `slirp` ou `tap`)

## Semaine 4 — Liaison sol et finitions

- [ ] [C] Crate `vigie-link` : handshake Noise `XX` (crate `snow`), trame, anti-rejeu
- [ ] [T] Station sol minimale (CLI ou web) qui affiche télémétrie et alertes
- [ ] [T] Fuzzing du décodeur de trames (`cargo-fuzz`)
- [ ] [T] Vidéo de démo (2 min) + GIF dans le README
- [ ] [T+C] Relecture complète, README final
