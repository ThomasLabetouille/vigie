# Vigie

![CI](https://github.com/ThomasLabetouille/vigie/actions/workflows/ci.yml/badge.svg)

Superviseur de sûreté pour drone, écrit en Rust, qui tourne sur le companion computer à côté
de l'autopilote PX4. Vigie lit l'état du véhicule en MAVLink, applique une geofence et des
failsafes (batterie, perte du lien sol) et reprend la main avec un RTL ou un atterrissage quand
une règle est franchie. La cible finale est une image Linux embarquée construite avec Yocto.

Tout tourne en simulation : PX4 SITL et Gazebo pour le véhicule, QGroundControl comme station
sol, QEMU aarch64 pour la cible (à venir).

## Vols en simulation

Le drone décolle depuis QGroundControl puis il est envoyé vers un point situé hors de la zone
autorisée (PX4 v1.17 SITL, Gazebo). Approche et franchissement, journal de `vigied` (champs raccourcis) :

```
07:43:29  INFO  état armed=true alt_m="3.0" marge_zone="67 m" lien_sol="il y a 0.9 s" etat=Nominal
07:43:34  INFO  état armed=true alt_m="3.0" marge_zone="42 m" lien_sol="il y a 0.2 s" etat=Nominal
07:43:39  WARN  alerte reason=FenceProximity
07:43:39  INFO  état armed=true alt_m="3.0" marge_zone="17 m" lien_sol="il y a 0.5 s" etat=Warning
07:43:43  ERROR failsafe : retour au point de décollage reason=FenceBreach
07:43:43  WARN  envoi de commande à l'autopilote cmd=ReturnToLaunch
```

Côté PX4, au même moment :

```
INFO  [commander] Returning to launch
INFO  [navigator] RTL: start return at 31 m (30 m above destination)
```

Sur un second vol, après l'ajout du suivi du mode de vol et des accusés de réception, le retour
complet :

```
07:55:42.626  ERROR failsafe : retour au point de décollage reason=FenceBreach
07:55:42.642  INFO  commande acceptée par PX4 command=MAV_CMD_NAV_RETURN_TO_LAUNCH
07:55:43.618  INFO  mode de vol PX4 mode=ReturnToLaunch
07:56:01      INFO  état mode=ReturnToLaunch alt_m="29.7" marge_zone="24 m" etat=Rtl
07:56:23      INFO  état mode=ReturnToLaunch alt_m="29.9" marge_zone="90 m" etat=Rtl
07:57:04      INFO  état mode=ReturnToLaunch alt_m="0.3"  marge_zone="107 m" etat=Rtl
07:57:08      INFO  drone désarmé
```

PX4 accuse réception environ 16 ms après l'envoi. Le drone monte à son altitude de retour, rentre dans
la zone, attend au-dessus du point de décollage puis se pose. Pendant l'attente à 30 m, la marge
affichée est de 90 m et non 107 m : c'est le plafond de 120 m qui devient la limite la plus
proche.

## Organisation

```
crates/
  vigie-core/      no_std, sans allocation : projection locale, geofence, machine à états des failsafes
  vigie-mavlink/   pont MAVLink async (tokio) : décodage pur + boucle réseau
  vigied/          le démon : configuration TOML, superviseur à 10 Hz
config/vigie.toml  configuration pour PX4 SITL
docs/adr/          décisions d'architecture
docs/              installation, planning
scripts/           lancement du SITL, installation Ubuntu et Windows/WSL2
```

```
PX4 SITL ──UDP 14540──▶ vigie-mavlink ──watch<VehicleState>──▶ superviseur 10 Hz
    ▲                                                              │
    └──────────── COMMAND_LONG (RTL / LAND) ◀── mpsc<Command> ◀────┘
```

`vigie-core` ne fait aucune I/O et ne lit jamais l'horloge : le temps lui est passé en
paramètre. Chaque décision de sûreté se rejoue donc dans un test, et la crate compile pour
Cortex-M (`thumbv7em-none-eabihf`). La CI le vérifie à chaque push, pour garder la possibilité de
déplacer cette logique sur un microcontrôleur de supervision indépendant. Les raisons des autres
choix (PX4 plutôt qu'ArduPilot, projection équirectangulaire, `watch` plutôt qu'une file pour
l'état) sont dans [l'ADR 0001](docs/adr/0001-architecture.md).

## Règles de sûreté

Par ordre de priorité :

1. batterie critique (≤ 15 %) : atterrissage immédiat
2. sortie de geofence, horizontale ou au-dessus du plafond : RTL
3. aucun heartbeat de station sol depuis plus de 5 s : RTL
4. batterie faible (≤ 30 %) ou bord de zone à moins de 20 m : alerte

Le moniteur ne fait qu'escalader (`Nominal` → `Warning` → `Rtl` → `Landing`) et n'émet une
commande qu'au moment d'un changement d'état, jamais en rafale. Seul le désarmement le remet à
zéro. Une donnée absente n'est jamais prise pour une donnée saine : si aucun heartbeat sol n'a été
reçu depuis l'armement, le délai court depuis l'armement.

Si le pilote annule le RTL depuis la station sol, Vigie ne renvoie pas la commande : le pilote
garde la main. Le journal affiche le mode de vol réel de PX4, ce qui rend la situation visible.

La spécification complète est en tête de `crates/vigie-core/src/failsafe.rs`, et
`crates/vigie-core/tests/failsafe.rs` la vérifie cas par cas (15 tests).

## Lancer

Installation sous Windows avec WSL2 : [docs/setup-wsl2.md](docs/setup-wsl2.md). Sous Ubuntu 24.04,
`scripts/setup-ubuntu.sh` installe Rust et PX4.

```bash
cargo test --workspace

# Terminal 1 : simulateur
scripts/sitl.sh                  # --headless pour se passer de la fenêtre Gazebo

# Terminal 2 : Vigie
cargo run -p vigied -- --config config/vigie.toml
# --dry-run : journalise les décisions sans commander le drone
```

QGroundControl se connecte tout seul au SITL. Sans lui, PX4 v1.17 refuse d'armer : dans la
console `pxh>`, `param set NAV_DLL_ACT 0` lève ce contrôle, et Vigie déclenche alors un RTL pour
perte du lien sol 5 s après l'armement.

## Qualité

La CI (GitHub Actions) passe `rustfmt`, `clippy` en mode pedantic avec les avertissements
bloquants, les tests, la compilation `no_std` pour Cortex-M, une vérification avec la version
minimale de Rust (1.88) et `cargo-deny` (licences, avis RustSec, provenance des crates).

Deux avis RustSec sur `quick-xml` sont ignorés dans `deny.toml`, avec la raison : la crate n'est
utilisée qu'à la compilation par `mavlink-bindgen`, pour lire les XML de dialecte MAVLink livrés
avec la crate, jamais sur une entrée externe.

## État

Semaine 1 sur 4 terminée : supervision MAVLink, geofence et failsafes validés en vol simulé.
Détail dans [docs/planning.md](docs/planning.md).

À venir :

- détection d'objets embarquée (ONNX) sur le flux caméra de Gazebo
- couche Yocto `meta-vigie` : image `qemuarm64`, rootfs en lecture seule, services systemd
  durcis, SBOM SPDX et rapport CVE
- liaison sol chiffrée (protocole Noise) avec une station sol minimale

Le lien sol surveillé est pour l'instant le heartbeat de QGroundControl relayé par PX4 ; il sera
remplacé par celui de la liaison chiffrée.

## Licence

MIT
