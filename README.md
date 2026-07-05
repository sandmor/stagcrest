# Stagcrest

Mod-first voxel engine in Rust: **server-authoritative** simulation, **Bevy** native client for rendering and UI, **wasmtime** (Component Model + WIT) for WASM mods. Every block, texture, and circuit rule comes from mods — the `stagcrest-core` mod provides default content.

## Architecture

- **stagcrest-server** — mods (wasmtime components), worldgen, block world, circuits, persistence (redb), chunk streaming
- **stagcrest-client** — Bevy rendering, meshing, UI, input, raycast against a world replica
- **stagcrest-net** — postcard-framed TCP (remote) or in-process transport (embedded single-player)
- Mods compile to WebAssembly **components** (`wasm32-unknown-unknown` + `wasm-tools component embed/new`). The server loads `.wasm` components through **wasmtime** using the WIT package in `wit/`.

Single-player embeds the server in-process (no second terminal). Remote play: run `stagcrest-server`, connect with `stagcrest-client --connect host:port`.

## Features

- Creative mode: fly, place/break blocks, hotbar with block previews, creative block picker
- Greedy mesh chunk rendering with texture atlas from mod PNGs
- Server-side chunk streaming with client mesh remeshing (`MeshScheduler`)
- Native world persistence (redb) on the server
- Basic circuits (wire, inverter, source, switch, repeater) via an event-driven circuit graph (default 10 Hz)
- Main menu → connect / load → in-game flow (Bevy UI)
- Linux, macOS, and Windows desktop (no web/WASM host)

## Requirements

- Rust 1.95+
- **Linux**: system packages for Bevy (see CI workflow for the full list)
- **macOS**: Xcode Command Line Tools (includes Metal support)
- **Windows**: Visual Studio Build Tools or MSVC toolchain (DirectX 12 support)

## Nightly builds

Pre-built binaries for Linux, macOS, and Windows are published on every push to `main`:

**[Releases → Nightly](https://github.com/sandmor/stagcrest/releases/tag/nightly)**

Download the archive for your platform, extract it, and run `stagcrest-client` from that folder (see `RUN.txt` inside).

## Build core mod (required)

Mods are built artifacts (not committed). Build before running:

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-tools --locked
bash scripts/build-core-mod.sh      # Linux/macOS
# or on Windows PowerShell:
# .\scripts\build-core-mod.ps1
```

This produces `mods/stagcrest-core/stagcrest-core.wasm`.

## Run (single-player)

Embedded server — no TCP latency:

```bash
bash scripts/build-core-mod.sh
cargo run -p stagcrest-client
# or
bash scripts/run-dev.sh
```

Run from the repo root so the server finds `mods/mods.toml`. Saves are written to `worlds/<name>/world.redb` on the server (gitignored).

## Run (dedicated server + client)

Terminal 1 — server:

```bash
bash scripts/build-core-mod.sh
cargo run -p stagcrest-server -- --bind 0.0.0.0:4242
```

Terminal 2 — client:

```bash
cargo run -p stagcrest-client -- --connect 127.0.0.1:4242
```

### CLI flags

| Binary             | Flag                         | Description                                     |
| ------------------ | ---------------------------- | ----------------------------------------------- |
| `stagcrest-client` | `--connect HOST:PORT`        | Remote server (omit for embedded single-player) |
| `stagcrest-client` | `--net-sim-latency-ms N`     | Artificial latency for localhost testing        |
| `stagcrest-server` | `--bind HOST:PORT`           | Listen address (default `0.0.0.0:4242`)         |
| `stagcrest-server` | `--net-sim-latency-ms N`     | Artificial latency on outbound frames           |
| `stagcrest-server` | `export-minimap` subcommand  | PNG minimap of all saved chunks (see below)     |
| `stagcrest-server` | `build-map` subcommand       | Pregenerate world chunks in a circular region   |
| `stagcrest-server` | `rebuild-minimap` subcommand | Rebuild stored minimap tiles from world chunks  |

Export a minimap PNG from explored/saved terrain (streams chunks from disk; does not load the full world into memory):

```bash
cargo run -p stagcrest-server -- export-minimap \
  --world default \
  --output worlds/default/minimap.png
```

Optional: `--scale N` (blocks per pixel, default 1), `--padding N` (extra border around saved bbox, default 64), `--rebuild-minimap` (rebuild map tiles before export), `--jobs N` (rayon threads).

Pregenerate world terrain in a circle around spawn (full vertical column, default radius 16 chunks). Map tiles for the built area are rebuilt automatically when chunks are saved:

```bash
cargo run -p stagcrest-server -- build-map \
  --world default \
  --radius 16 \
  --center-x 8 \
  --center-z 8
```

Optional: `--seed`, `--force` (regenerate existing chunks), `--jobs N`.

Rebuild all minimap tiles from saved world chunks without exporting PNG:

```bash
cargo run -p stagcrest-server -- rebuild-minimap --world default
```

Press **F3** in-game for debug overlay (position, target block, net transport, RTT from ping). Press **M** for the minimap (top-right); **+** / **-** to zoom. The HUD minimap uses a per-column color cache with incremental framebuffer compositing (pan scrolls the buffer and resolves only new edge columns; zoom re-composites from cache).

## Networking

- **In-process** (default): zero-lag single-player; same postcard framing as TCP without sockets.
- **TCP remote**: `TCP_NODELAY`, tuned socket buffers, server priority/bulk send queues so block updates are not blocked behind chunk snapshots.
- Protocol: versioned handshake → `ContentManifest` → initial spawn → streaming `ChunkSnapshot`s and `BlockUpdate`s.

## Chunk streaming and meshing

The **server** generates/loads chunks and sends compressed `ChunkSnapshot` frames. The **client** integrates chunks into a read-only `WorldReplica` and remeshes via `MeshScheduler`:

| Urgency     | Source                               |
| ----------- | ------------------------------------ |
| Interactive | Player place/break (from server ack) |
| Circuit     | Redstone power visual updates        |
| Visible     | Chunk integrate                      |
| Background  | Remaining dirty-chunk drain          |

## Resource packs (optional)

Texture packs are **not included** in the repo. A fresh clone runs with flat-color block placeholders and bundled/procedural biome colormaps.

**In the client:** use **Resource Packs** on the main menu to download packs from Modrinth. You can enable multiple packs and reorder priority.

**Advanced / dedicated server:** configure `data/settings.toml` (see `data/settings.toml.example`). Pack folders live under `data/resourcepacks/` and must contain `pack.mcmeta`.

The server loads block PNGs from `{pack}/assets/minecraft/textures/block/` (or legacy `{pack}/minecraft/textures/block/`) for textures referenced by `stagcrest-core`.

## Project layout

```
crates/
  stagcrest-protocol    — shared types, ContentManifest
  stagcrest-world       — chunks, raycast
  stagcrest-storage     — chunk persistence (redb)
  stagcrest-mesh        — greedy meshing
  stagcrest-circuit     — event-driven circuit graph
  stagcrest-mod-sdk     — mod author API (host imports)
  stagcrest-mod-server  — wasmtime component loader, worldgen, server registries
  stagcrest-mod-client  — client content from manifest
  stagcrest-net         — framing, transports, NetConfig
  stagcrest-server      — authoritative simulation (lib + bin)
  stagcrest-render      — chunk mesh → Bevy entities
  stagcrest-minimap     — column cache, resolve/composite, strip export for PNG
  stagcrest-modrinth    — Modrinth API client
  stagcrest-content     — resource pack settings and installation
  stagcrest-client      — Bevy client (menu, loading, game)
mods/
  stagcrest-core/       — air, blocks, redstone, textures
  mods.toml             — mod manifest
data/resourcepacks/     — local MC-format packs (gitignored)
data/settings.toml      — content settings (see settings.toml.example)
```

## Controls

| Input                | Action                                                |
| -------------------- | ----------------------------------------------------- |
| Main menu Play       | Connect (embedded or remote)                          |
| WASD / Space / Shift | Fly                                                   |
| Mouse                | Look (after click to capture)                         |
| LMB                  | Break block                                           |
| RMB                  | Place / toggle redstone component                     |
| Middle-click         | Pick looked-at block into selected hotbar slot        |
| 1–9                  | Hotbar slot                                           |
| Scroll wheel         | Cycle hotbar slot                                     |
| E                    | Creative inventory (search, drag-drop, block catalog) |
| M                    | Toggle minimap (top-right)                            |
| + / -                | Minimap zoom in / out                                 |
| F3                   | Debug overlay                                         |
| Escape               | Release cursor / pause                                |

## Mod API

Mods are WebAssembly **components** defined by the WIT package `stagcrest:plugin` in `wit/`. Guest exports (implemented via `wit-bindgen` in the mod crate):

| Export                                                         | Purpose                                                                 |
| -------------------------------------------------------------- | ----------------------------------------------------------------------- |
| `register`                                                     | Register blocks, textures, biomes, features, and commands with the host |
| `handle-command`                                               | Handle dispatched slash commands                                        |
| `on-place` / `on-break` / `on-use`                             | Block lifecycle hooks (optional per-block via `callbacks` flags)        |
| `on-neighbor-changed` / `on-scheduled-tick` / `on-random-tick` | Simulation hooks                                                        |
| `state-for-place` / `dynamic-light`                            | Placement state and per-instance lighting                               |

Host imports (called from the mod during `register` or callbacks) include `register-block`, `register-texture`, `register-command`, `log`, and a runtime `world` API (`get-block`, `set-block`, `schedule-tick`, world time, chat).

Block definitions use `behavior` (native redstone id or WASM). Redstone components in `stagcrest-core` use native behaviors; mod special blocks can opt into WASM callbacks.

See `mods/stagcrest-core/src/bindings.rs` and `mods/stagcrest-core/src/content.rs` for a full example.

Build a mod:

```bash
bash scripts/build-core-mod.sh   # builds stagcrest-core component
# or for your own mod:
cargo build --release --target wasm32-unknown-unknown -p your-mod
wasm-tools component embed wit/ target/wasm32-unknown-unknown/release/your_mod.wasm -o /tmp/embedded.wasm
wasm-tools component new /tmp/embedded.wasm -o your-mod.wasm
```

Add an entry to `mods/mods.toml` pointing at the `.wasm` file.

## License

MIT OR Apache-2.0
