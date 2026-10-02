#!/usr/bin/env bash
# Prépare Ubuntu 24.04 (WSL2 ou natif) pour Vigie : paquets, Rust, PX4 SITL.
# À lancer depuis la racine du dépôt. Relançable : les étapes faites sont sautées.
set -euo pipefail

PX4_TAG="v1.17.0"
PX4_DIR="${PX4_DIR:-$HOME/PX4-Autopilot}"
REPO="$(cd "$(dirname "$0")/.." && pwd)"
log() { printf '\n\033[1;36m==> %s\033[0m\n' "$*"; }

log "Paquets système"
sudo apt-get update -q
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q \
  build-essential pkg-config git curl ca-certificates

log "Rust"
if ! command -v cargo >/dev/null 2>&1 && [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal -c clippy,rustfmt,rust-analyzer
fi
# shellcheck disable=SC1091
source "$HOME/.cargo/env"
rustup target add thumbv7em-none-eabihf aarch64-unknown-linux-gnu

log "Compilation et tests de Vigie"
cd "$REPO"
cargo test --workspace

log "PX4 ${PX4_TAG} dans ${PX4_DIR}"
if [[ ! -d "$PX4_DIR" ]]; then
  git clone --recursive -b "$PX4_TAG" https://github.com/PX4/PX4-Autopilot.git "$PX4_DIR"
fi
# --no-nuttx : pas de toolchain pour carte réelle, uniquement la simulation.
bash "$PX4_DIR/Tools/setup/ubuntu.sh" --no-nuttx
make -C "$PX4_DIR" px4_sitl

log "Prêt"
echo "Simulateur : scripts/sitl.sh   |   Vigie : cargo run -p vigied -- --config config/vigie.toml"
