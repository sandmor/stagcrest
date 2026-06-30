#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"
$ROOT = Split-Path -Parent $PSScriptRoot
Push-Location "$ROOT/mods/stagcrest-core"
cargo build --release --target wasm32-unknown-unknown
$null = New-Item -ItemType Directory -Force -Path "$ROOT/mods/stagcrest-core"
Copy-Item "$ROOT/target/wasm32-unknown-unknown/release/stagcrest_core.wasm" "$ROOT/mods/stagcrest-core/stagcrest-core.wasm"
Pop-Location
