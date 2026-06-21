#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/mods/stagcrest-core"
cargo build --release --target wasm32-unknown-unknown
mkdir -p "$ROOT/mods/stagcrest-core"
cp "$ROOT/target/wasm32-unknown-unknown/release/stagcrest_core.wasm" "$ROOT/mods/stagcrest-core/stagcrest-core.wasm"
