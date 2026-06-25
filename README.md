# Stagcrest

Mod-first voxel engine in Rust: **server-authoritative** simulation, **Bevy** native client for rendering and UI, **wasmi** for WASM mods. Every block, texture, and redstone rule comes from mods — the `stagcrest-core` mod provides vanilla content.

## Architecture

- **stagcrest-server** — mods (wasmi), worldgen, block world, circuits, persistence (redb), chunk streaming
- **stagcrest-client** — Bevy rendering, meshing, UI, input, raycast against a world replica
- **stagcrest-net** — postcard-framed TCP (remote) or in-process transport (embedded single-player)
- Mods compile to `wasm32-unknown-unknown` cdylibs and export `_stagcrest_register()`. The server loads mod `.wasm` bytes through **wasmi**.

Single-player embeds the server in-process (no second terminal). Remote play: run `stagcrest-server`, connect with `stagcrest-client --connect host:port`.

## Features

- Creative mode: fly, place/break blocks, hotbar with block previews, creative block picker
- Greedy mesh chunk rendering with texture atlas from mod PNGs
- Server-side chunk streaming with client mesh remeshing (`MeshScheduler`)
- Native world persistence (redb) on the server
- Basic redstone (dust, torch, block, lever, button, repeater) via an event-driven circuit graph (default 10 Hz)
- Main menu → connect / load → in-game flow (Bevy UI)
- Native desktop only (no web/WASM host)

## Requirements

- Rust 1.78+

## Build core mod (required)

Mods are built artifacts (not committed). Build before running:

```bash
rustup target add wasm32-unknown-unknown
bash scripts/build-core-mod.sh
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

| Binary             | Flag                     | Description                                     |
| ------------------ | ------------------------ | ----------------------------------------------- |
| `stagcrest-client` | `--connect HOST:PORT`    | Remote server (omit for embedded single-player) |
| `stagcrest-client` | `--net-sim-latency-ms N` | Artificial latency for localhost testing        |
| `stagcrest-server` | `--bind HOST:PORT`       | Listen address (default `0.0.0.0:4242`)         |
| `stagcrest-server` | `--net-sim-latency-ms N` | Artificial latency on outbound frames           |

Press **F3** in-game for debug overlay (position, target block, net transport, RTT from ping).

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

To use Minecraft-format block textures locally:

1. Drop a pack folder under `resourcepacks/` (must contain `pack.mcmeta`).
2. Copy the example manifest:
   ```bash
   cp resourcepacks/resourcepacks.toml.example resourcepacks/resourcepacks.toml
   ```
3. Edit `resourcepacks/resourcepacks.toml`: set `path` to your pack folder name and `enabled = true`.

The server loads block PNGs from `{pack}/assets/minecraft/textures/block/` for textures referenced by `stagcrest-core`.

## Project layout

```
crates/
  stagcrest-protocol    — shared types, ContentManifest
  stagcrest-world       — chunks, raycast
  stagcrest-storage     — chunk persistence (redb)
  stagcrest-mesh        — greedy meshing
  stagcrest-circuit     — event-driven circuit graph
  stagcrest-mod-sdk     — mod author API (host imports)
  stagcrest-mod-server  — wasmi loader, worldgen, server registries
  stagcrest-mod-client  — client content from manifest
  stagcrest-net         — framing, transports, NetConfig
  stagcrest-server      — authoritative simulation (lib + bin)
  stagcrest-render      — chunk mesh → Bevy entities
  stagcrest-client      — Bevy client (menu, loading, game)
mods/
  stagcrest-core/       — air, blocks, redstone, textures
  mods.toml             — mod manifest
resourcepacks/          — local MC-format packs (gitignored)
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
| F3                   | Debug overlay                                         |
| Escape               | Release cursor / pause                                |

## Mod API

Mods export `_stagcrest_register()` and import from module `stagcrest_host`:

| Import                   | Signature                                       | Payload                                                                          |
| ------------------------ | ----------------------------------------------- | -------------------------------------------------------------------------------- |
| `register_block`         | `(ptr: i32, len: i32) -> i32`                   | UTF-8 JSON → block definition                                                    |
| `register_texture`       | `(ptr: i32, len: i32) -> i32`                   | UTF-8 JSON → RGBA texture                                                        |
| `log_message`            | `(ptr: i32, len: i32)`                          | UTF-8 string                                                                     |
| `load_texture_from_pack` | `(name_ptr, name_len, out_ptr, out_max) -> i32` | Load MC-format block PNG from host resource packs; returns bytes written or `-1` |

Mods must export WebAssembly `memory`. See `mods/stagcrest-core/src/content.rs` for a full example.

Build a mod:

```bash
cd mods/your-mod
cargo build --release --target wasm32-unknown-unknown
```

Add an entry to `mods/mods.toml` pointing at the `.wasm` file.

## License

MIT OR Apache-2.0
