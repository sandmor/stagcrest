use glam::Vec3;
use stagcrest_mod_client::{face_texture_for, BlockRegistry};
use stagcrest_protocol::{
    BlockFaceTextures, BlockGeometry, BlockId, ChunkPos, FaceTexture, CHUNK_SIZE,
};
use stagcrest_world::ChunkBlock;

use crate::{
    emit_quad, face_corners, face_tint_mul, should_cull_face, vertex_tint, ChunkMesh,
    ColumnTintCache, MeshBucket, MeshClimateTint,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct MergeKey {
    block_id: u32,
    block_state: u32,
    face_tex_hash: u64,
    tint: u32,
    overlay_tint: u32,
    tint_mul: [u32; 3],
    bucket: u8,
}

fn face_tex_key(tex: &FaceTexture) -> u64 {
    let mut h = tex.texture.0 as u64;
    h = h.wrapping_mul(31).wrapping_add(tex.overlay.map(|t| t.0 as u64).unwrap_or(0));
    h = h.wrapping_mul(31).wrapping_add(tex.tint as u32 as u64);
    h = h.wrapping_mul(31).wrapping_add(tex.overlay_tint as u32 as u64);
    h
}

fn f32_bits(v: f32) -> u32 {
    v.to_bits()
}

fn merge_key(
    block: ChunkBlock,
    face_tex: FaceTexture,
    bucket: MeshBucket,
    lx: i32,
    lz: i32,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) -> MergeKey {
    let tint_mul = face_tint_mul(&face_tex, lx, lz, climate, column_tints);
    MergeKey {
        block_id: block.id.0,
        block_state: block.state.0 as u32,
        face_tex_hash: face_tex_key(&face_tex),
        tint: f32_bits(vertex_tint(face_tex, 0)),
        overlay_tint: f32_bits(face_tex.overlay_tint.as_f32()),
        tint_mul: [
            f32_bits(tint_mul[0]),
            f32_bits(tint_mul[1]),
            f32_bits(tint_mul[2]),
        ],
        bucket: match bucket {
            MeshBucket::Opaque => 0,
            MeshBucket::Blend => 1,
            MeshBucket::Cutout => 2,
        },
    }
}

struct CubeCell {
    block: ChunkBlock,
    face_textures: BlockFaceTextures,
    bucket: MeshBucket,
}

pub(crate) struct GreedyGrid {
    cells: [[[Option<CubeCell>; CHUNK_SIZE as usize]; CHUNK_SIZE as usize]; CHUNK_SIZE as usize],
}

impl GreedyGrid {
    pub fn new() -> Self {
        Self {
            cells: std::array::from_fn(|_| {
                std::array::from_fn(|_| std::array::from_fn(|_| None))
            }),
        }
    }

    pub fn insert(
        &mut self,
        x: usize,
        y: usize,
        z: usize,
        block: ChunkBlock,
        face_textures: BlockFaceTextures,
        bucket: MeshBucket,
    ) {
        self.cells[x][y][z] = Some(CubeCell {
            block,
            face_textures,
            bucket,
        });
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Option<&CubeCell> {
        if !(0..CHUNK_SIZE).contains(&x) || !(0..CHUNK_SIZE).contains(&y) || !(0..CHUNK_SIZE).contains(&z)
        {
            return None;
        }
        self.cells[x as usize][y as usize][z as usize].as_ref()
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_greedy_cubes(
    mesh: &mut ChunkMesh,
    grid: &GreedyGrid,
    chunk_pos: ChunkPos,
    air: BlockId,
    registry: &BlockRegistry,
    neighbor_at: &impl Fn(i32, i32, i32) -> Option<ChunkBlock>,
    climate: Option<&MeshClimateTint<'_>>,
    column_tints: Option<&ColumnTintCache>,
) {
    let base_x = chunk_pos.x * CHUNK_SIZE;
    let base_y = chunk_pos.y * CHUNK_SIZE;
    let base_z = chunk_pos.z * CHUNK_SIZE;

    for axis in 0..3usize {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        let dims = [CHUNK_SIZE, CHUNK_SIZE, CHUNK_SIZE];

        for direction in 0..2u8 {
            for slice in 0..CHUNK_SIZE {
                let mut mask: Vec<Option<MergeKey>> =
                    vec![None; (dims[u] * dims[v]) as usize];

                for j in 0..dims[u] {
                    for k in 0..dims[v] {
                        let mut pos = [0i32; 3];
                        pos[axis] = slice;
                        pos[u] = j;
                        pos[v] = k;

                        let (cx, cy, cz) = (pos[0], pos[1], pos[2]);
                        let Some(cell) = grid.get(cx, cy, cz) else {
                            continue;
                        };
                        let Some(def) = registry.block(cell.block.id) else {
                            continue;
                        };

                        let normal = {
                            let mut n = [0.0f32; 3];
                            n[axis] = if direction == 0 { -1.0 } else { 1.0 };
                            Vec3::from_array(n)
                        };

                        let neighbor_pos = (
                            cx + normal.x as i32,
                            cy + normal.y as i32,
                            cz + normal.z as i32,
                        );
                        if should_cull_face(
                            def,
                            neighbor_at(
                                neighbor_pos.0,
                                neighbor_pos.1,
                                neighbor_pos.2,
                            ),
                            air,
                            registry,
                            normal,
                        ) {
                            continue;
                        }

                        let face_tex = if normal.y.abs() > 0.5 {
                            face_texture_for(&cell.face_textures, normal.y)
                        } else {
                            cell.face_textures.sides
                        };

                        let key = merge_key(
                            cell.block,
                            face_tex,
                            cell.bucket,
                            cx,
                            cz,
                            climate,
                            column_tints,
                        );
                        mask[(j * dims[v] + k) as usize] = Some(key);
                    }
                }

                let mut n = 0usize;
                while n < mask.len() {
                    let Some(key) = mask[n] else {
                        n += 1;
                        continue;
                    };
                    let j = (n as i32) / dims[v];
                    let k = (n as i32) % dims[v];

                    let mut width = 1i32;
                    while (k + width) < dims[v]
                        && mask[n + width as usize] == Some(key)
                    {
                        width += 1;
                    }

                    let mut height = 1i32;
                    'grow: while (j + height) < dims[u] {
                        for w in 0..width {
                            let idx = ((j + height) * dims[v] + k + w) as usize;
                            if mask[idx] != Some(key) {
                                break 'grow;
                            }
                        }
                        height += 1;
                    }

                    for dj in 0..height {
                        for dk in 0..width {
                            let idx = ((j + dj) * dims[v] + k + dk) as usize;
                            mask[idx] = None;
                        }
                    }

                    let mut block_pos = [0i32; 3];
                    block_pos[axis] = slice;
                    block_pos[u] = j;
                    block_pos[v] = k;
                    let (bx, by, bz) = (block_pos[0], block_pos[1], block_pos[2]);
                    let Some(cell) = grid.get(bx, by, bz) else {
                        n += 1;
                        continue;
                    };

                    let face_tex = {
                        let mut n_vec = [0.0f32; 3];
                        n_vec[axis] = if direction == 0 { -1.0 } else { 1.0 };
                        let normal = Vec3::from_array(n_vec);
                        if normal.y.abs() > 0.5 {
                            face_texture_for(&cell.face_textures, normal.y)
                        } else {
                            cell.face_textures.sides
                        }
                    };

                    let corners = merged_face_corners(
                        base_x,
                        base_y,
                        base_z,
                        axis,
                        direction,
                        u,
                        v,
                        [bx, by, bz],
                        width,
                        height,
                    );

                    emit_quad(
                        mesh,
                        corners,
                        face_tex,
                        0,
                        cell.bucket,
                        registry,
                        bx,
                        bz,
                        climate,
                        column_tints,
                        false,
                    );

                    n += 1;
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn merged_face_corners(
    base_x: i32,
    base_y: i32,
    base_z: i32,
    axis: usize,
    direction: u8,
    u: usize,
    v: usize,
    block_origin: [i32; 3],
    width: i32,
    height: i32,
) -> [[f32; 3]; 4] {
    let block_lo = [
        base_x as f32 + block_origin[0] as f32,
        base_y as f32 + block_origin[1] as f32,
        base_z as f32 + block_origin[2] as f32,
    ];
    let lo = block_lo;
    let mut hi = [lo[0] + 1.0, lo[1] + 1.0, lo[2] + 1.0];
    hi[u] = lo[u] + height as f32;
    hi[v] = lo[v] + width as f32;

    let mut normal = [0.0f32; 3];
    normal[axis] = if direction == 0 { -1.0 } else { 1.0 };
    let plane = if direction == 0 { lo[axis] } else { hi[axis] };

    // Reuse per-face corner order so greedy quads share winding and UV layout.
    let unit = face_corners(block_lo, Vec3::from_array(normal));
    let mut corners = [[0.0f32; 3]; 4];
    for (i, corner) in corners.iter_mut().enumerate() {
        for a in 0..3 {
            if a == axis {
                corner[a] = plane;
            } else {
                let is_min = (unit[i][a] - block_lo[a]).abs() < 0.001;
                corner[a] = if is_min { lo[a] } else { hi[a] };
            }
        }
    }
    corners
}

pub(crate) fn is_greedy_eligible(
    geometry: BlockGeometry,
    fluid: bool,
) -> bool {
    geometry == BlockGeometry::Cube && !fluid && greedy_mesh_enabled()
}

pub fn greedy_mesh_enabled() -> bool {
    !matches!(
        std::env::var("STAGCREST_GREEDY_MESH").as_deref(),
        Ok("0") | Ok("false") | Ok("FALSE")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::face_corners;

    fn outward_normal(corners: [[f32; 3]; 4]) -> Vec3 {
        let a = Vec3::from_array(corners[1]) - Vec3::from_array(corners[0]);
        let b = Vec3::from_array(corners[2]) - Vec3::from_array(corners[0]);
        a.cross(b)
    }

    #[test]
    fn merged_corners_match_per_face_winding() {
        for axis in 0..3usize {
            let u = (axis + 1) % 3;
            let v = (axis + 2) % 3;
            for direction in 0..2u8 {
                let mut expected_normal = [0.0f32; 3];
                expected_normal[axis] = if direction == 0 { -1.0 } else { 1.0 };
                let expected_normal = Vec3::from_array(expected_normal);

                let block_lo = [4.0, 8.0, 12.0];
                let merged = merged_face_corners(
                    0, 0, 0, axis, direction, u, v, [4, 8, 12], 1, 1,
                );
                let per_face = face_corners(block_lo, expected_normal);

                for i in 0..4 {
                    for a in 0..3 {
                        assert!(
                            (merged[i][a] - per_face[i][a]).abs() < 1e-4,
                            "axis={axis} dir={direction} corner={i} axis={a}: merged={:?} per_face={:?}",
                            merged[i],
                            per_face[i]
                        );
                    }
                }

                let merged_n = outward_normal(merged).normalize();
                assert!(
                    merged_n.dot(expected_normal) > 0.99,
                    "axis={axis} dir={direction}: merged normal {merged_n:?} expected {expected_normal:?}"
                );
            }
        }
    }

    #[test]
    fn merged_quad_splits_into_per_block_uvs() {
        use crate::{emit_quad, ChunkMesh, MeshBucket};
        use stagcrest_mod_client::BlockRegistry;
        use stagcrest_protocol::{FaceTexture, TintKind};

        let mut reg = BlockRegistry::new();
        reg.set_atlas_dimensions(256, 256);
        let tex = reg.register_texture("t".into(), 16, 16, vec![0; 16 * 16 * 4]);
        reg.set_atlas_uv(tex, stagcrest_protocol::AtlasRect { x: 0, y: 0, w: 16, h: 16 });
        let face_tex = FaceTexture {
            texture: tex,
            overlay: None,
            tint: TintKind::None,
            overlay_tint: TintKind::None,
        };
        let merged = merged_face_corners(0, 0, 0, 2, 1, 0, 1, [0, 0, 0], 2, 3);

        let mut mesh = ChunkMesh::default();
        emit_quad(
            &mut mesh,
            merged,
            face_tex,
            0,
            MeshBucket::Opaque,
            &reg,
            0,
            0,
            None,
            None,
            false,
        );

        let v = &mesh.opaque_vertices;
        assert_eq!(v.len(), 24, "2×3 merged face → six 1×1 sub-quads");
        let tile_u = v[1].uv[0] - v[0].uv[0];
        let tile_v = v[0].uv[1] - v[3].uv[1];
        assert!(tile_u > 0.0 && tile_v > 0.0);
    }
}
