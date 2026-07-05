#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/mods/stagcrest-core"

cargo build --release --target wasm32-unknown-unknown -p stagcrest-core

CORE_WASM="$ROOT/target/wasm32-unknown-unknown/release/stagcrest_core.wasm"
OUT_WASM="$ROOT/mods/stagcrest-core/stagcrest-core.wasm"
EMBEDDED_WASM="$ROOT/target/wasm32-unknown-unknown/release/stagcrest_core.embedded.wasm"

if ! command -v wasm-tools >/dev/null 2>&1; then
  echo "error: wasm-tools not found on PATH" >&2
  echo "Install with: cargo install wasm-tools --locked" >&2
  exit 1
fi

wasm-tools component embed "$ROOT/wit" "$CORE_WASM" -o "$EMBEDDED_WASM"
wasm-tools component new "$EMBEDDED_WASM" -o "$OUT_WASM"
echo "built component mod: $OUT_WASM"
