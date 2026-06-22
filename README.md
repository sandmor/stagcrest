# Stagcrest

Mod-first voxel engine in Rust: **Bevy** for UI and app shell, **wgpu** (via Bevy mesh rendering) for the world, **wasmi** for WASM mods. Every block, texture, and redstone rule comes from mods — the `stagcrest-core` mod provides vanilla content.

## Architecture

- Mods compile to `wasm32-unknown-unknown` cdylibs and export `_stagcrest_register()`.
- The host loads mod `.wasm` bytes through **wasmi** on both native and web (same pipeline).
- Native reads assets from the repo filesystem; the web build fetches the same paths over HTTP (Trunk copies `mods/`, `assets/`, and any local `resourcepacks/` content into the bundle).
- Mods call host imports in the `stagcrest_host` module to register blocks, textures, and log messages.

## Features

- Creative mode: fly, place/break blocks, hotbar with block previews, creative block picker
- Greedy mesh chunk rendering with texture atlas from mod PNGs
- Basic redstone (dust, torch, block, lever, button, repeater) via a 10 Hz circuit graph engine
- Main menu → mod loading → in-game flow (Bevy UI)
- Native desktop + WASM/web

## Requirements

- Rust 1.78+
- For web: [Trunk](https://trunkrs.io/) and `wasm32-unknown-unknown` target

## Build core mod (required)

Mods are built artifacts (not committed). Build before running native or web:

```bash
rustup target add wasm32-unknown-unknown
bash scripts/build-core-mod.sh
```

This produces `mods/stagcrest-core/stagcrest-core.wasm`.

## Run (native)

```bash
cd /path/to/stagcrest
bash scripts/build-core-mod.sh
cargo run -p stagcrest-app
```

Run from the repo root so the engine finds `mods/mods.toml`. Resource packs are optional (see below).

## Run (web)

```bash
bash scripts/build-core-mod.sh
cargo install trunk
trunk serve
```

Open the URL Trunk prints (usually `http://127.0.0.1:8080`). Trunk serves:

- `mods/` — mod manifest and `.wasm` binaries
- `assets/` — bundled colormaps and shaders
- `resourcepacks/` — local Minecraft-format resource packs (if configured)

Requires a modern browser with WebGPU (preferred) or WebGL2 fallback.

## Build (web release)

Production static bundle for hosting (e.g. Cloudflare Pages). Output goes to `dist/`.

```bash
# Optional: slim a full local pack down to the textures the engine needs
bash scripts/prepare-web-resourcepack.sh "resourcepacks/My Pack"

bash scripts/build-web-release.sh
```

`prepare-web-resourcepack.sh` copies only the block textures referenced by `stagcrest-core` (and optional colormaps) into `resourcepacks/web/`, then writes `resourcepacks/resourcepacks.toml`. Skip it if you already have a configured pack or want placeholder colors.

`build-web-release.sh` builds the core mod and runs `trunk build --release`. Requires [Trunk](https://trunkrs.io/) on your `PATH`.

Serve locally to verify:

```bash
npx serve dist
```

## Resource packs (optional)

Texture packs are **not included** in the repo. A fresh clone runs with flat-color block placeholders and bundled/procedural biome colormaps — no setup required.

To use Minecraft-format block textures locally:

1. Drop a pack folder under `resourcepacks/` (must contain `pack.mcmeta`).
2. Copy the example manifest:
   ```bash
   cp resourcepacks/resourcepacks.toml.example resourcepacks/resourcepacks.toml
   ```
3. Edit `resourcepacks/resourcepacks.toml`: set `path` to your pack folder name and `enabled = true`.

The host loads block PNGs from `{pack}/assets/minecraft/textures/block/` for the textures referenced by `stagcrest-core`. If a pack or texture is missing, the core mod falls back to solid-color placeholders.

## Block tinting and colormaps

Grass blocks use **greyscale tint masks** (like vanilla Minecraft): the engine multiplies texture RGB by a color sampled from `colormap/grass.png`. Side faces blend a base texture with an overlay (`grass_block_side_overlay`) where the overlay grey pixels receive the same tint.

| Source                    | Path                                                  |
| ------------------------- | ----------------------------------------------------- |
| Resource pack (preferred) | `{pack}/assets/minecraft/textures/colormap/grass.png` |
| Bundled fallback          | `assets/minecraft/colormap/grass.png`                 |

Tint rules for blocks are curated in `crates/stagcrest-mod-host/src/block_tints.rs`. For MVP, a single fixed Plains-like green (temperature `0.8`, downfall `0.4`) is passed to the voxel shader on native and WASM.

Resource packs can supply block overlay PNGs via the normal block texture path (e.g. `grass_block_side_overlay.png`).

## Project layout

```
crates/
  stagcrest-protocol   — shared types
  stagcrest-world      — chunks, raycast
  stagcrest-mesh       — greedy meshing
  stagcrest-circuit     — 10 Hz event-driven circuit graph interpreter
  stagcrest-mod-sdk    — mod author API (host imports)
  stagcrest-mod-host   — wasmi loader, AssetReader, registries
  stagcrest-render     — chunk mesh → Bevy entities
  stagcrest-app        — Bevy app (menu, loading, game)
mods/
  stagcrest-core/      — air, blocks, redstone, textures
  mods.toml            — mod manifest
resourcepacks/         — local MC-format packs (gitignored; see example manifest)
```

## Controls

| Input                | Action                                              |
| -------------------- | --------------------------------------------------- |
| Main menu Play       | Start loading mods                                  |
| WASD / Space / Shift | Fly                                                 |
| Mouse                | Look (after click to capture)                       |
| LMB                  | Break block                                         |
| RMB                  | Place / toggle redstone component                   |
| Middle-click         | Pick looked-at block into selected hotbar slot      |
| 1–9                  | Hotbar slot                                         |
| Scroll wheel         | Cycle hotbar slot                                   |
| E                    | Creative inventory (search, drag-drop, block catalog) |
| Escape               | Release cursor / pause                              |

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
