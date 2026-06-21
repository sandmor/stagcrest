use std::collections::HashMap;

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use stagcrest_mesh::{build_single_block_icon_mesh, ChunkMesh};
use stagcrest_protocol::{
    decode_power_tint, BlockGeometry, BlockId, BlockState, TINT_POWER_BASE,
    torch_state, TorchAttachment,
};

use crate::game::ModContext;
use stagcrest_render::BlockAtlasResource;

pub const ICON_SIZE: u32 = 64;

#[derive(Resource)]
pub struct BlockIconCache {
    pub icons: HashMap<BlockId, Handle<Image>>,
    default: Handle<Image>,
}

impl BlockIconCache {
    pub fn get(&self, id: BlockId) -> Handle<Image> {
        self.icons
            .get(&id)
            .cloned()
            .unwrap_or_else(|| self.default.clone())
    }
}

#[derive(Clone, Copy)]
enum IconProjection {
    Isometric,
    TopDown,
}

#[derive(Clone, Copy)]
struct IconBakeProfile {
    projection: IconProjection,
    yaw: f32,
    margin: f32,
    alpha_cutoff: u8,
}

fn profile_for_geometry(geometry: BlockGeometry) -> IconBakeProfile {
    match geometry {
        BlockGeometry::Cube => IconBakeProfile {
            projection: IconProjection::Isometric,
            yaw: 0.0,
            margin: 8.0,
            alpha_cutoff: 8,
        },
        BlockGeometry::Model(_) => IconBakeProfile {
            projection: IconProjection::Isometric,
            yaw: 0.0,
            margin: 10.0,
            alpha_cutoff: 4,
        },
        BlockGeometry::Flat => IconBakeProfile {
            projection: IconProjection::TopDown,
            yaw: 0.0,
            margin: 8.0,
            alpha_cutoff: 2,
        },
    }
}

pub fn bake_block_icons(
    mod_ctx: &ModContext,
    atlas_res: &BlockAtlasResource,
    images: &mut Assets<Image>,
) -> BlockIconCache {
    let grass = [
        atlas_res.grass_tint.to_srgba().red,
        atlas_res.grass_tint.to_srgba().green,
        atlas_res.grass_tint.to_srgba().blue,
    ];
    let foliage = [
        atlas_res.foliage_tint.to_srgba().red,
        atlas_res.foliage_tint.to_srgba().green,
        atlas_res.foliage_tint.to_srgba().blue,
    ];
    let power_dark = [0.4f32, 0.0, 0.0];
    let power_bright = [1.0f32, 0.0, 0.0];

    let mut icons = HashMap::new();
    for &block_id in mod_ctx.registry.placeable_blocks() {
        let geometry = mod_ctx
            .registry
            .block(block_id)
            .map(|d| d.geometry)
            .unwrap_or_default();
        let profile = profile_for_geometry(geometry);

        let power = if mod_ctx
            .registry
            .block(block_id)
            .is_some_and(|d| d.namespaced_id == "stagcrest:redstone_dust")
        {
            15
        } else {
            0
        };
        let icon_state = if mod_ctx
            .registry
            .block(block_id)
            .is_some_and(|d| d.namespaced_id == "stagcrest:redstone_torch")
        {
            torch_state(true, TorchAttachment::Floor)
        } else {
            BlockState(0)
        };
        let mesh = build_single_block_icon_mesh(
            &mod_ctx.registry,
            &mod_ctx.models,
            block_id,
            icon_state,
            power,
        );

        let rgba = rasterize_block_icon(
            &mesh,
            profile,
            &atlas_res.atlas.pixels,
            atlas_res.atlas.width,
            atlas_res.atlas.height,
            grass,
            foliage,
            power_dark,
            power_bright,
        );
        let handle = images.add(image_from_rgba(rgba));
        icons.insert(block_id, handle);
    }

    let default = images.add(image_from_rgba(placeholder_icon_rgba()));

    BlockIconCache { icons, default }
}

fn image_from_rgba(rgba: Vec<u8>) -> Image {
    Image::new(
        Extent3d {
            width: ICON_SIZE,
            height: ICON_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    )
}

fn placeholder_icon_rgba() -> Vec<u8> {
    let mut rgba = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..ICON_SIZE {
        for x in 0..ICON_SIZE {
            let border = x < 2 || y < 2 || x >= ICON_SIZE - 2 || y >= ICON_SIZE - 2;
            if border {
                rgba.extend_from_slice(&[80, 80, 90, 255]);
            } else {
                rgba.extend_from_slice(&[55, 55, 62, 255]);
            }
        }
    }
    rgba
}

struct Triangle {
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    z0: f32,
    z1: f32,
    z2: f32,
    uv0: [f32; 2],
    uv1: [f32; 2],
    uv2: [f32; 2],
    tint: f32,
}

fn rasterize_block_icon(
    mesh: &ChunkMesh,
    profile: IconBakeProfile,
    atlas: &[u8],
    atlas_w: u32,
    atlas_h: u32,
    grass_tint: [f32; 3],
    foliage_tint: [f32; 3],
    power_dark: [f32; 3],
    power_bright: [f32; 3],
) -> Vec<u8> {
    let mut tris = Vec::new();
    collect_triangles(mesh, profile, 0, &mut tris);
    collect_triangles(mesh, profile, 1, &mut tris);
    collect_triangles(mesh, profile, 2, &mut tris);
    fit_triangles_to_icon(&mut tris, profile.margin);
    tris.sort_by(|a, b| {
        let za = (a.z0 + a.z1 + a.z2) / 3.0;
        let zb = (b.z0 + b.z1 + b.z2) / 3.0;
        za.partial_cmp(&zb).unwrap_or(std::cmp::Ordering::Equal)
    });

    let size = ICON_SIZE as f32;
    let mut color = vec![0u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    let mut depth = vec![f32::MAX; (ICON_SIZE * ICON_SIZE) as usize];

    for tri in &tris {
        rasterize_triangle(
            tri,
            profile.alpha_cutoff,
            &mut color,
            &mut depth,
            size,
            atlas,
            atlas_w,
            atlas_h,
            grass_tint,
            foliage_tint,
            power_dark,
            power_bright,
        );
    }

    color
}

fn fit_triangles_to_icon(tris: &mut [Triangle], margin: f32) {
    if tris.is_empty() {
        return;
    }

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for tri in tris.iter() {
        for p in [tri.p0, tri.p1, tri.p2] {
            min_x = min_x.min(p[0]);
            max_x = max_x.max(p[0]);
            min_y = min_y.min(p[1]);
            max_y = max_y.max(p[1]);
        }
    }

    let w = (max_x - min_x).max(1e-6);
    let h = (max_y - min_y).max(1e-6);
    let avail = ICON_SIZE as f32 - margin * 2.0;
    let scale = avail / w.max(h);
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    let icon_c = ICON_SIZE as f32 * 0.5;

    for tri in tris.iter_mut() {
        for p in [&mut tri.p0, &mut tri.p1, &mut tri.p2] {
            p[0] = (p[0] - cx) * scale + icon_c;
            p[1] = (p[1] - cy) * scale + icon_c;
        }
    }
}

fn collect_triangles(
    mesh: &ChunkMesh,
    profile: IconBakeProfile,
    bucket: u8,
    out: &mut Vec<Triangle>,
) {
    let (verts, indices) = match bucket {
        1 => (
            &mesh.transparent_vertices,
            &mesh.transparent_indices,
        ),
        2 => (&mesh.cutout_vertices, &mesh.cutout_indices),
        _ => (&mesh.opaque_vertices, &mesh.opaque_indices),
    };

    for chunk in indices.chunks(3) {
        if chunk.len() < 3 {
            continue;
        }
        let v0 = &verts[chunk[0] as usize];
        let v1 = &verts[chunk[1] as usize];
        let v2 = &verts[chunk[2] as usize];
        let (p0, z0) = project(v0.position, profile);
        let (p1, z1) = project(v1.position, profile);
        let (p2, z2) = project(v2.position, profile);
        out.push(Triangle {
            p0,
            p1,
            p2,
            z0,
            z1,
            z2,
            uv0: v0.uv,
            uv1: v1.uv,
            uv2: v2.uv,
            tint: v0.tint,
        });
    }
}

fn project(pos: [f32; 3], profile: IconBakeProfile) -> ([f32; 2], f32) {
    let mut x = pos[0] - 0.5;
    let y = pos[1] - 0.5;
    let mut z = pos[2] - 0.5;

    if profile.yaw != 0.0 {
        let (s, c) = profile.yaw.sin_cos();
        let rx = x * c - z * s;
        let rz = x * s + z * c;
        x = rx;
        z = rz;
    }

    let (px, py, depth) = match profile.projection {
        IconProjection::Isometric => {
            let px = x - z;
            let py = -y * 1.25 + (x + z) * 0.5;
            (px, py, x + z + y)
        }
        IconProjection::TopDown => (x, z, -y),
    };

    ([px, py], depth)
}

fn rasterize_triangle(
    tri: &Triangle,
    alpha_cutoff: u8,
    color: &mut [u8],
    depth: &mut [f32],
    size: f32,
    atlas: &[u8],
    atlas_w: u32,
    atlas_h: u32,
    grass_tint: [f32; 3],
    foliage_tint: [f32; 3],
    power_dark: [f32; 3],
    power_bright: [f32; 3],
) {
    let min_x = tri.p0[0].min(tri.p1[0]).min(tri.p2[0]).floor().max(0.0) as i32;
    let max_x = tri.p0[0].max(tri.p1[0]).max(tri.p2[0]).ceil().min(size - 1.0) as i32;
    let min_y = tri.p0[1].min(tri.p1[1]).min(tri.p2[1]).floor().max(0.0) as i32;
    let max_y = tri.p0[1].max(tri.p1[1]).max(tri.p2[1]).ceil().min(size - 1.0) as i32;

    let area = edge(tri.p0, tri.p1, tri.p2);
    if area.abs() < 1e-6 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let w0 = edge(tri.p1, tri.p2, p) / area;
            let w1 = edge(tri.p2, tri.p0, p) / area;
            let w2 = edge(tri.p0, tri.p1, p) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let z = w0 * tri.z0 + w1 * tri.z1 + w2 * tri.z2;
            let idx = (y as u32 * ICON_SIZE + x as u32) as usize;
            if z >= depth[idx] {
                continue;
            }
            depth[idx] = z;

            let u = w0 * tri.uv0[0] + w1 * tri.uv1[0] + w2 * tri.uv2[0];
            let v = w0 * tri.uv0[1] + w1 * tri.uv1[1] + w2 * tri.uv2[1];
            let mut px = sample_atlas(atlas, atlas_w, atlas_h, u, v);
            apply_tint(
                &mut px,
                tri.tint,
                grass_tint,
                foliage_tint,
                power_dark,
                power_bright,
            );

            if px[3] < alpha_cutoff {
                continue;
            }

            let i = idx * 4;
            color[i..i + 3].copy_from_slice(&px[0..3]);
            color[i + 3] = 255;
        }
    }
}

fn edge(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f32 {
    (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
}

fn sample_atlas(atlas: &[u8], aw: u32, ah: u32, u: f32, v: f32) -> [u8; 4] {
    let x = (u * aw as f32).clamp(0.0, aw as f32 - 1.0) as u32;
    let y = (v * ah as f32).clamp(0.0, ah as f32 - 1.0) as u32;
    let i = ((y * aw + x) * 4) as usize;
    if i + 3 >= atlas.len() {
        return [128, 128, 128, 255];
    }
    [atlas[i], atlas[i + 1], atlas[i + 2], atlas[i + 3]]
}

fn apply_tint(
    px: &mut [u8; 4],
    tint: f32,
    grass: [f32; 3],
    foliage: [f32; 3],
    power_dark: [f32; 3],
    power_bright: [f32; 3],
) {
    if tint >= TINT_POWER_BASE {
        let power = decode_power_tint(tint);
        let rs = [
            power_dark[0] + (power_bright[0] - power_dark[0]) * power,
            power_dark[1] + (power_bright[1] - power_dark[1]) * power,
            power_dark[2] + (power_bright[2] - power_dark[2]) * power,
        ];
        for c in 0..3 {
            px[c] = (px[c] as f32 * rs[c]).clamp(0.0, 255.0) as u8;
        }
        return;
    }
    if tint >= 1.5 {
        for c in 0..3 {
            px[c] = (px[c] as f32 * foliage[c]).clamp(0.0, 255.0) as u8;
        }
        return;
    }
    if tint >= 0.5 {
        for c in 0..3 {
            px[c] = (px[c] as f32 * grass[c]).clamp(0.0, 255.0) as u8;
        }
    }
}
