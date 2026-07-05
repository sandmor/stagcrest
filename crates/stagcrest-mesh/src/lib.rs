mod block_model;
mod chunk_build;
mod greedy_mesh;
mod light;
mod mesh_snapshot;
mod model_bake;

use std::collections::{HashMap, HashSet};

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use stagcrest_mod_client::{
    face_texture_for, resolve_block_model, resolve_wire_line_textures, sample_colormap_rgb,
    wire_power_vertex_tint, wire_shows_center_junction, BlockRegistry, ColormapSet, ModelRegistry,
    WireConnections, WireLineTextures, WireLink,
};
use stagcrest_protocol::{
    BlockGeometry, BlockId, BlockState, ChunkPos, FaceTexture, TextureId, TintKind, CHUNK_SIZE,
};
use stagcrest_world::ChunkBlock;

pub use block_model::{
    block_selection_bounds, emit_block_model, mesh_bucket_for_layer, MeshBucket, SelectionBounds,
};
pub use chunk_build::build_chunk_mesh_snapshot;
pub use light::{
    encode_normal_axis, pack_light, shade_vertex, vertex_ao, ChunkLightGrid, LightBuildContext,
    LightSampler, LightingContext, GRID_SIZE,
};
pub use greedy_mesh::greedy_mesh_enabled;
pub use mesh_snapshot::{capture_power_grid, MeshClimateSnapshot, MeshSnapshot};
pub use model_bake::{
    bake_block_model, bake_cross_plant, bake_unit_quad, bake_wire_quad, BakedMesh, GpuMeshVertex,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub overlay_uv: [f32; 2],
    pub tint: f32,
    pub overlay_tint: f32,
    pub tint_mul: [f32; 3],
    pub atlas_index: f32,
    pub overlay_atlas_index: f32,
    pub normal: u8,
    pub light: u8,
    pub ao: u8,
    pub flags: u8,
}

pub const VERTEX_FLAG_EMISSIVE: u8 = 1;
pub const VERTEX_FLAG_FACES_FLUID: u8 = 2;

#[derive(Debug, Clone, Default)]
pub struct ChunkMesh {
    pub opaque_vertices: Vec<VoxelVertex>,
    pub opaque_indices: Vec<u32>,
    pub transparent_vertices: Vec<VoxelVertex>,
    pub transparent_indices: Vec<u32>,
    pub cutout_vertices: Vec<VoxelVertex>,
    pub cutout_indices: Vec<u32>,
}

pub struct MeshCache {
    meshes: HashMap<ChunkPos, ChunkMesh>,
    /// Chunks whose CPU meshes changed and need a Bevy mesh upload.
    dirty: HashSet<ChunkPos>,
}

impl Default for MeshCache {
    fn default() -> Self {
        Self {
            meshes: HashMap::new(),
            dirty: HashSet::new(),
        }
    }
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

    /// Drop cached CPU meshes and return their chunk positions for rebuild.
    pub fn clear_meshes(&mut self) -> Vec<ChunkPos> {
        let keys: Vec<_> = self.meshes.keys().copied().collect();
        self.meshes.clear();
        self.dirty.clear();
        keys
    }

    pub fn take_dirty(&mut self) -> HashSet<ChunkPos> {
        std::mem::take(&mut self.dirty)
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.meshes.remove(&pos);
        self.dirty.insert(pos);
    }
}

#[derive(Clone)]
pub struct MeshClimateTint<'a> {
    pub colormaps: &'a ColormapSet,
    pub temperature: f32,
    pub downfall: f32,
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
        let textures = resolve_wire_line_textures(registry);
        emit_wire_line(
            &mut mesh,
            [0.0, 0.0, 0.0],
            WireConnections::icon_cross(),
            textures,
            power,
            registry,
            0,
            0,
            0,
            None,
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
        0,
        None,
        None,
        |_| false,
        None,
        None,
    );
    mesh
}

pub(crate) fn should_cull_face(
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

pub(crate) fn neighbor_culls_face(
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
pub(crate) type ColumnTintCache =
    [[([f32; 3], [f32; 3]); CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

pub(crate) fn build_column_tint_cache(climate: &MeshClimateTint<'_>) -> ColumnTintCache {
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

pub(crate) fn emit_block_geometry(
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
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    mut should_cull: impl FnMut(Vec3) -> bool,
    wire_connections: Option<WireConnections>,
    light: Option<&LightingContext<'_>>,
) {
    match geometry {
        BlockGeometry::Cube => emit_cube_faces(
            mesh,
            origin,
            face_textures,
            cube_bucket,
            registry,
            lx,
            ly,
            lz,
            climate,
            column_tints,
            &mut should_cull,
            light,
        ),
        BlockGeometry::Flat => {
            if let Some(connections) = wire_connections {
                let textures = resolve_wire_line_textures(registry);
                emit_wire_line(
                    mesh,
                    origin,
                    connections,
                    textures,
                    power,
                    registry,
                    lx,
                    ly,
                    lz,
                    climate,
                    column_tints,
                    light,
                );
            } else {
                emit_flat(
                    mesh,
                    origin,
                    face_textures.sides,
                    power,
                    registry,
                    lx,
                    ly,
                    lz,
                    climate,
                    column_tints,
                    light,
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
                ly,
                lz,
                climate,
                column_tints,
                light,
            );
        }
        BlockGeometry::Model(model_id) => {
            let model = resolve_block_model(models, model_id, namespaced_id, state);
            emit_block_model(
                mesh,
                origin,
                model,
                face_textures,
                power,
                registry,
                lx,
                ly,
                lz,
                climate,
                column_tints,
                light,
            );
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
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    mut should_cull: impl FnMut(Vec3) -> bool,
    light: Option<&LightingContext<'_>>,
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
            ly,
            lz,
            climate,
            column_tints,
            light,
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
) -> ([(f32, f32); 4], u8) {
    let uv_rect = registry.atlas_uv(tex_id);
    let (aw, ah) = registry.atlas_dimensions(uv_rect.atlas_index);
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
    (
        [(u0, v1), (u1, v1), (u1, v0), (u0, v0)],
        uv_rect.atlas_index,
    )
}

fn quad_normal(corners: [[f32; 3]; 4]) -> Vec3 {
    let a = Vec3::from_array(corners[1]) - Vec3::from_array(corners[0]);
    let b = Vec3::from_array(corners[3]) - Vec3::from_array(corners[0]);
    a.cross(b).normalize_or_zero()
}

#[allow(clippy::too_many_arguments)]
fn emit_wire_quad_raw(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    tex_id: TextureId,
    uv_sub: [f32; 4],
    power: u8,
    power_tinted: bool,
    registry: &BlockRegistry,
    lx: i32,
    _ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
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
    let (uvs, atlas_index) = texture_uv_corners(registry, tex_id, uv_sub);
    let tint = if power_tinted {
        wire_power_vertex_tint(power)
    } else {
        0.0
    };
    let tint_mul = face_tint_mul(&face_tex, lx, lz, climate, column_tints);
    let normal = quad_normal(corners);
    let (verts, indices) = block_model::mesh_buffers(mesh, MeshBucket::Cutout);
    let base = verts.len() as u32;
    for (i, pos) in corners.iter().enumerate() {
        let (normal_enc, light_enc, ao, flags) = shade_vertex(light, normal, i as u8);
        verts.push(VoxelVertex {
            position: *pos,
            uv: [uvs[i].0, uvs[i].1],
            overlay_uv: [0.0, 0.0],
            tint,
            overlay_tint: 0.0,
            tint_mul,
            atlas_index: atlas_index as f32,
            overlay_atlas_index: 0.0,
            normal: normal_enc,
            light: light_enc,
            ao,
            flags,
        });
    }
    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

#[allow(clippy::too_many_arguments)]
fn emit_wire_layer_pair(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    line_tex: TextureId,
    overlay_tex: Option<TextureId>,
    uv_sub: [f32; 4],
    normal_axis: usize,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
) {
    if let Some(overlay_tex) = overlay_tex {
        let overlay_rect = registry.atlas_uv(overlay_tex);
        if overlay_rect.w > 0 && overlay_rect.h > 0 {
            emit_wire_quad_raw(
                mesh,
                offset_corners(corners, normal_axis, -DUST_LAYER_EPS),
                overlay_tex,
                uv_sub,
                power,
                false,
                registry,
                lx,
                ly,
                lz,
                climate,
                column_tints,
                light,
            );
        }
    }
    emit_wire_quad_raw(
        mesh,
        offset_corners(corners, normal_axis, DUST_LAYER_EPS),
        line_tex,
        uv_sub,
        power,
        true,
        registry,
        lx,
        ly,
        lz,
        climate,
        column_tints,
        light,
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_wire_line(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    connections: WireConnections,
    textures: WireLineTextures,
    power: u8,
    registry: &BlockRegistry,
    lx: i32,
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
) {
    let o = origin;
    let line_y = o[1] + block_model::FLAT_Y + DUST_LAYER_EPS;
    let y_bot = o[1] + 1.0 / 16.0;

    if wire_shows_center_junction(connections) {
        let dot = [
            [o[0], line_y, o[2] + 1.0],
            [o[0] + 1.0, line_y, o[2] + 1.0],
            [o[0] + 1.0, line_y, o[2]],
            [o[0], line_y, o[2]],
        ];
        emit_wire_layer_pair(
            mesh,
            dot,
            textures.dot,
            textures.overlay,
            FULL_TILE_UV,
            1,
            power,
            registry,
            lx,
            ly,
            lz,
            climate,
            column_tints,
            light,
        );
    }

    for (i, side) in connections.sides.iter().enumerate() {
        match side {
            WireLink::Side => {
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
                emit_wire_layer_pair(
                    mesh,
                    corners,
                    line_tex,
                    textures.overlay,
                    FULL_TILE_UV,
                    1,
                    power,
                    registry,
                    lx,
                    ly,
                    lz,
                    climate,
                    column_tints,
                    light,
                );
            }
            WireLink::Up => {
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
                emit_wire_layer_pair(
                    mesh,
                    corners,
                    line_tex,
                    textures.overlay,
                    DUST_UP_UV,
                    axis,
                    power,
                    registry,
                    lx,
                    ly,
                    lz,
                    climate,
                    column_tints,
                    light,
                );
            }
            WireLink::None => {}
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
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
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
        ly,
        lz,
        climate,
        column_tints,
        light,
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
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
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
            ly,
            lz,
            climate,
            column_tints,
            light,
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
            ly,
            lz,
            climate,
            column_tints,
            light,
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
            ly,
            lz,
            climate,
            column_tints,
            light,
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
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
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
            ly,
            lz,
            climate,
            column_tints,
            light,
            true,
        );
    }
}

const WHITE_TINT_MUL: [f32; 3] = [1.0, 1.0, 1.0];

pub(crate) fn face_tint_mul(
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

fn tint_mul_for_kind(kind: TintKind, climate: Option<&MeshClimateTint<'_>>) -> [f32; 3] {
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
        wire_power_vertex_tint(power)
    } else {
        face_tex.tint.as_f32()
    }
}

pub(crate) fn atlas_frame_height(
    registry: &BlockRegistry,
    tex_id: TextureId,
    uv_rect: stagcrest_protocol::AtlasRect,
) -> u32 {
    let anim_meta = registry.texture_animation(tex_id);
    anim_meta
        .map(|anim| anim.frame_height.min(uv_rect.h))
        .or_else(|| {
            if uv_rect.h > uv_rect.w && uv_rect.w > 0 && uv_rect.h % uv_rect.w == 0 {
                Some(uv_rect.w)
            } else {
                None
            }
        })
        .unwrap_or(uv_rect.h)
        .max(1)
}

pub(crate) fn atlas_uv_bounds(
    registry: &BlockRegistry,
    tex_id: TextureId,
    uv_rect: stagcrest_protocol::AtlasRect,
) -> (f32, f32, f32, f32) {
    let (aw, ah) = registry.atlas_dimensions(uv_rect.atlas_index);
    let frame_h = atlas_frame_height(registry, tex_id, uv_rect);
    let x = uv_rect.x as f32;
    let y = uv_rect.y as f32;
    let w = uv_rect.w as f32;
    let u0 = (x + 0.5) / aw as f32;
    let v0 = (y + 0.5) / ah as f32;
    let u1 = (x + w - 0.5) / aw as f32;
    let v1 = (y + frame_h as f32 - 0.5) / ah as f32;
    (u0, v0, u1.max(u0), v1.max(v0))
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
    _ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
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
                atlas_index: 0,
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
        light,
    );
}

fn quad_block_tiles(corners: [[f32; 3]; 4]) -> (i32, i32) {
    fn edge_blocks(a: [f32; 3], b: [f32; 3]) -> i32 {
        ((a[0] - b[0])
            .abs()
            .max((a[1] - b[1]).abs())
            .max((a[2] - b[2]).abs())
            .round() as i32)
            .max(1)
    }
    (
        edge_blocks(corners[0], corners[1]),
        edge_blocks(corners[0], corners[3]),
    )
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Bilinear point on a quad (0=BL, 1=BR, 2=TR, 3=TL).
fn bilinear_corner(corners: [[f32; 3]; 4], u: f32, v: f32) -> [f32; 3] {
    let bottom = lerp3(corners[0], corners[1], u);
    let top = lerp3(corners[3], corners[2], u);
    lerp3(bottom, top, v)
}

fn sub_quad_corners(
    corners: [[f32; 3]; 4],
    tile_u: i32,
    tile_v: i32,
    i: i32,
    j: i32,
) -> [[f32; 3]; 4] {
    let tu = tile_u as f32;
    let tv = tile_v as f32;
    let u0 = i as f32 / tu;
    let u1 = (i + 1) as f32 / tu;
    let v0 = j as f32 / tv;
    let v1 = (j + 1) as f32 / tv;
    [
        bilinear_corner(corners, u0, v0),
        bilinear_corner(corners, u1, v0),
        bilinear_corner(corners, u1, v1),
        bilinear_corner(corners, u0, v1),
    ]
}

pub(crate) fn emit_quad(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    face_tex: FaceTexture,
    power: u8,
    bucket: MeshBucket,
    registry: &BlockRegistry,
    lx: i32,
    ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
    double_sided: bool,
) {
    let (tile_u, tile_v) = quad_block_tiles(corners);
    if tile_u > 1 || tile_v > 1 {
        for j in 0..tile_v {
            for i in 0..tile_u {
                let sub = sub_quad_corners(corners, tile_u, tile_v, i, j);
                emit_quad(
                    mesh,
                    sub,
                    face_tex,
                    power,
                    bucket,
                    registry,
                    lx,
                    ly,
                    lz,
                    climate,
                    column_tints,
                    light,
                    double_sided,
                );
            }
        }
        return;
    }

    emit_quad_single(
        mesh,
        corners,
        face_tex,
        power,
        bucket,
        registry,
        lx,
        ly,
        lz,
        climate,
        column_tints,
        light,
        double_sided,
    );
}

fn emit_quad_single(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    face_tex: FaceTexture,
    power: u8,
    bucket: MeshBucket,
    registry: &BlockRegistry,
    lx: i32,
    _ly: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
    light: Option<&LightingContext<'_>>,
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
                atlas_index: 0,
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
    let normal = quad_normal(corners);

    for (i, pos) in corners.iter().enumerate() {
        let (normal_enc, light_enc, ao, flags) = shade_vertex(light, normal, i as u8);
        verts.push(VoxelVertex {
            position: *pos,
            uv: [uvs[i].0, uvs[i].1],
            overlay_uv: [overlay_uvs[i].0, overlay_uvs[i].1],
            tint,
            overlay_tint: face_tex.overlay_tint.as_f32(),
            tint_mul,
            atlas_index: uv_rect.atlas_index as f32,
            overlay_atlas_index: overlay_uv.atlas_index as f32,
            normal: normal_enc,
            light: light_enc,
            ao,
            flags,
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
    light: Option<&LightingContext<'_>>,
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
        let (normal_enc, light_enc, ao, flags) = shade_vertex(light, normal, i as u8);
        verts.push(VoxelVertex {
            position: *pos,
            uv: [u, v],
            overlay_uv: [ou, ov],
            tint,
            overlay_tint,
            tint_mul,
            atlas_index: uv.atlas_index as f32,
            overlay_atlas_index: overlay_uv_rect.atlas_index as f32,
            normal: normal_enc,
            light: light_enc,
            ao,
            flags,
        });
    }

    indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

pub(crate) fn face_corners(origin: [f32; 3], normal: glam::Vec3) -> [[f32; 3]; 4] {
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
            push_reaction: stagcrest_protocol::PushReaction::Normal,
            map_color: [128, 128, 128],
            redstone_powerable: stagcrest_protocol::default_redstone_powerable(
                solid,
                opaque,
                fluid,
                false,
            ),
            light_emission: 0,
            light_emission_when_lit: false,
            light_attenuation: 0,
            blocks_sky_light: None,
        }
    }

    #[test]
    fn neighbor_face_culling() {
        let stone = def(false, true, true);
        let water = def(true, false, false);
        let mut plant = def(false, false, false);
        plant.geometry = BlockGeometry::Cross;
        plant.transparent = true;

        assert!(neighbor_culls_face(&water, Some(&water), Vec3::Y));
        assert!(!neighbor_culls_face(&water, None, Vec3::Y));
        assert!(neighbor_culls_face(&stone, Some(&stone), Vec3::Y));
        assert!(!neighbor_culls_face(&stone, Some(&plant), Vec3::Y));
    }

    #[test]
    fn packed_texture_uvs_fit_atlas_pages() {
        use image::{ImageBuffer, Rgba};
        use stagcrest_atlas::{build_atlas_set, texture_def_from_png, AtlasLimits};
        use stagcrest_mod_client::BlockRegistry;

        fn solid_png(w: u32, h: u32) -> Vec<u8> {
            let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
                ImageBuffer::from_pixel(w, h, Rgba([40, 80, 120, 255]));
            let mut png = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            png
        }

        let png = solid_png(64, 64);
        let stone =
            texture_def_from_png(TextureId(1), "stagcrest:stone".into(), &png, None).unwrap();
        let water_png = solid_png(64, 2048);
        let water = texture_def_from_png(
            TextureId(2),
            "stagcrest:water_still".into(),
            &water_png,
            None,
        )
        .unwrap();
        let set = build_atlas_set(
            [stone.clone(), water],
            AtlasLimits {
                max_dimension: 8192,
                max_atlases: 8,
            },
        )
        .unwrap();

        let mut registry = BlockRegistry::new();
        registry.register_texture_with_animation(
            stone.namespaced_id.clone(),
            stone.width,
            stone.height,
            stone.rgba,
            stone.animation,
        );
        registry.apply_atlas_set(&set);

        for tex in registry.textures() {
            let rect = registry.atlas_uv(tex.id);
            let (aw, ah) = registry.atlas_dimensions(rect.atlas_index);
            assert!(
                rect.x + rect.w <= aw,
                "texture {:?} exceeds page width",
                tex.namespaced_id
            );
            assert!(
                rect.y + rect.h <= ah,
                "texture {:?} exceeds page height",
                tex.namespaced_id
            );
            let corners = texture_uv_corners(&registry, tex.id, FULL_TILE_UV);
            let (uvs, page_idx) = corners;
            assert_eq!(page_idx, rect.atlas_index);
            for (u, v) in uvs {
                assert!((0.0..=1.0).contains(&u));
                assert!((0.0..=1.0).contains(&v));
            }
        }
    }
}
