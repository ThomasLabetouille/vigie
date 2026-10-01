#!/usr/bin/env bash
# Lance PX4 SITL (quadricoptère x500 dans Gazebo).
# Usage : scripts/sitl.sh [--headless]
set -euo pipefail

PX4_DIR="${PX4_DIR:-$HOME/PX4-Autopilot}"
[[ -d "$PX4_DIR" ]] || { echo "PX4 introuvable dans $PX4_DIR (variable PX4_DIR)" >&2; exit 1; }

if [[ "${1:-}" == "--headless" ]]; then
  export HEADLESS=1
fi

cd "$PX4_DIR"
exec make px4_sitl gz_x500
