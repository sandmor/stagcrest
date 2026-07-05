//! Bake block models into local-space mesh geometry for GPU upload.

use stagcrest_mod_client::BlockRegistry;
use stagcrest_protocol::{
    AtlasRect, BlockFaceTextures, BlockModel, FaceTexture, ModelElement, ModelFace, ModelRotation,
    ModelTexture,
};

use crate::{encode_normal_axis, pack_light, vertex_ao};

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuMeshVertex {
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

#[derive(Debug, Clone, Default)]
pub struct BakedMesh {
    pub vertices: Vec<GpuMeshVertex>,
    pub indices: Vec<u32>,
}

const CENTER: [f32; 3] = [0.5, 0.5, 0.5];

const FACE_CORNER_INDICES: [[usize; 4]; 6] = [
    [0, 1, 2, 3],
    [7, 6, 5, 4],
    [1, 0, 4, 5],
    [3, 2, 6, 7],
    [0, 3, 7, 4],
    [2, 1, 5, 6],
];

pub fn bake_unit_quad() -> BakedMesh {
    let mut mesh = BakedMesh::default();
    let corners = [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    push_quad(&mut mesh, &corners, FULL_UV, FULL_UV, 0.0, 0.0);
    mesh
}

pub fn bake_cross_plant() -> BakedMesh {
    let mut mesh = BakedMesh::default();
    // Two diagonal planes that cross at the cell center forming an X. Each plane
    // is emitted on both sides so it stays visible with back-face culling on.
    let plane_a = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 0.0],
    ];
    let plane_b = [
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
    ];
    for plane in [plane_a, plane_b] {
        push_quad(&mut mesh, &plane, FULL_UV, ZERO_UV, 0.0, 0.0);
        let back = [plane[3], plane[2], plane[1], plane[0]];
        let back_uv = [FULL_UV[3], FULL_UV[2], FULL_UV[1], FULL_UV[0]];
        push_quad(&mut mesh, &back, back_uv, ZERO_UV, 0.0, 0.0);
    }
    mesh
}

pub fn bake_wire_quad() -> BakedMesh {
    bake_unit_quad()
}

pub fn bake_block_model(
    model: &BlockModel,
    face_textures: &BlockFaceTextures,
    registry: &BlockRegistry,
) -> BakedMesh {
    let mut mesh = BakedMesh::default();
    for element in &model.elements {
        let face_tex = match element.texture {
            ModelTexture::Top => face_textures.top,
            ModelTexture::Bottom => face_textures.bottom,
            ModelTexture::Sides => face_textures.sides,
        };
        bake_element(&mut mesh, element, model.rotation, face_tex, registry);
    }
    mesh
}

fn bake_element(
    mesh: &mut BakedMesh,
    element: &ModelElement,
    model_rotation: [f32; 3],
    face_tex: FaceTexture,
    registry: &BlockRegistry,
) {
    let transformed = element_corners_block_local(element, model_rotation);
    let atlas_uv = registry.atlas_uv(face_tex.texture);
    let overlay_atlas = face_tex
        .overlay
        .map(|id| registry.atlas_uv(id))
        .unwrap_or(AtlasRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            atlas_index: 0,
        });
    let (aw, ah) = registry.atlas_dimensions(atlas_uv.atlas_index);
    let (oaw, oah) = registry.atlas_dimensions(overlay_atlas.atlas_index);
    let tint = face_tex.tint.as_f32();
    let overlay_tint = face_tex.overlay_tint.as_f32();

    for face in 0..6 {
        let Some(face_def) = element.faces[face] else {
            continue;
        };
        let indices = FACE_CORNER_INDICES[face];
        let corners = [
            transformed[indices[0]],
            transformed[indices[1]],
            transformed[indices[2]],
            transformed[indices[3]],
        ];
        let uv_pixels = face_uv_corners(face_def);
        push_model_face(
            mesh,
            &corners,
            &uv_pixels,
            atlas_uv,
            overlay_atlas,
            aw,
            ah,
            oaw,
            oah,
            tint,
            overlay_tint,
            face,
        );
    }
}

const FACE_NORMALS: [[f32; 3]; 6] = [
    [0.0, -1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, -1.0],
    [0.0, 0.0, 1.0],
    [-1.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
];

const WHITE_TINT_MUL: [f32; 3] = [1.0, 1.0, 1.0];

fn quad_normal(corners: &[[f32; 3]; 4]) -> [f32; 3] {
    let ax = corners[1][0] - corners[0][0];
    let ay = corners[1][1] - corners[0][1];
    let az = corners[1][2] - corners[0][2];
    let bx = corners[3][0] - corners[0][0];
    let by = corners[3][1] - corners[0][1];
    let bz = corners[3][2] - corners[0][2];
    let nx = ay * bz - az * by;
    let ny = az * bx - ax * bz;
    let nz = ax * by - ay * bx;
    let len = (nx * nx + ny * ny + nz * nz).sqrt();
    if len > 0.0 {
        [nx / len, ny / len, nz / len]
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn default_vertex_shade(normal: [f32; 3], _corner: u8) -> (u8, u8, u8, u8) {
    (
        encode_normal_axis(glam::Vec3::from_array(normal)),
        pack_light(15, 0),
        vertex_ao(3),
        0,
    )
}

const FULL_UV: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
const ZERO_UV: [[f32; 2]; 4] = [[0.0; 2]; 4];

fn push_quad(
    mesh: &mut BakedMesh,
    corners: &[[f32; 3]; 4],
    uvs: [[f32; 2]; 4],
    overlay_uvs: [[f32; 2]; 4],
    tint: f32,
    overlay_tint: f32,
) {
    let normal = quad_normal(corners);
    let base = mesh.vertices.len() as u32;
    for (i, pos) in corners.iter().enumerate() {
        let (normal_enc, light_enc, ao, flags) = default_vertex_shade(normal, i as u8);
        mesh.vertices.push(GpuMeshVertex {
            position: *pos,
            uv: uvs[i],
            overlay_uv: overlay_uvs[i],
            tint,
            overlay_tint,
            tint_mul: WHITE_TINT_MUL,
            atlas_index: 0.0,
            overlay_atlas_index: 0.0,
            normal: normal_enc,
            light: light_enc,
            ao,
            flags,
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn push_model_face(
    mesh: &mut BakedMesh,
    corners: &[[f32; 3]; 4],
    uv_pixels: &[[f32; 2]; 4],
    atlas_uv: AtlasRect,
    overlay_atlas: AtlasRect,
    aw: u32,
    ah: u32,
    oaw: u32,
    oah: u32,
    tint: f32,
    overlay_tint: f32,
    face: usize,
) {
    let normal = FACE_NORMALS[face];
    let mut uvs = [[0.0f32; 2]; 4];
    let mut overlay_uvs = [[0.0f32; 2]; 4];
    for (i, uv) in uv_pixels.iter().enumerate() {
        uvs[i] = [
            (atlas_uv.x as f32 + uv[0] / 16.0 * atlas_uv.w as f32) / aw as f32,
            (atlas_uv.y as f32 + uv[1] / 16.0 * atlas_uv.h as f32) / ah as f32,
        ];
        overlay_uvs[i] = [
            (overlay_atlas.x as f32 + uv[0] / 16.0 * overlay_atlas.w as f32) / oaw as f32,
            (overlay_atlas.y as f32 + uv[1] / 16.0 * overlay_atlas.h as f32) / oah as f32,
        ];
    }
    let base = mesh.vertices.len() as u32;
    for (i, pos) in corners.iter().enumerate() {
        let (normal_enc, light_enc, ao, flags) = default_vertex_shade(normal, i as u8);
        mesh.vertices.push(GpuMeshVertex {
            position: *pos,
            uv: uvs[i],
            overlay_uv: overlay_uvs[i],
            tint,
            overlay_tint,
            tint_mul: WHITE_TINT_MUL,
            atlas_index: atlas_uv.atlas_index as f32,
            overlay_atlas_index: overlay_atlas.atlas_index as f32,
            normal: normal_enc,
            light: light_enc,
            ao,
            flags,
        });
    }
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn element_corners_block_local(element: &ModelElement, model_rotation: [f32; 3]) -> [[f32; 3]; 8] {
    box_corners(element.from, element.to).map(|mut p| {
        if let Some(rot) = element.rotation {
            p = rotate_local(p, rot);
        }
        rotate_model_about(p, model_rotation, CENTER)
    })
}

fn box_corners(from: [f32; 3], to: [f32; 3]) -> [[f32; 3]; 8] {
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

fn rotate_local(p: [f32; 3], rot: ModelRotation) -> [f32; 3] {
    use stagcrest_protocol::ModelAxis;
    let rel = [
        p[0] - rot.origin[0],
        p[1] - rot.origin[1],
        p[2] - rot.origin[2],
    ];
    let angle = rot.angle.to_radians();
    let mut out = match rot.axis {
        ModelAxis::X => rotate_x(rel, angle),
        ModelAxis::Y => rotate_y(rel, angle),
        ModelAxis::Z => rotate_z(rel, angle),
    };
    if rot.rescale {
        let (s, c) = angle.sin_cos();
        let scale = 1.0 / c.abs().max(s.abs());
        out = match rot.axis {
            ModelAxis::X => [out[0], out[1] * scale, out[2] * scale],
            ModelAxis::Y => [out[0] * scale, out[1], out[2] * scale],
            ModelAxis::Z => [out[0] * scale, out[1] * scale, out[2]],
        };
    }
    [
        out[0] + rot.origin[0],
        out[1] + rot.origin[1],
        out[2] + rot.origin[2],
    ]
}

fn rotate_model_about(p: [f32; 3], euler_deg: [f32; 3], pivot: [f32; 3]) -> [f32; 3] {
    if euler_deg == [0.0, 0.0, 0.0] {
        return p;
    }
    let mut rel = [p[0] - pivot[0], p[1] - pivot[1], p[2] - pivot[2]];
    if euler_deg[0] != 0.0 {
        rel = rotate_x(rel, euler_deg[0].to_radians());
    }
    if euler_deg[1] != 0.0 {
        rel = rotate_y(rel, euler_deg[1].to_radians());
    }
    if euler_deg[2] != 0.0 {
        rel = rotate_z(rel, euler_deg[2].to_radians());
    }
    [rel[0] + pivot[0], rel[1] + pivot[1], rel[2] + pivot[2]]
}

fn rotate_x(p: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

fn rotate_y(p: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

fn rotate_z(p: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c, p[2]]
}

fn face_uv_corners(face_def: ModelFace) -> [[f32; 2]; 4] {
    let [u0, v0, u1, v1] = face_def.uv;
    [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
}
