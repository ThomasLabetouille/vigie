# ADR 0001 — Architecture générale de Vigie

- Statut : accepté
- Date : 2026-10-01

## Contexte

Vigie est un logiciel de companion computer pour drone : il tourne à côté de
l'autopilote (PX4), observe l'état du véhicule et peut reprendre la main
(RTL, atterrissage) si une règle de sûreté est violée. Il ajoute une détection
d'objets embarquée sur le flux caméra et une liaison télémétrie chiffrée vers
une station sol.

Contraintes :

- tout doit tourner en simulation sur un PC Windows (WSL2), sans matériel ;
- la cible finale est une image Linux embarquée construite avec Yocto,
  démarrée dans QEMU (aarch64) ;
- la partie qui décide d'une action sur le véhicule doit pouvoir être testée
  de façon exhaustive, sans réseau ni simulateur.

## Décision

### Découpage en crates

```
vigie-core      no_std, sans allocation. Géométrie, geofence, machine à états des failsafes.
vigie-mavlink   I/O MAVLink async (tokio). Conversions pures + boucle réseau.
vigie-vision    (semaine 2) détection ONNX sur le flux caméra Gazebo.
vigie-link      (semaine 4) liaison sol chiffrée (Noise).
vigied          binaire qui assemble le tout, configuré en TOML.
meta-vigie      (semaine 3) couche Yocto : recettes, services systemd, image.
```

La frontière importante est entre `vigie-core` et le reste. `vigie-core`
ne fait aucune I/O et ne lit pas l'horloge : le temps arrive en paramètre
(`now_ms`). Toute décision est donc rejouable dans un test unitaire ou un
test de propriétés, et la crate pourrait être embarquée telle quelle sur un
microcontrôleur de supervision indépendant. La CI compile `vigie-core` pour
`thumbv7em-none-eabihf` pour garder cette propriété.

### Flux de données dans `vigied`

```
PX4 SITL ──UDP 14540──▶ vigie-mavlink ──watch<VehicleState>──▶ superviseur (10 Hz)
    ▲                                                              │
    └──────────── COMMAND_LONG ◀── mpsc<Command> ◀─────────────────┘
```

- `watch` pour l'état : le superviseur veut la dernière valeur, pas
  l'historique. Pas de file qui grossit si le superviseur prend du retard.
- `mpsc` borné (8) pour les commandes : elles sont rares (une par changement
  d'état grâce à l'invariant « pas de rafale » du moniteur).
- Le superviseur tourne sur un `interval` à 10 Hz avec
  `MissedTickBehavior::Skip` : après une pause (debugger, machine chargée), on
  ne rattrape pas les ticks manqués en rafale.

### Règles de sûreté

La table de priorité et les invariants (escalade seulement, une commande par
transition, donnée absente ≠ donnée saine) sont documentés en tête de
`vigie-core/src/failsafe.rs` et vérifiés par `vigie-core/tests/failsafe.rs`.

### Choix techniques

| Sujet | Choix | Alternatives écartées |
|---|---|---|
| Autopilote simulé | PX4 v1.17 SITL + Gazebo (gz) | ArduPilot SITL : très bien aussi, mais PX4 est plus courant dans l'écosystème défense européen et son SITL Gazebo fournit directement un flux caméra. |
| MAVLink | crate `mavlink` 0.18, dialecte `common`, async tokio | `mavsdk` (bindings C++) : on veut du Rust pur pour la cross-compilation Yocto. |
| Projection géo | équirectangulaire locale | ENU via ECEF : inutile sous 10 km, voir `geo.rs`. |
| Polygone | `heapless::Vec<_, 32>` | `Vec` : demanderait un allocateur dans `vigie-core`. |
| Distribution | Yocto 6.0 « Wrynose » (LTS) | Buildroot : plus simple, mais Yocto est ce qu'on trouve en production dans le secteur. |
| Édition Rust | 2024, MSRV 1.88 (let-chains) | La toolchain Rust de Wrynose doit être ≥ 1.88 ; à confirmer en semaine 3 (`meta/recipes-devtools/rust`). |

## Conséquences

- Les tests de `vigie-core` couvrent la logique de sûreté sans simulateur ;
  le simulateur ne sert qu'aux tests d'intégration et à la démo.
- Le lien sol surveillé par le failsafe est, en semaine 1, le heartbeat de
  QGroundControl relayé par PX4 (forwarding MAVLink activé sur l'instance
  onboard du SITL — à vérifier en pratique). Il sera remplacé par le
  heartbeat de `vigie-link` en semaine 4.
- Bug amont contourné : `mavlink-core` 0.18 utilise `futures::lock` sans
  activer la feature `std` de `futures` quand on coupe les features par défaut
  de `mavlink`. Le workspace l'active explicitement (voir `Cargo.toml`).
