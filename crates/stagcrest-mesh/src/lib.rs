use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use stagcrest_mod_host::{face_texture_for, BlockRegistry};
use stagcrest_protocol::{BlockId, ChunkPos, FaceTexture, CHUNK_SIZE};
use stagcrest_world::{Chunk, ChunkNeighborhood, World};
use std::collections::HashMap;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub overlay_uv: [f32; 2],
    pub tint: f32,
    pub overlay_tint: f32,
}

#[derive(Debug, Clone, Default)]
pub struct ChunkMesh {
    pub opaque_vertices: Vec<VoxelVertex>,
    pub opaque_indices: Vec<u32>,
    pub transparent_vertices: Vec<VoxelVertex>,
    pub transparent_indices: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct MeshCache {
    meshes: HashMap<ChunkPos, ChunkMesh>,
}

impl MeshCache {
    pub fn get(&self, pos: ChunkPos) -> Option<&ChunkMesh> {
        self.meshes.get(&pos)
    }

    pub fn rebuild_dirty(
        &mut self,
        world: &World,
        registry: &BlockRegistry,
        dirty: impl IntoIterator<Item = ChunkPos>,
    ) {
        let dirty: Vec<_> = dirty.into_iter().collect();
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            let built: Vec<_> = dirty
                .par_iter()
                .filter_map(|&pos| {
                    world
                        .chunk(pos)
                        .map(|chunk| (pos, build_chunk_mesh(pos, chunk, world, registry)))
                })
                .collect();
            for (pos, mesh) in built {
                self.meshes.insert(pos, mesh);
            }
        }
        #[cfg(not(feature = "parallel"))]
        {
            for pos in dirty {
                if let Some(chunk) = world.chunk(pos) {
                    let mesh = build_chunk_mesh(pos, chunk, world, registry);
                    self.meshes.insert(pos, mesh);
                }
            }
        }
    }

    pub fn meshes(&self) -> &HashMap<ChunkPos, ChunkMesh> {
        &self.meshes
    }

    pub fn remove(&mut self, pos: ChunkPos) {
        self.meshes.remove(&pos);
    }
}

fn build_chunk_mesh(
    chunk_pos: ChunkPos,
    chunk: &Chunk,
    world: &World,
    registry: &BlockRegistry,
) -> ChunkMesh {
    let mut mesh = ChunkMesh::default();
    let base_x = chunk_pos.x * CHUNK_SIZE;
    let base_y = chunk_pos.y * CHUNK_SIZE;
    let base_z = chunk_pos.z * CHUNK_SIZE;

    let mut neighbors = HashMap::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                if dx == 0 && dy == 0 && dz == 0 {
                    continue;
                }
                let npos = ChunkPos {
                    x: chunk_pos.x + dx,
                    y: chunk_pos.y + dy,
                    z: chunk_pos.z + dz,
                };
                if let Some(c) = world.chunk(npos) {
                    neighbors.insert((dx, dy, dz), c);
                }
            }
        }
    }

    let hood = ChunkNeighborhood {
        center: chunk,
        neighbors,
    };

    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let local = stagcrest_protocol::LocalBlockPos {
                    x: x as u8,
                    y: y as u8,
                    z: z as u8,
                };
                let block = chunk.get(local);
                if block.id == world.air() {
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

                let face_textures = registry
                    .block_face_textures_for_state(block.id, block.state)
                    .unwrap_or(def.face_textures);

                let faces: [(Vec3, FaceTexture); 6] = [
                    (
                        Vec3::new(0.0, -1.0, 0.0),
                        face_texture_for(&face_textures, -1.0),
                    ),
                    (Vec3::new(0.0, 1.0, 0.0), face_texture_for(&face_textures, 1.0)),
                    (Vec3::new(0.0, 0.0, -1.0), face_textures.sides),
                    (Vec3::new(0.0, 0.0, 1.0), face_textures.sides),
                    (Vec3::new(-1.0, 0.0, 0.0), face_textures.sides),
                    (Vec3::new(1.0, 0.0, 0.0), face_textures.sides),
                ];

                for (normal, face_tex) in faces {
                    let neighbor = hood.get(
                        x + normal.x as i32,
                        y + normal.y as i32,
                        z + normal.z as i32,
                    );
                    let neighbor_def = registry.block(neighbor.id);
                    let cull = neighbor_def.map(|d| d.opaque && d.solid).unwrap_or(false);
                    if cull {
                        continue;
                    }

                    let _ao = compute_ao(normal, x, y, z, &hood, registry, world.air());
                    let uv_rect = registry.atlas_uv(face_tex.texture);
                    let overlay_uv = face_tex
                        .overlay
                        .map(|id| registry.atlas_uv(id))
                        .unwrap_or(stagcrest_protocol::AtlasRect {
                            x: 0,
                            y: 0,
                            w: 0,
                            h: 0,
                        });

                    emit_face(
                        &mut mesh,
                        [wx as f32, wy as f32, wz as f32],
                        normal,
                        uv_rect,
                        overlay_uv,
                        face_tex.tint.as_f32(),
                        face_tex.overlay_tint.as_f32(),
                        def.transparent,
                        registry,
                    );
                }
            }
        }
    }

    mesh
}

fn compute_ao(
    normal: glam::Vec3,
    x: i32,
    y: i32,
    z: i32,
    hood: &ChunkNeighborhood,
    registry: &BlockRegistry,
    air: BlockId,
) -> f32 {
    let mut solid = 0;
    for &(dx, dy, dz) in &[
        (1, 0, 0),
        (-1, 0, 0),
        (0, 1, 0),
        (0, -1, 0),
        (0, 0, 1),
        (0, 0, -1),
    ] {
        let b = hood.get(x + dx, y + dy, z + dz);
        if b.id != air {
            if let Some(d) = registry.block(b.id) {
                if d.solid {
                    solid += 1;
                }
            }
        }
    }
    let _ = normal;
    1.0 - (solid as f32 * 0.12).min(0.75)
}

fn atlas_uv_to_normalized(
    uv: stagcrest_protocol::AtlasRect,
    registry: &BlockRegistry,
) -> [f32; 2] {
    let (aw, ah) = registry.atlas_dimensions();
    [
        uv.x as f32 / aw as f32,
        uv.y as f32 / ah as f32,
    ]
}

fn emit_face(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    normal: glam::Vec3,
    uv: stagcrest_protocol::AtlasRect,
    overlay_uv_rect: stagcrest_protocol::AtlasRect,
    tint: f32,
    overlay_tint: f32,
    transparent: bool,
    registry: &BlockRegistry,
) {
    let (verts, indices) = if transparent {
        (
            &mut mesh.transparent_vertices,
            &mut mesh.transparent_indices,
        )
    } else {
        (&mut mesh.opaque_vertices, &mut mesh.opaque_indices)
    };

    let base = verts.len() as u32;
    let (aw, ah) = registry.atlas_dimensions();
    let u0 = uv.x as f32 / aw as f32;
    let v0 = uv.y as f32 / ah as f32;
    let u1 = (uv.x + uv.w) as f32 / aw as f32;
    let v1 = (uv.y + uv.h) as f32 / ah as f32;

    let (ou0, ov0) = {
        let n = atlas_uv_to_normalized(overlay_uv_rect, registry);
        (n[0], n[1])
    };
    let ou1 = (overlay_uv_rect.x + overlay_uv_rect.w) as f32 / aw as f32;
    let ov1 = (overlay_uv_rect.y + overlay_uv_rect.h) as f32 / ah as f32;

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
