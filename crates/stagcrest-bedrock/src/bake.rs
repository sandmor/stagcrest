//! Bake parsed Bedrock geometry into per-bone triangle meshes.
//!
//! Each bone becomes a rigid mesh expressed in *bone-local* space (relative to
//! the bone pivot, in block units where 16 model pixels = 1 block). The bone's
//! bind rotation and any animation are applied at the transform level by the
//! renderer, so cubes rotate about their bone pivot exactly as in Bedrock.

use crate::geometry::{Cube, CubeUv, FaceUv, FaceUvSet, Geometry};

/// Model pixels per world block.
pub const MODEL_PIXELS_PER_BLOCK: f32 = 16.0;

/// A single mesh vertex for an entity model.
#[derive(Debug, Clone, Copy)]
pub struct EntityVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct BakedBone {
    pub name: String,
    pub parent: Option<usize>,
    /// Bone pivot in block units, model space.
    pub pivot: [f32; 3],
    /// Bind rotation in degrees.
    pub bind_rotation: [f32; 3],
    pub vertices: Vec<EntityVertex>,
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct BakedModel {
    pub bones: Vec<BakedBone>,
    pub texture_width: u32,
    pub texture_height: u32,
}

// Cube corner layout and face windings mirror `stagcrest-mesh::model_bake`,
// which is already validated against this project's CCW front-face culling.
const FACE_CORNER_INDICES: [[usize; 4]; 6] = [
    [0, 1, 2, 3], // 0: down  (-Y)
    [7, 6, 5, 4], // 1: up    (+Y)
    [1, 0, 4, 5], // 2: north (-Z)
    [3, 2, 6, 7], // 3: south (+Z)
    [0, 3, 7, 4], // 4: west  (-X)
    [2, 1, 5, 6], // 5: east  (+X)
];

const FACE_NORMALS: [[f32; 3]; 6] = [
    [0.0, -1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, -1.0],
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
];

impl BakedModel {
    pub fn from_geometry(geo: &Geometry) -> BakedModel {
        let tex_w = geo.texture_width.max(1) as f32;
        let tex_h = geo.texture_height.max(1) as f32;

        let name_to_index: std::collections::HashMap<&str, usize> = geo
            .bones
            .iter()
            .enumerate()
            .map(|(i, b)| (b.name.as_str(), i))
            .collect();

        let mut bones = Vec::with_capacity(geo.bones.len());
        for bone in &geo.bones {
            let parent = bone
                .parent
                .as_deref()
                .and_then(|p| name_to_index.get(p).copied());
            let pivot = [
                bone.pivot[0] / MODEL_PIXELS_PER_BLOCK,
                bone.pivot[1] / MODEL_PIXELS_PER_BLOCK,
                bone.pivot[2] / MODEL_PIXELS_PER_BLOCK,
            ];

            let mut vertices = Vec::new();
            let mut indices = Vec::new();
            for cube in &bone.cubes {
                bake_cube(
                    &mut vertices,
                    &mut indices,
                    cube,
                    bone.pivot,
                    bone.mirror,
                    tex_w,
                    tex_h,
                );
            }

            bones.push(BakedBone {
                name: bone.name.clone(),
                parent,
                pivot,
                bind_rotation: bone.rotation,
                vertices,
                indices,
            });
        }

        BakedModel {
            bones,
            texture_width: geo.texture_width.max(1),
            texture_height: geo.texture_height.max(1),
        }
    }
}

/// Bake a single cube into `vertices`/`indices`, positions relative to the bone
/// pivot (in pixels initially, converted to blocks at emit).
fn bake_cube(
    vertices: &mut Vec<EntityVertex>,
    indices: &mut Vec<u32>,
    cube: &Cube,
    bone_pivot: [f32; 3],
    bone_mirror: bool,
    tex_w: f32,
    tex_h: f32,
) {
    let inflate = cube.inflate;
    let min = [
        cube.origin[0] - inflate,
        cube.origin[1] - inflate,
        cube.origin[2] - inflate,
    ];
    let max = [
        cube.origin[0] + cube.size[0] + inflate,
        cube.origin[1] + cube.size[1] + inflate,
        cube.origin[2] + cube.size[2] + inflate,
    ];

    let corners = cube_corners(min, max);
    // Optional per-cube rotation about its pivot, baked into geometry.
    let cube_pivot = cube.pivot.unwrap_or([0.0, 0.0, 0.0]);
    let rotated: [[f32; 3]; 8] = corners.map(|c| {
        let r = rotate_euler_about(c, cube.rotation, cube_pivot);
        // Convert to bone-local, block units.
        [
            (r[0] - bone_pivot[0]) / MODEL_PIXELS_PER_BLOCK,
            (r[1] - bone_pivot[1]) / MODEL_PIXELS_PER_BLOCK,
            (r[2] - bone_pivot[2]) / MODEL_PIXELS_PER_BLOCK,
        ]
    });

    let mirror = cube.mirror.unwrap_or(bone_mirror);

    for face in 0..6 {
        let rect = face_uv_rect(face, cube, tex_w, tex_h);
        let Some(rect) = rect else { continue };
        let idx = FACE_CORNER_INDICES[face];
        let normal = FACE_NORMALS[face];
        let base = vertices.len() as u32;
        for (slot, &ci) in idx.iter().enumerate() {
            let pos = rotated[ci];
            let (mut tu, tv) = corner_uv(face, slot);
            if mirror {
                tu = 1.0 - tu;
            }
            let u = (rect[0] + tu * rect[2]) / tex_w;
            let v = (rect[1] + tv * rect[3]) / tex_h;
            vertices.push(EntityVertex {
                position: pos,
                normal,
                uv: [u, v],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

fn cube_corners(from: [f32; 3], to: [f32; 3]) -> [[f32; 3]; 8] {
    [
        [from[0], from[1], from[2]],
        [to[0], from[1], from[2]],
        [to[0], from[1], to[2]],
        [from[0], from[1], to[2]],
        [from[0], to[1], from[2]],
        [to[0], to[1], from[2]],
        [to[0], to[1], to[2]],
        [from[0], to[1], to[2]],
    ]
}

/// Fractional (u, v) within a face for each of the 4 winding slots, oriented so
/// that texture-up (small v) is the higher-Y edge on side faces.
fn corner_uv(face: usize, slot: usize) -> (f32, f32) {
    // Corners per face in the winding order above. We map each to a UV corner.
    // Layout: slot -> (u, v) fractions.
    match face {
        // down (-Y): corners [p000, p100, p101, p001] over X/Z.
        0 => [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)][slot],
        // up (+Y): corners [p011, p111, p101, p001].
        1 => [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)][slot],
        // north (-Z): corners [p100, p000, p010, p110] -> X across, Y up.
        2 => [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)][slot],
        // south (+Z): corners [p001, p101, p111, p011].
        3 => [(1.0, 1.0), (0.0, 1.0), (0.0, 0.0), (1.0, 0.0)][slot],
        // west (-X): corners [p000, p001, p011, p010].
        4 => [(1.0, 1.0), (0.0, 1.0), (0.0, 0.0), (1.0, 0.0)][slot],
        // east (+X): corners [p101, p100, p110, p111].
        5 => [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)][slot],
        _ => (0.0, 0.0),
    }
}

/// Pixel-space UV rect `[x, y, w, h]` for a face, from box UV or per-face UV.
fn face_uv_rect(face: usize, cube: &Cube, _tex_w: f32, _tex_h: f32) -> Option<[f32; 4]> {
    match &cube.uv {
        CubeUv::Box { origin } => {
            let [u, v] = *origin;
            let w = cube.size[0];
            let h = cube.size[1];
            let d = cube.size[2];
            Some(match face {
                0 => [u + d + w, v, w, d],     // down
                1 => [u + d, v, w, d],         // up
                2 => [u + d, v + d, w, h],     // north (front)
                3 => [u + d + w + d, v + d, w, h], // south (back)
                4 => [u + d + w, v + d, d, h], // west
                5 => [u, v + d, d, h],         // east
                _ => return None,
            })
        }
        CubeUv::PerFace(set) => {
            let fu = pick_face_uv(set, face)?;
            let size = if fu.uv_size == [0.0, 0.0] {
                // Fall back to cube face size when uv_size missing.
                match face {
                    0 | 1 => [cube.size[0], cube.size[2]],
                    2 | 3 => [cube.size[0], cube.size[1]],
                    _ => [cube.size[2], cube.size[1]],
                }
            } else {
                fu.uv_size
            };
            Some([fu.uv[0], fu.uv[1], size[0], size[1]])
        }
    }
}

fn pick_face_uv(set: &FaceUvSet, face: usize) -> Option<FaceUv> {
    match face {
        0 => set.down,
        1 => set.up,
        2 => set.north,
        3 => set.south,
        4 => set.west,
        5 => set.east,
        _ => None,
    }
}

fn rotate_euler_about(p: [f32; 3], euler_deg: [f32; 3], pivot: [f32; 3]) -> [f32; 3] {
    if euler_deg == [0.0, 0.0, 0.0] {
        return p;
    }
    let mut rel = [p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]];
    // Bedrock applies cube rotation in Z, Y, X order about the pivot.
    if euler_deg[2] != 0.0 {
        rel = rotate_z(rel, euler_deg[2].to_radians());
    }
    if euler_deg[1] != 0.0 {
        rel = rotate_y(rel, euler_deg[1].to_radians());
    }
    if euler_deg[0] != 0.0 {
        rel = rotate_x(rel, euler_deg[0].to_radians());
    }
    [rel[0] + pivot[0], rel[1] + pivot[1], rel[2] + pivot[2]]
}

fn rotate_x(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}
fn rotate_y(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}
fn rotate_z(p: [f32; 3], a: f32) -> [f32; 3] {
    let (s, c) = a.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Geometry;

    const SAMPLE: &str = r#"
    { "minecraft:geometry": [ {
      "description": { "identifier": "geometry.t", "texture_width": 64, "texture_height": 64 },
      "bones": [
        { "name": "head", "pivot": [0, 24, 0], "cubes": [
          { "origin": [-4, 24, -4], "size": [8, 8, 8], "uv": [0, 0] }
        ]}
      ]
    }]}"#;

    #[test]
    fn bakes_cube_into_bone_mesh() {
        let geo = Geometry::from_json_bytes(SAMPLE.as_bytes()).unwrap();
        let model = BakedModel::from_geometry(&geo);
        assert_eq!(model.bones.len(), 1);
        let head = &model.bones[0];
        // 6 faces * 4 verts.
        assert_eq!(head.vertices.len(), 24);
        assert_eq!(head.indices.len(), 36);
        // Pivot in block units.
        assert!((head.pivot[1] - 1.5).abs() < 1e-4);
        // All UVs normalized within [0,1].
        for v in &head.vertices {
            assert!(v.uv[0] >= 0.0 && v.uv[0] <= 1.0, "u {}", v.uv[0]);
            assert!(v.uv[1] >= 0.0 && v.uv[1] <= 1.0, "v {}", v.uv[1]);
        }
    }

    #[test]
    fn box_uv_matches_classic_head_front() {
        let geo = Geometry::from_json_bytes(SAMPLE.as_bytes()).unwrap();
        let model = BakedModel::from_geometry(&geo);
        let head = &model.bones[0];
        // North (front) face box rect should start at pixel (8,8) for an 8-cube at [0,0].
        // First 4 verts belong to face 0 (down); front is face 2 -> verts 8..12.
        let front = &head.vertices[8..12];
        let min_u = front.iter().map(|v| v.uv[0]).fold(f32::MAX, f32::min) * 64.0;
        let min_v = front.iter().map(|v| v.uv[1]).fold(f32::MAX, f32::min) * 64.0;
        assert!((min_u - 8.0).abs() < 1e-3, "front min_u {min_u}");
        assert!((min_v - 8.0).abs() < 1e-3, "front min_v {min_v}");
    }
}
