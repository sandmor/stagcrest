#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PLATFORM="${1:?platform label, e.g. linux-x86_64}"
EXE_EXT="${2:-}"

STAGING="$ROOT/dist/stagcrest-nightly"
rm -rf "$STAGING"
mkdir -p "$STAGING/mods/stagcrest-core"

cp "$ROOT/target/release/stagcrest-client${EXE_EXT}" "$STAGING/"
cp "$ROOT/target/release/stagcrest-server${EXE_EXT}" "$STAGING/"
cp "$ROOT/mods/mods.toml" "$STAGING/mods/"
cp "$ROOT/mods/stagcrest-core/stagcrest-core.wasm" "$STAGING/mods/stagcrest-core/"

cat >"$STAGING/RUN.txt" <<'EOF'
Stagcrest nightly build

Run from this directory:
  ./stagcrest-client          single-player (embedded server)
  ./stagcrest-server        dedicated multiplayer server

Optional: add Minecraft-format resource packs under data/resourcepacks/
(see data/resourcepacks/resourcepacks.toml.example in the source repo).
EOF

mkdir -p "$ROOT/dist"
rm -f "$ROOT/dist/stagcrest-nightly-${PLATFORM}"*

case "$(uname -s)" in
MINGW* | MSYS* | CYGWIN* | Windows*)
  # Built-in tar on Windows runners; avoids PowerShell/Git Bash path translation issues.
  (
    cd "$ROOT/dist"
    tar -a -cf "stagcrest-nightly-${PLATFORM}.zip" stagcrest-nightly
  )
  ;;
*)
  tar -czf "$ROOT/dist/stagcrest-nightly-${PLATFORM}.tar.gz" -C "$ROOT/dist" stagcrest-nightly
  ;;
esac
