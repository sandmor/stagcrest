#!/usr/bin/env pwsh
$ErrorActionPreference = "Stop"
$ROOT = Split-Path -Parent $PSScriptRoot

Push-Location "$ROOT/mods/stagcrest-core"
try {
    cargo build --release --target wasm32-unknown-unknown -p stagcrest-core
} finally {
    Pop-Location
}

$CORE_WASM = Join-Path $ROOT "target/wasm32-unknown-unknown/release/stagcrest_core.wasm"
$OUT_WASM = Join-Path $ROOT "mods/stagcrest-core/stagcrest-core.wasm"
$EMBEDDED_WASM = Join-Path $ROOT "target/wasm32-unknown-unknown/release/stagcrest_core.embedded.wasm"
$WIT = Join-Path $ROOT "wit"

if (-not (Get-Command wasm-tools -ErrorAction SilentlyContinue)) {
    Write-Error @"
wasm-tools not found on PATH.
Install it with: cargo install wasm-tools --locked
"@
}

wasm-tools component embed $WIT $CORE_WASM -o $EMBEDDED_WASM
wasm-tools component new $EMBEDDED_WASM -o $OUT_WASM
Write-Host "built component mod: $OUT_WASM"
