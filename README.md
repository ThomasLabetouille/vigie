# Vigie

Logiciel de companion computer pour drone, en Rust. Vigie tourne à côté de
l'autopilote PX4, surveille la position, la batterie et le lien sol, et
déclenche un retour au point de décollage ou un atterrissage quand une règle
de sûreté est franchie. Il est conçu pour être livré dans une image Linux
embarquée construite avec Yocto.

Tout tourne en simulation : PX4 SITL + Gazebo pour le véhicule, QEMU aarch64
pour la cible.

> Projet en cours. État actuel : semaine 1 sur 4, voir [docs/planning.md](docs/planning.md).

## Organisation

```
crates/
  vigie-core/      no_std, sans allocation : geofence, machine à états des failsafes
  vigie-mavlink/   pont MAVLink async (tokio) vers PX4
  vigied/          démon qui assemble le tout, configuré en TOML
config/vigie.toml  configuration pour PX4 SITL
docs/adr/          décisions d'architecture
```

`vigie-core` ne fait aucune I/O et ne lit jamais l'horloge : le temps lui est
passé en paramètre. Chaque décision de sûreté est donc rejouable dans un test,
et la crate compile pour Cortex-M (`thumbv7em-none-eabihf`), ce que la CI
vérifie à chaque push. Le détail est dans
[l'ADR 0001](docs/adr/0001-architecture.md).

```
PX4 SITL ──UDP 14540──▶ vigie-mavlink ──watch<VehicleState>──▶ superviseur 10 Hz
    ▲                                                              │
    └──────────── COMMAND_LONG (RTL / LAND) ◀── mpsc<Command> ◀────┘
```

## Règles de sûreté

Par ordre de priorité :

1. batterie critique → atterrissage immédiat ;
2. sortie de geofence (horizontale ou plafond) → RTL ;
3. lien sol perdu plus de 5 s → RTL ;
4. batterie faible ou bord de zone à moins de 20 m → alerte.

Une fois en RTL ou en atterrissage, Vigie ne revient pas en arrière tout seul,
et n'envoie qu'une commande par changement d'état. Les seuils sont dans
`config/vigie.toml`.

## Lancer

Installation de l'environnement sous Windows/WSL2 : [docs/setup-wsl2.md](docs/setup-wsl2.md).

```bash
cargo test --workspace

# Terminal 1 : simulateur
scripts/sitl.sh            # ou --headless

# Terminal 2 : Vigie
RUST_LOG=info cargo run -p vigied -- --config config/vigie.toml
# --dry-run pour journaliser les décisions sans commander le drone
```

## Feuille de route

- Détection d'objets embarquée (ONNX) sur le flux caméra Gazebo
- Couche Yocto `meta-vigie` : image `qemuarm64`, rootfs en lecture seule,
  services systemd durcis, SBOM SPDX et rapport CVE
- Liaison sol chiffrée (protocole Noise) avec station sol minimale

## Licence

MIT
