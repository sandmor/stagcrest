use stagcrest_mod_host::BlockRegistry;
use stagcrest_protocol::{
    AtlasRect, BlockFaceTextures, BlockModel, FaceTexture, ModelAxis, ModelElement, ModelFace,
    ModelRenderLayer, ModelRotation, ModelTexture, TintKind,
};

use crate::{vertex_tint, ChunkMesh, VoxelVertex};

/// Block-space center of a voxel cell, used as the pivot for the whole-model
/// orientation rotation.
const CENTER: [f32; 3] = [0.5, 0.5, 0.5];

pub fn emit_block_model(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    model: &BlockModel,
    face_textures: &BlockFaceTextures,
    power: u8,
    registry: &BlockRegistry,
) {
    let bucket = layer_bucket(model.layer);
    for element in &model.elements {
        let face_tex = select_face_texture(face_textures, element.texture);
        emit_element(
            mesh,
            origin,
            element,
            model.rotation,
            face_tex,
            power,
            bucket,
            registry,
        );
    }
}

fn select_face_texture(textures: &BlockFaceTextures, slot: ModelTexture) -> FaceTexture {
    match slot {
        ModelTexture::Top => textures.top,
        ModelTexture::Bottom => textures.bottom,
        ModelTexture::Sides => textures.sides,
    }
}

fn layer_bucket(layer: ModelRenderLayer) -> MeshBucket {
    match layer {
        ModelRenderLayer::Opaque => MeshBucket::Opaque,
        ModelRenderLayer::Blend => MeshBucket::Blend,
        ModelRenderLayer::Cutout => MeshBucket::Cutout,
    }
}

#[derive(Clone, Copy)]
pub enum MeshBucket {
    Opaque,
    Blend,
    Cutout,
}

pub fn mesh_buffers<'a>(
    mesh: &'a mut ChunkMesh,
    bucket: MeshBucket,
) -> (&'a mut Vec<VoxelVertex>, &'a mut Vec<u32>) {
    match bucket {
        MeshBucket::Opaque => (&mut mesh.opaque_vertices, &mut mesh.opaque_indices),
        MeshBucket::Blend => (
            &mut mesh.transparent_vertices,
            &mut mesh.transparent_indices,
        ),
        MeshBucket::Cutout => (&mut mesh.cutout_vertices, &mut mesh.cutout_indices),
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_element(
    mesh: &mut ChunkMesh,
    origin: [f32; 3],
    element: &ModelElement,
    model_rotation: [f32; 3],
    face_tex: FaceTexture,
    power: u8,
    bucket: MeshBucket,
    registry: &BlockRegistry,
) {
    let from = element.from;
    let to = element.to;
    let corners = box_corners(from, to);

    // Transform each corner entirely in block-local space (element rotation,
    // then the whole-model orientation), then translate by the world origin.
    let transformed: [[f32; 3]; 8] = corners.map(|c| {
        let mut p = c;
        if let Some(rot) = element.rotation {
            p = rotate_local(p, rot);
        }
        p = rotate_model_about(p, model_rotation, CENTER);
        [origin[0] + p[0], origin[1] + p[1], origin[2] + p[2]]
    });

    let atlas_uv = registry.atlas_uv(face_tex.texture);
    let overlay_atlas = face_tex
        .overlay
        .map(|id| registry.atlas_uv(id))
        .unwrap_or(AtlasRect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        });
    let (aw, ah) = registry.atlas_dimensions();
    // Model faces never use dust power tint; `TintKind::PowerLevel` at power 0
    // still encodes as 3.0 and triggers a red multiply in the shader.
    let tint = if face_tex.tint == TintKind::PowerLevel {
        vertex_tint(face_tex, power)
    } else {
        face_tex.tint.as_f32()
    };
    let overlay_tint = face_tex.overlay_tint.as_f32();

    for face in 0..6 {
        let Some(face_def) = element.faces[face] else {
            continue;
        };
        let indices = FACE_CORNER_INDICES[face];
        let face_corners = [
            transformed[indices[0]],
            transformed[indices[1]],
            transformed[indices[2]],
            transformed[indices[3]],
        ];
        let uv_corners = face_uv_corners(face_def);
        emit_model_face(
            mesh,
            face_corners,
            uv_corners,
            atlas_uv,
            overlay_atlas,
            aw,
            ah,
            tint,
            overlay_tint,
            bucket,
        );
    }
}

/// Corner indices per face (`BoxFace` order: Down, Up, North, South, West,
/// East). The winding matches the working axis-aligned cube faces so model
/// faces share the same outward orientation and survive back-face culling.
const FACE_CORNER_INDICES: [[usize; 4]; 6] = [
    [0, 1, 2, 3], // Down  (-Y)
    [7, 6, 5, 4], // Up    (+Y)
    [1, 0, 4, 5], // North (-Z)
    [3, 2, 6, 7], // South (+Z)
    [0, 3, 7, 4], // West  (-X)
    [2, 1, 5, 6], // East  (+X)
];

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

/// Rotate a block-local point about an element rotation (pivot in block-local
/// coordinates).
fn rotate_local(p: [f32; 3], rot: ModelRotation) -> [f32; 3] {
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
        out = rescale_relative(out, rot.axis, angle);
    }
    [
        out[0] + rot.origin[0],
        out[1] + rot.origin[1],
        out[2] + rot.origin[2],
    ]
}

/// Apply a whole-model orientation (Euler degrees in X, then Y, then Z order)
/// about `pivot`.
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

fn rescale_relative(p: [f32; 3], axis: ModelAxis, angle: f32) -> [f32; 3] {
    let (s, c) = angle.sin_cos();
    let scale = 1.0 / c.abs().max(s.abs());
    match axis {
        ModelAxis::X => [p[0], p[1] * scale, p[2] * scale],
        ModelAxis::Y => [p[0] * scale, p[1], p[2] * scale],
        ModelAxis::Z => [p[0] * scale, p[1] * scale, p[2]],
    }
}

/// Map a face's UV rectangle (in 0–16 pixel space) onto the four corners in
/// `FACE_CORNER_INDICES` order. The corner order is chosen to match the cube
/// face winding, where corner 0 is the texture's bottom-left, so the same
/// fixed assignment keeps every face upright. Returned values stay in pixel
/// space and are scaled into the atlas in `emit_model_face`.
fn face_uv_corners(face_def: ModelFace) -> [[f32; 2]; 4] {
    let [u0, v0, u1, v1] = face_def.uv;
    [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
}

fn emit_model_face(
    mesh: &mut ChunkMesh,
    corners: [[f32; 3]; 4],
    uv_pixels: [[f32; 2]; 4],
    atlas_uv: AtlasRect,
    overlay_atlas: AtlasRect,
    aw: u32,
    ah: u32,
    tint: f32,
    overlay_tint: f32,
    bucket: MeshBucket,
) {
    let (verts, indices) = mesh_buffers(mesh, bucket);
    let base = verts.len() as u32;

    for (i, pos) in corners.iter().enumerate() {
        let u = (atlas_uv.x as f32 + uv_pixels[i][0] / 16.0 * atlas_uv.w as f32) / aw as f32;
        let v = (atlas_uv.y as f32 + uv_pixels[i][1] / 16.0 * atlas_uv.h as f32) / ah as f32;
        let ou =
            (overlay_atlas.x as f32 + uv_pixels[i][0] / 16.0 * overlay_atlas.w as f32) / aw as f32;
        let ov =
            (overlay_atlas.y as f32 + uv_pixels[i][1] / 16.0 * overlay_atlas.h as f32) / ah as f32;
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
