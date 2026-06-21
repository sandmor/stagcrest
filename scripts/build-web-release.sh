#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

rustup target add wasm32-unknown-unknown

bash "$ROOT/scripts/build-core-mod.sh"

if ! command -v trunk &>/dev/null; then
  echo "error: trunk not found (install with: cargo install trunk --locked)" >&2
  exit 1
fi

# Trunk 0.21+ rejects NO_COLOR=1 (expects true/false).
unset NO_COLOR
trunk build --release

echo "Web release built to $ROOT/dist"
