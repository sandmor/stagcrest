mod block_model;
mod mesh_snapshot;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use stagcrest_mod_client::{
    compute_dust_connections, dust_shows_dot, dust_vertex_tint, face_texture_for,
    is_dust_connectable_neighbor, resolve_block_model, resolve_dust_textures, sample_colormap_rgb,
    BlockRegistry, ColormapSet, DustConnections, DustSide, DustTextures, ModelRegistry,
    PowerLookup,
};
use stagcrest_protocol::{
    fluid_flowing, BlockGeometry, BlockId, BlockPos, BlockState, ChunkPos, FaceTexture, TextureId,
    TintKind, CHUNK_SIZE,
};
use stagcrest_world::ChunkBlock;
use std::collections::{HashMap, HashSet};

pub use block_model::{
    block_selection_bounds, emit_block_model, mesh_bucket_for_layer, MeshBucket, SelectionBounds,
};
pub use mesh_snapshot::{capture_power_grid, MeshClimateSnapshot, MeshSnapshot};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub overlay_uv: [f32; 2],
    pub tint: f32,
    pub overlay_tint: f32,
    pub tint_mul: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct ChunkMesh {
    pub opaque_vertices: Vec<VoxelVertex>,
    pub opaque_indices: Vec<u32>,
    pub transparent_vertices: Vec<VoxelVertex>,
    pub transparent_indices: Vec<u32>,
    pub cutout_vertices: Vec<VoxelVertex>,
    pub cutout_indices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct MeshCache {
    meshes: HashMap<ChunkPos, ChunkMesh>,
    /// Chunks whose CPU meshes changed and need a Bevy mesh upload.
    dirty: HashSet<ChunkPos>,
}

#[derive(Clone)]
pub struct MeshClimateTint<'a> {
    pub colormaps: &'a ColormapSet,
    pub temperature: f32,
    pub downfall: f32,
}

impl MeshCache {
    pub fn get(&self, pos: ChunkPos) -> Option<&ChunkMesh> {
        self.meshes.get(&pos)
    }

    pub fn commit_mesh(&mut self, pos: ChunkPos, mesh: ChunkMesh) {
        self.meshes.insert(pos, mesh);
        self.dirty.insert(pos);
    }

    pub fn meshes(&self) -> &HashMap<ChunkPos, ChunkMesh> {
        &self.meshes
    }

    pub fn mark_all_dirty(&mut self) {
        self.dirty.extend(self.meshes.keys().copied());
    }

    pub fn take_dirty(&mut self) -> HashSet<ChunkPos> {
        std::mem::take(&mut self.dirty)
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.meshes.remove(&pos);
        // Mark dirty so the render sync despawns GPU entities for this chunk.
        self.dirty.insert(pos);
    }
}

/// Build an isolated preview mesh for inventory icons.
pub fn build_single_block_mesh(
    registry: &BlockRegistry,
    models: &ModelRegistry,
    block_id: BlockId,
    state: BlockState,
) -> ChunkMesh {
    build_single_block_mesh_with_power(registry, models, block_id, state, 15)
}

pub fn build_single_block_mesh_with_power(
    registry: &BlockRegistry,
    models: &ModelRegistry,
    block_id: BlockId,
    state: BlockState,
    power: u8,
) -> ChunkMesh {
    build_single_block_mesh_internal(registry, models, block_id, state, power, true)
}

/// Icon preview mesh: blend/cutout cubes render as opaque in the inventory.
pub fn build_single_block_icon_mesh(
    registry: &BlockRegistry,
    models: &ModelRegistry,
    block_id: BlockId,
    state: BlockState,
    power: u8,
) -> ChunkMesh {
    build_single_block_mesh_internal(registry, models, block_id, state, power, false)
}

fn build_single_block_mesh_internal(
    registry: &BlockRegistry,
    models: &ModelRegistry,
    block_id: BlockId,
    state: BlockState,
    power: u8,
    use_block_transparency: bool,
) -> ChunkMesh {
    let Some(def) = registry.block(block_id) else {
        return ChunkMesh::default();
    };
    if !def.solid && !def.opaque && !def.transparent {
        return ChunkMesh::default();
    }

    let mut mesh = ChunkMesh::default();

    if def.namespaced_id == "stagcrest:redstone_dust" {
        let textures = resolve_dust_textures(registry);
        emit_dust(
            &mut mesh,
            [0.0, 0.0, 0.0],
            DustConnections::icon_cross(),
            textures,
            power,
            registry,
            0,
            0,
            None,
            None,
        );
        return mesh;
    }

    let bucket = if use_block_transparency {
        mesh_bucket_for_layer(def.render_layer)
    } else {
        MeshBucket::Opaque
    };

    let face_textures = registry
        .block_face_textures_for_state(block_id, state)
        .unwrap_or(def.face_textures);

    emit_block_geometry(
        &mut mesh,
        [0.0, 0.0, 0.0],
        def.geometry,
        &def.namespaced_id,
        &face_textures,
        bucket,
        registry,
        models,
        power,
        state,
        0,
        0,
        None,
        None,
        |_| false,
        None,
    );
    mesh
}

fn should_cull_face(
    block_def: &stagcrest_protocol::BlockDef,
    neighbor: Option<ChunkBlock>,
    air: BlockId,
    registry: &BlockRegistry,
    normal: Vec3,
) -> bool {
    let Some(neighbor) = neighbor else {
        return false;
    };
    if neighbor.id == air {
        return false;
    }
    let neighbor_def = registry.block(neighbor.id);
    neighbor_culls_face(block_def, neighbor_def, normal)
}

fn neighbor_culls_face(
    block_def: &stagcrest_protocol::BlockDef,
    neighbor_def: Option<&stagcrest_protocol::BlockDef>,
    _normal: Vec3,
) -> bool {
    let Some(neighbor) = neighbor_def else {
        return false;
    };
    if block_def.fluid && neighbor.fluid {
        return true;
    }
    neighbor.opaque && neighbor.solid
}

/// Per-column grass and foliage tint multipliers for a chunk (indexed by local x, z).
type ColumnTintCache = [[([f32; 3], [f32; 3]); CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

fn build_column_tint_cache(climate: &MeshClimateTint<'_>) -> ColumnTintCache {
    let grass = tint_mul_for_kind(TintKind::Grass, Some(climate));
    let foliage = tint_mul_for_kind(TintKind::Foliage, Some(climate));
    let cell = (grass, foliage);
    [[cell; CHUNK_SIZE as usize]; CHUNK_SIZE as usize]
}

pub(crate) fn fluid_flow_textures(
    mut faces: stagcrest_protocol::BlockFaceTextures,
    flow_tex: TextureId,
) -> stagcrest_protocol::BlockFaceTextures {
    let flow = FaceTexture {
        texture: flow_tex,
        overlay: None,
        tint: faces.top.tint,
        overlay_tint: TintKind::None,
    };
    faces.top = flow;
    faces.bottom = flow;
    faces.sides = flow;
    faces
}

pub fn build_chunk_mesh_snapshot(snapshot: &MeshSnapshot) -> ChunkMesh {
    mesh_snapshot::mesh_from_snapshot(snapshot)
}

pub(crate) fn build_chunk_mesh_neighbors(
    chunk_pos: ChunkPos,
    air: BlockId,
    registry: &BlockRegistry,
    models: &ModelRegistry,
    power: Option<&dyn PowerLookup>,
    climate: Option<&MeshClimateTint<'_>>,
    neighbor_at: impl Fn(i32, i32, i32) -> Option<ChunkBlock>,
) -> ChunkMesh {
    let mut mesh = ChunkMesh::default();
    let base_x = chunk_pos.x * CHUNK_SIZE;
    let base_y = chunk_pos.y * CHUNK_SIZE;
    let base_z = chunk_pos.z * CHUNK_SIZE;

    let column_tints = climate.map(build_column_tint_cache);

    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let Some(block) = neighbor_at(x, y, z) else {
                    continue;
                };
                if block.id == air {
                    continue;
                }
                let Some(def) = registry.block(block.id) else {
                    continue;
                };
                if !def.solid && !def.opaque && !def.transparent {
                    continue;
                }

                let wx = base_x + x;
                let wy = base_y + y;
                let wz = base_z + z;
                let origin = [wx as f32, wy as f32, wz as f32];
                let block_power = power
                    .map(|p| p.power_at(BlockPos::new(wx, wy, wz)))
                    .unwrap_or(0);

                let mut face_textures = registry
                    .block_face_textures_for_state(block.id, block.state)
                    .unwrap_or(def.face_textures);

                if def.fluid && fluid_flowing(block.state) {
                    if let Some(flow_tex) = registry.texture_by_name("stagcrest:water_flow") {
                        face_textures = fluid_flow_textures(face_textures, flow_tex);
                    }
                }

                let dust_connections = if def.namespaced_id == "stagcrest:redstone_dust" {
                    Some(compute_dust_connections(
                        |dx, dy, dz| {
                            let Some(neighbor) = neighbor_at(x + dx, y + dy, z + dz) else {
                                return false;
                            };
                            neighbor.id != air
                                && is_dust_connectable_neighbor(
                                    registry,
                                    neighbor.id,
                                    neighbor.state,
                                    -dx,
                                    -dz,
                                )
                        },
                        |dx, dy, dz| {
                            let Some(neighbor) = neighbor_at(x + dx, y + dy, z + dz) else {
                                return false;
                            };
                            if neighbor.id == air {
                                return false;
                            }
                            registry
                                .block(neighbor.id)
                                .is_some_and(|n| n.opaque && n.solid)
                        },
                    ))
                } else {
                    None
                };

                emit_block_geometry(
                    &mut mesh,
                    origin,
                    def.geometry,
                    &def.namespaced_id,
                    &face_textures,
                    mesh_bucket_for_layer(def.render_layer),
                    registry,
                    models,
                    block_power,
                    block.state,
                    x as i32,
                    z as i32,
                    climate,
                    column_tints.as_ref(),
                    |normal| {
                        should_cull_face(
                            def,
                            neighbor_at(
                                x + normal.x as i32,
                                y + normal.y as i32,
                                z + normal.z as i32,
                            ),
                            air,
                            registry,
                            normal,
                        )
                    },
                    dust_connections,
                );
            }
        }
    }

    mesh
}

fn emit_block_geometry(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    geometry: BlockGeometry,
    namespaced_id: &str,
    face_textures: &stagcrest_protocol::BlockFaceTextures,
    cube_bucket: MeshBucket,
    registry: &BlockRegistry,
    models: &ModelRegistry,
    power: u8,
    state: BlockState,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    mut should_cull: impl FnMut(Vec3) -> bool,
    dust_connections: Option<DustConnections>,
) {
    match geometry {
        BlockGeometry::Cube => emit_cube_faces(
            mesh,
            origin,
            face_textures,
            cube_bucket,
            registry,
            lx,
            lz,
            climate,
            column_tints,
            &mut should_cull,
        ),
        BlockGeometry::Flat => {
            if let Some(connections) = dust_connections {
                let textures = resolve_dust_textures(registry);
                emit_dust(
                    mesh,
                    origin,
                    connections,
                    textures,
                    power,
                    registry,
                    lx,
                    lz,
                    climate,
                    column_tints,
                );
            } else {
                emit_flat(
                    mesh,
                    origin,
                    face_textures.sides,
                    power,
                    registry,
                    lx,
                    lz,
                    climate,
                    column_tints,
                );
            }
        }
        BlockGeometry::Cross => {
            emit_cross_plants(
                mesh,
                origin,
                face_textures,
                power,
                registry,
                lx,
                lz,
                climate,
                column_tints,
            );
        }
        BlockGeometry::Model(model_id) => {
            let model = resolve_block_model(models, model_id, namespaced_id, state);
            // Circuit power is for dust tinting only; models use state-driven textures.
            emit_block_model(mesh, origin, model, face_textures, 0, registry);
        }
    }
}

fn emit_cube_faces(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    face_textures: &stagcrest_protocol::BlockFaceTextures,
    bucket: MeshBucket,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    mut should_cull: impl FnMut(Vec3) -> bool,
) {
    let faces: [(Vec3, FaceTexture); 6] = [
        (
            Vec3::new(0.0, -1.0, 0.0),
            face_texture_for(face_textures, -1.0),
        ),
        (
            Vec3::new(0.0, 1.0, 0.0),
            face_texture_for(face_textures, 1.0),
        ),
        (Vec3::new(0.0, 0.0, -1.0), face_textures.sides),
        (Vec3::new(0.0, 0.0, 1.0), face_textures.sides),
        (Vec3::new(-1.0, 0.0, 0.0), face_textures.sides),
        (Vec3::new(1.0, 0.0, 0.0), face_textures.sides),
    ];

    for (normal, face_tex) in faces {
        if should_cull(normal) {
            continue;
        }
        emit_face_from_texture(
            mesh,
            origin,
            normal,
            face_tex,
            0,
            bucket,
            registry,
            lx,
            lz,
            climate,
            column_tints,
        );
    }
}

const DUST_LAYER_EPS: f32 = 0.001;
const DUST_UP_UV: [f32; 4] = [3.0, 0.0, 13.0, 16.0];
const FULL_TILE_UV: [f32; 4] = [0.0, 0.0, 16.0, 16.0];

fn offset_corners(corners: [[f32; 3]; 4], axis: usize, delta: f32) -> [[f32; 3]; 4] {
    corners.map(|mut c| {
        c[axis] += delta;
        c
    })
}

fn texture_uv_corners(
    registry: &BlockRegistry,
    tex_id: TextureId,
    sub: [f32; 4],
) -> [(f32, f32); 4] {
    let uv_rect = registry.atlas_uv(tex_id);
    let (aw, ah) = registry.atlas_dimensions();
    let anim_meta = registry.texture_animation(tex_id);
    let frame_h = anim_meta
        .map(|anim| anim.frame_height.min(uv_rect.h))
        .or_else(|| {
            if uv_rect.h > uv_rect.w && uv_rect.w > 0 && uv_rect.h % uv_rect.w == 0 {
                Some(uv_rect.w)
            } else {
                None
            }
        })
        .unwrap_or(uv_rect.h);

    let x = uv_rect.x as f32;
    let y = uv_rect.y as f32;
    let w = uv_rect.w as f32;
    let h = frame_h as f32;

    let su0 = x + sub[0] / 16.0 * w;
    let sv0 = y + sub[1] / 16.0 * h;
    let su1 = x + sub[2] / 16.0 * w;
    let sv1 = y + sub[3] / 16.0 * h;

    // Inset to texel centers within the sub-rect so corners never bleed into atlas neighbors.
    let u0 = (su0 + 0.5).min(su1) / aw as f32;
    let u1 = (su1 - 0.5).max(su0) / aw as f32;
    let v0 = (sv0 + 0.5).min(sv1) / ah as f32;
    let v1 = (sv1 - 0.5).max(sv0) / ah as f32;
    [(u0, v1), (u1, v1), (u1, v0), (u0, v0)]
}

#[allow(clippy::too_many_arguments)]
fn emit_dust_quad_raw(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    tex_id: TextureId,
    uv_sub: [f32; 4],
    power: u8,
    power_tinted: bool,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let face_tex = FaceTexture {
        texture: tex_id,
        overlay: None,
        tint: if power_tinted {
            TintKind::PowerLevel
        } else {
            TintKind::None
        },
        overlay_tint: TintKind::None,
    };
    let uvs = texture_uv_corners(registry, tex_id, uv_sub);
    let tint = if power_tinted {
        dust_vertex_tint(power)
    } else {
        0.0
    };
    let tint_mul = face_tint_mul(&face_tex, lx, lz, climate, column_tints);
    let (verts, indices) = block_model::mesh_buffers(mesh, MeshBucket::Cutout);
    let base = verts.len() as u32;
    for (i, pos) in corners.iter().enumerate() {
        verts.push(VoxelVertex {
            position: *pos,
            uv: [uvs[i].0, uvs[i].1],
            overlay_uv: [0.0, 0.0],
            tint,
            overlay_tint: 0.0,
            tint_mul,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[allow(clippy::too_many_arguments)]
fn emit_dust_layer_pair(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    line_tex: TextureId,
    overlay_tex: Option<TextureId>,
    uv_sub: [f32; 4],
    normal_axis: usize,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    if let Some(overlay_tex) = overlay_tex {
        let overlay_rect = registry.atlas_uv(overlay_tex);
        if overlay_rect.w > 0 && overlay_rect.h > 0 {
            emit_dust_quad_raw(
                mesh,
                offset_corners(corners, normal_axis, -DUST_LAYER_EPS),
                overlay_tex,
                uv_sub,
                power,
                false,
                registry,
                lx,
                lz,
                climate,
                column_tints,
            );
        }
    }
    emit_dust_quad_raw(
        mesh,
        offset_corners(corners, normal_axis, DUST_LAYER_EPS),
        line_tex,
        uv_sub,
        power,
        true,
        registry,
        lx,
        lz,
        climate,
        column_tints,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_dust(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    connections: DustConnections,
    textures: DustTextures,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let o = origin;
    let line_y = o[1] + block_model::FLAT_Y + DUST_LAYER_EPS;
    let y_bot = o[1] + 1.0 / 16.0;

    if dust_shows_dot(connections) {
        let dot = [
            [o[0], line_y, o[2] + 1.0],
            [o[0] + 1.0, line_y, o[2] + 1.0],
            [o[0] + 1.0, line_y, o[2]],
            [o[0], line_y, o[2]],
        ];
        emit_dust_layer_pair(
            mesh,
            dot,
            textures.dot,
            textures.overlay,
            FULL_TILE_UV,
            1,
            power,
            registry,
            lx,
            lz,
            climate,
            column_tints,
        );
    }

    for (i, side) in connections.sides.iter().enumerate() {
        match side {
            DustSide::Side => {
                let (line_tex, corners) = match i {
                    0 => (
                        textures.line_ns,
                        [
                            [o[0], line_y, o[2] + 0.5],
                            [o[0] + 1.0, line_y, o[2] + 0.5],
                            [o[0] + 1.0, line_y, o[2]],
                            [o[0], line_y, o[2]],
                        ],
                    ),
                    1 => (
                        textures.line_ew,
                        [
                            [o[0] + 1.0, line_y, o[2] + 1.0],
                            [o[0] + 1.0, line_y, o[2]],
                            [o[0] + 0.5, line_y, o[2]],
                            [o[0] + 0.5, line_y, o[2] + 1.0],
                        ],
                    ),
                    2 => (
                        textures.line_ns,
                        [
                            [o[0], line_y, o[2] + 1.0],
                            [o[0] + 1.0, line_y, o[2] + 1.0],
                            [o[0] + 1.0, line_y, o[2] + 0.5],
                            [o[0], line_y, o[2] + 0.5],
                        ],
                    ),
                    _ => (
                        textures.line_ew,
                        [
                            [o[0], line_y, o[2]],
                            [o[0], line_y, o[2] + 1.0],
                            [o[0] + 0.5, line_y, o[2] + 1.0],
                            [o[0] + 0.5, line_y, o[2]],
                        ],
                    ),
                };
                emit_dust_layer_pair(
                    mesh,
                    corners,
                    line_tex,
                    textures.overlay,
                    FULL_TILE_UV,
                    1,
                    power,
                    registry,
                    lx,
                    lz,
                    climate,
                    column_tints,
                );
            }
            DustSide::Up => {
                let (line_tex, corners, axis) = match i {
                    0 => (
                        textures.line_ns,
                        [
                            [o[0] + 3.0 / 16.0, y_bot, o[2]],
                            [o[0] + 13.0 / 16.0, y_bot, o[2]],
                            [o[0] + 13.0 / 16.0, o[1] + 1.0, o[2]],
                            [o[0] + 3.0 / 16.0, o[1] + 1.0, o[2]],
                        ],
                        2,
                    ),
                    1 => (
                        textures.line_ew,
                        [
                            [o[0] + 1.0, y_bot, o[2] + 3.0 / 16.0],
                            [o[0] + 1.0, y_bot, o[2] + 13.0 / 16.0],
                            [o[0] + 1.0, o[1] + 1.0, o[2] + 13.0 / 16.0],
                            [o[0] + 1.0, o[1] + 1.0, o[2] + 3.0 / 16.0],
                        ],
                        0,
                    ),
                    2 => (
                        textures.line_ns,
                        [
                            [o[0] + 3.0 / 16.0, y_bot, o[2] + 1.0],
                            [o[0] + 13.0 / 16.0, y_bot, o[2] + 1.0],
                            [o[0] + 13.0 / 16.0, o[1] + 1.0, o[2] + 1.0],
                            [o[0] + 3.0 / 16.0, o[1] + 1.0, o[2] + 1.0],
                        ],
                        2,
                    ),
                    _ => (
                        textures.line_ew,
                        [
                            [o[0], y_bot, o[2] + 3.0 / 16.0],
                            [o[0], y_bot, o[2] + 13.0 / 16.0],
                            [o[0], o[1] + 1.0, o[2] + 13.0 / 16.0],
                            [o[0], o[1] + 1.0, o[2] + 3.0 / 16.0],
                        ],
                        0,
                    ),
                };
                emit_dust_layer_pair(
                    mesh,
                    corners,
                    line_tex,
                    textures.overlay,
                    DUST_UP_UV,
                    axis,
                    power,
                    registry,
                    lx,
                    lz,
                    climate,
                    column_tints,
                );
            }
            DustSide::None => {}
        }
    }
}

fn emit_flat(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    face_tex: FaceTexture,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let o = origin;
    let y = o[1] + block_model::FLAT_Y;
    let top = [
        [o[0], y, o[2] + 1.0],
        [o[0] + 1.0, y, o[2] + 1.0],
        [o[0] + 1.0, y, o[2]],
        [o[0], y, o[2]],
    ];
    emit_quad(
        mesh,
        top,
        face_tex,
        power,
        MeshBucket::Cutout,
        registry,
        lx,
        lz,
        climate,
        column_tints,
        false,
    );
}

fn emit_cross_plants(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    face_textures: &stagcrest_protocol::BlockFaceTextures,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let uniform = face_textures.sides;
    let layered = face_textures.top.texture != face_textures.bottom.texture;
    if layered {
        emit_cross_layer(
            mesh,
            origin,
            0.0,
            0.5,
            face_textures.bottom,
            power,
            registry,
            lx,
            lz,
            climate,
            column_tints,
        );
        emit_cross_layer(
            mesh,
            origin,
            0.5,
            1.0,
            face_textures.top,
            power,
            registry,
            lx,
            lz,
            climate,
            column_tints,
        );
    } else {
        emit_cross_layer(
            mesh,
            origin,
            0.0,
            1.0,
            uniform,
            power,
            registry,
            lx,
            lz,
            climate,
            column_tints,
        );
    }
}

fn emit_cross_layer(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    y0: f32,
    y1: f32,
    face_tex: FaceTexture,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let o = origin;
    let y_bottom = o[1] + y0;
    let y_top = o[1] + y1;
    let quad_x = [
        [o[0], y_bottom, o[2]],
        [o[0] + 1.0, y_bottom, o[2] + 1.0],
        [o[0] + 1.0, y_top, o[2] + 1.0],
        [o[0], y_top, o[2]],
    ];
    let quad_z = [
        [o[0], y_bottom, o[2] + 1.0],
        [o[0] + 1.0, y_bottom, o[2]],
        [o[0] + 1.0, y_top, o[2]],
        [o[0], y_top, o[2] + 1.0],
    ];
    for corners in [quad_x, quad_z] {
        emit_quad(
            mesh,
            corners,
            face_tex,
            power,
            MeshBucket::Cutout,
            registry,
            lx,
            lz,
            climate,
            column_tints,
            true,
        );
    }
}

const WHITE_TINT_MUL: [f32; 3] = [1.0, 1.0, 1.0];

fn face_tint_mul(
    face_tex: &FaceTexture,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) -> [f32; 3] {
    let kind = if matches!(face_tex.overlay_tint, TintKind::Grass | TintKind::Foliage) {
        face_tex.overlay_tint
    } else {
        face_tex.tint
    };
    if let Some(grid) = column_tints {
        let (grass, foliage) = grid[lz as usize][lx as usize];
        return match kind {
            TintKind::Grass => grass,
            TintKind::Foliage => foliage,
            _ => WHITE_TINT_MUL,
        };
    }
    tint_mul_for_kind(kind, climate)
}

fn tint_mul_for_kind(
    kind: TintKind,
    climate: Option<&MeshClimateTint<'_>>,
) -> [f32; 3] {
    let Some(ctx) = climate else {
        return WHITE_TINT_MUL;
    };
    let norm_temp = ctx.temperature.clamp(0.0, 1.0);
    let downfall = ctx.downfall.clamp(0.0, 1.0);
    match kind {
        TintKind::Grass => sample_colormap_rgb(
            &ctx.colormaps.grass,
            ctx.colormaps.grass_w,
            ctx.colormaps.grass_h,
            norm_temp,
            downfall,
        ),
        TintKind::Foliage => sample_colormap_rgb(
            &ctx.colormaps.foliage,
            ctx.colormaps.foliage_w,
            ctx.colormaps.foliage_h,
            norm_temp,
            downfall,
        ),
        _ => WHITE_TINT_MUL,
    }
}

pub(crate) fn vertex_tint(face_tex: FaceTexture, power: u8) -> f32 {
    if face_tex.tint == TintKind::PowerLevel {
        dust_vertex_tint(power)
    } else {
        face_tex.tint.as_f32()
    }
}

fn atlas_uv_bounds(
    registry: &BlockRegistry,
    tex_id: TextureId,
    uv_rect: stagcrest_protocol::AtlasRect,
) -> (f32, f32, f32, f32) {
    let (aw, ah) = registry.atlas_dimensions();
    let anim_meta = registry.texture_animation(tex_id);
    let frame_h = anim_meta
        .map(|anim| anim.frame_height.min(uv_rect.h))
        .or_else(|| {
            if uv_rect.h > uv_rect.w && uv_rect.w > 0 && uv_rect.h % uv_rect.w == 0 {
                Some(uv_rect.w)
            } else {
                None
            }
        })
        .unwrap_or(uv_rect.h);
    let bounds = {
        // Inset to texel centers so quad corners never sample the neighboring atlas column/row.
        let x = uv_rect.x as f32;
        let y = uv_rect.y as f32;
        let w = uv_rect.w as f32;
        let u0 = (x + 0.5) / aw as f32;
        let v0 = (y + 0.5) / ah as f32;
        let u1 = (x + w - 0.5) / aw as f32;
        let v1 = (y + frame_h as f32 - 0.5) / ah as f32;
        (u0, v0, u1.max(u0), v1.max(v0))
    };
    bounds
}

fn overlay_uv_bounds(
    registry: &BlockRegistry,
    tex_id: TextureId,
    uv_rect: stagcrest_protocol::AtlasRect,
) -> (f32, f32, f32, f32) {
    if uv_rect.w == 0 || uv_rect.h == 0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    atlas_uv_bounds(registry, tex_id, uv_rect)
}

fn emit_face_from_texture(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    normal: Vec3,
    face_tex: FaceTexture,
    power: u8,
    bucket: MeshBucket,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let uv_rect = registry.atlas_uv(face_tex.texture);
    let overlay_uv =
        face_tex
            .overlay
            .map(|id| registry.atlas_uv(id))
            .unwrap_or(stagcrest_protocol::AtlasRect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            });
    let tint_mul = face_tint_mul(&face_tex, lx, lz, climate, column_tints);
    emit_face(
        mesh,
        origin,
        normal,
        face_tex.texture,
        uv_rect,
        face_tex.overlay,
        overlay_uv,
        vertex_tint(face_tex, power),
        face_tex.overlay_tint.as_f32(),
        tint_mul,
        bucket,
        registry,
    );
}

fn emit_quad(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    face_tex: FaceTexture,
    power: u8,
    bucket: MeshBucket,
    registry: &BlockRegistry,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    double_sided: bool,
) {
    let uv_rect = registry.atlas_uv(face_tex.texture);
    let overlay_uv =
        face_tex
            .overlay
            .map(|id| registry.atlas_uv(id))
            .unwrap_or(stagcrest_protocol::AtlasRect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            });

    let (verts, indices) = block_model::mesh_buffers(mesh, bucket);

    let base = verts.len() as u32;
    let (u0, v0, u1, v1) = atlas_uv_bounds(registry, face_tex.texture, uv_rect);
    let (ou0, ov0, ou1, ov1) = face_tex
        .overlay
        .map(|id| overlay_uv_bounds(registry, id, overlay_uv))
        .unwrap_or((0.0, 0.0, 0.0, 0.0));

    let uvs = [(u0, v1), (u1, v1), (u1, v0), (u0, v0)];
    let overlay_uvs = [(ou0, ov1), (ou1, ov1), (ou1, ov0), (ou0, ov0)];
    let tint = vertex_tint(face_tex, power);
    let tint_mul = face_tint_mul(&face_tex, lx, lz, climate, column_tints);

    for (i, pos) in corners.iter().enumerate() {
        verts.push(VoxelVertex {
            position: *pos,
            uv: [uvs[i].0, uvs[i].1],
            overlay_uv: [overlay_uvs[i].0, overlay_uvs[i].1],
            tint,
            overlay_tint: face_tex.overlay_tint.as_f32(),
            tint_mul,
        });
    }

    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    if double_sided {
        indices.extend_from_slice(&[base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

fn emit_face(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    normal: glam::Vec3,
    tex_id: TextureId,
    uv: stagcrest_protocol::AtlasRect,
    overlay_tex: Option<TextureId>,
    overlay_uv_rect: stagcrest_protocol::AtlasRect,
    tint: f32,
    overlay_tint: f32,
    tint_mul: [f32; 3],
    bucket: MeshBucket,
    registry: &BlockRegistry,
) {
    let (verts, indices) = block_model::mesh_buffers(mesh, bucket);

    let base = verts.len() as u32;
    let (u0, v0, u1, v1) = atlas_uv_bounds(registry, tex_id, uv);
    let (ou0, ov0, ou1, ov1) = overlay_tex
        .map(|id| overlay_uv_bounds(registry, id, overlay_uv_rect))
        .unwrap_or((0.0, 0.0, 0.0, 0.0));

    let corners = face_corners(origin, normal);
    for (i, pos) in corners.iter().enumerate() {
        let (u, v) = match i {
            0 => (u0, v1),
            1 => (u1, v1),
            2 => (u1, v0),
            _ => (u0, v0),
        };
        let (ou, ov) = match i {
            0 => (ou0, ov1),
            1 => (ou1, ov1),
            2 => (ou1, ov0),
            _ => (ou0, ov0),
        };
        verts.push(VoxelVertex {
            position: *pos,
            uv: [u, v],
            overlay_uv: [ou, ov],
            tint,
            overlay_tint,
            tint_mul,
        });
    }

    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn face_corners(origin: [f32; 3], normal: glam::Vec3) -> [[f32; 3]; 4] {
    let o = origin;
    if normal.y > 0.0 {
        [
            [o[0], o[1] + 1.0, o[2] + 1.0],
            [o[0] + 1.0, o[1] + 1.0, o[2] + 1.0],
            [o[0] + 1.0, o[1] + 1.0, o[2]],
            [o[0], o[1] + 1.0, o[2]],
        ]
    } else if normal.y < 0.0 {
        [
            [o[0], o[1], o[2]],
            [o[0] + 1.0, o[1], o[2]],
            [o[0] + 1.0, o[1], o[2] + 1.0],
            [o[0], o[1], o[2] + 1.0],
        ]
    } else if normal.z > 0.0 {
        [
            [o[0], o[1], o[2] + 1.0],
            [o[0] + 1.0, o[1], o[2] + 1.0],
            [o[0] + 1.0, o[1] + 1.0, o[2] + 1.0],
            [o[0], o[1] + 1.0, o[2] + 1.0],
        ]
    } else if normal.z < 0.0 {
        [
            [o[0] + 1.0, o[1], o[2]],
            [o[0], o[1], o[2]],
            [o[0], o[1] + 1.0, o[2]],
            [o[0] + 1.0, o[1] + 1.0, o[2]],
        ]
    } else if normal.x > 0.0 {
        [
            [o[0] + 1.0, o[1], o[2] + 1.0],
            [o[0] + 1.0, o[1], o[2]],
            [o[0] + 1.0, o[1] + 1.0, o[2]],
            [o[0] + 1.0, o[1] + 1.0, o[2] + 1.0],
        ]
    } else {
        [
            [o[0], o[1], o[2]],
            [o[0], o[1], o[2] + 1.0],
            [o[0], o[1] + 1.0, o[2] + 1.0],
            [o[0], o[1] + 1.0, o[2]],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{BlockDef, ModelRenderLayer, TextureId};

    fn def(fluid: bool, opaque: bool, solid: bool) -> BlockDef {
        BlockDef {
            id: BlockId(0),
            namespaced_id: String::new(),
            display_name: String::new(),
            opaque,
            transparent: !opaque,
            solid,
            fluid,
            hardness: 1.0,
            face_textures: stagcrest_protocol::BlockFaceTextures::uniform(TextureId(0)),
            placeable: false,
            geometry: BlockGeometry::Cube,
            circuit: None,
            render_layer: if fluid {
                ModelRenderLayer::Blend
            } else if !opaque {
                ModelRenderLayer::Cutout
            } else {
                ModelRenderLayer::Opaque
            },
        }
    }

    #[test]
    fn fluid_fluid_faces_culled() {
        let water = def(true, false, false);
        assert!(neighbor_culls_face(
            &water,
            Some(&def(true, false, false)),
            Vec3::Y
        ));
    }

    #[test]
    fn fluid_air_faces_not_culled() {
        let water = def(true, false, false);
        assert!(!neighbor_culls_face(&water, None, Vec3::Y));
    }

    #[test]
    fn stone_stone_opaque_solid_culled() {
        let stone = def(false, true, true);
        assert!(neighbor_culls_face(
            &stone,
            Some(&def(false, true, true)),
            Vec3::Y
        ));
    }

    #[test]
    fn grass_top_not_culled_under_flat_decoration() {
        let grass = def(false, true, true);
        let mut plant = def(false, false, false);
        plant.geometry = BlockGeometry::Flat;
        plant.transparent = true;
        assert!(!neighbor_culls_face(&grass, Some(&plant), Vec3::Y));
    }

    #[test]
    fn grass_top_not_culled_under_cross_plant() {
        let grass = def(false, true, true);
        let mut plant = def(false, false, false);
        plant.geometry = BlockGeometry::Cross;
        plant.transparent = true;
        assert!(!neighbor_culls_face(&grass, Some(&plant), Vec3::Y));
    }

    #[test]
    fn mesh_bucket_matches_render_layer() {
        use stagcrest_protocol::ModelRenderLayer;
        assert!(matches!(
            mesh_bucket_for_layer(ModelRenderLayer::Cutout),
            MeshBucket::Cutout
        ));
        assert!(matches!(
            mesh_bucket_for_layer(ModelRenderLayer::Blend),
            MeshBucket::Blend
        ));
        assert!(matches!(
            mesh_bucket_for_layer(ModelRenderLayer::Opaque),
            MeshBucket::Opaque
        ));
    }
}
