# Scene lighting conventions

Canonical reference for world axes, celestial directions, and how lighting data flows
from `TimeOfDay` through shaders. All new rendering code (voxels, sky, water, entities)
should follow these conventions.

## World coordinate system

Stagcrest uses a **Y-up, right-handed** world space (Bevy / glam default):

| Axis   | Direction | Notes                                                                     |
| ------ | --------- | ------------------------------------------------------------------------- |
| **+X** | East      | Sunrise at `cycle ≈ 0.25` points here                                     |
| **-X** | West      | Sunset at `cycle ≈ 0.75`                                                  |
| **+Y** | Up        | Skylight propagates downward from chunk top                               |
| **+Z** | South     | Noon sun sits over the southern horizon (`+Z` horizontal component peaks) |
| **-Z** | North     |                                                                           |

Block mesh face normals use the same axes (`encode_normal_axis` in
`assets/shaders/scene_lighting.wgsl`).

## Two direction conventions

Lighting code uses **two** direction vectors. Mixing them inverts bright/dark sides.

| Name                       | Meaning                                                        | Used by                                                                                                                          |
| -------------------------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| **Position direction**     | Unit vector **from the scene toward** the celestial body       | `TimeOfDay::sun_dir()`, `moon_dir()`; GPU `sun_position_dir`, `moon_position_dir`; sky glow; Lambert `dot(normal, position_dir)` |
| **Light travel direction** | Unit vector **from the sun toward the scene** (incoming light) | `TimeOfDay::sun_light_dir()`; Bevy `DirectionalLight` transform                                                                  |

Relationship: `sun_light_dir = -sun_dir`.

Lambert diffuse for voxels and entities:

```wgsl
max(dot(normalize(normal), normalize(sun_position_dir)), 0.0)
```

A face pointing **toward** the sun has a positive dot product and receives direct sun.

## Day/night cycle (`TimeOfDay`)

Defined in `crates/stagcrest-protocol/src/world_time.rs`, synced server → client.

| `cycle` (0..1) | Phase                                                     |
| -------------- | --------------------------------------------------------- |
| 0.0            | Midnight (sun at nadir below the world, moon near zenith) |
| 0.25           | Sunrise (east, `+X`)                                      |
| 0.5            | Noon (high elevation, over south)                         |
| 0.75           | Sunset (west, `-X`)                                       |

- **`day_factor`**: 0 deep night, ~0.35 at sunrise/sunset, 1 at noon.
- **`sun_disc_factor` / `moon_disc_factor`**: GPU disc visibility packed in celestial `.w`.
- **`moon_dir`**: opposite the sun at twilight (west at sunrise, east at sunset), high at night.

## GPU uniform (`SceneLightingUniform`)

Built each frame in client `sync_scene_lighting` from `WorldTime` and `PlayerEnvironment`.

| Field               | Convention                                                    |
| ------------------- | ------------------------------------------------------------- |
| `sun_position_dir`  | Toward the sun (not light travel); `.w` = sun disc visibility |
| `moon_position_dir` | Toward the moon; `.w` = moon disc visibility                  |
| `params.x`          | `day_factor`                                                  |
| `params.y`          | `cycle` 0..1                                                  |
| `params.z`          | medium (0 = air, 1 = water)                                   |
| `params.w`          | camera submersion 0..1                                        |

Shared WGSL helpers live in `assets/shaders/scene_lighting.wgsl` and are imported by
`voxel.wgsl`, `skybox.wgsl`, and future entity shaders.

## Voxel terrain

Per-vertex **baked** data (mesh build time):

- Skylight + block light levels (0..15)
- Ambient occlusion per corner
- Face normal axis

Per-frame **runtime** data (uniform):

- Sun/moon diffuse via `shade_voxel`
- Biome ambient, horizon colors, underwater absorption

Baked skylight does not move with the sun; runtime sun/moon diffuse adds directional
variation on exposed faces.

## Bevy `DirectionalLight`

Spawned in `game_session.rs`, oriented each frame in `sync_scene_lighting` using
`sun_light_dir()` (light **travel**, not position).

**Purpose:** optional shadow maps on non-voxel content (debug geometry, future meshes).
Voxel terrain does **not** use Bevy shadows; it uses baked light + `shade_voxel`.

When entity rendering lands, prefer `shade_voxel` + foot light sampling (see
`entity_lighting_foundation.md`) for color consistency. Keep the directional light only
if entity meshes need GPU shadow maps.

## Adding new consumers

1. Read `SceneLighting` / `SceneLightingUniform`; do not recompute sun angles locally.
2. For diffuse shading, use **position** directions (`sun_position_dir`), never negate
   unless you explicitly need light travel (Bevy only).
3. Match face normals to world axes when calling `shade_voxel`.
4. Add tests in `world_time` if you change the sun path or axis mapping.
