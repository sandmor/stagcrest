use stagcrest_mod_client::{
    model_variant_for_block, BlockRegistry, ModelRegistry,
};
use stagcrest_protocol::{BlockDef, BlockGeometry, ModelId};

use crate::gpu_voxel::bucket::DrawBucketRegistry;
use crate::gpu_voxel::types::{
    GpuAtlasRect, GpuBlockMeta, GpuGeometryKind, GpuRenderLayer, BLOCK_FLAG_FLUID,
    BLOCK_FLAG_OPAQUE, BLOCK_FLAG_REDSTONE_DUST, BLOCK_FLAG_SOLID, BLOCK_FLAG_TRANSPARENT,
    MAX_BLOCK_IDS, MAX_TEXTURES,
};

#[derive(Clone)]
pub struct GpuBlockTables {
    pub block_meta: Vec<GpuBlockMeta>,
    pub atlas_rects: Vec<GpuAtlasRect>,
    pub atlas_size: [u32; 2],
}

impl GpuBlockTables {
    pub fn build(
        registry: &BlockRegistry,
        models: &ModelRegistry,
        buckets: &DrawBucketRegistry,
    ) -> Self {
        let mut block_meta = vec![GpuBlockMeta::default(); MAX_BLOCK_IDS];
        let (aw, ah) = registry.atlas_dimensions();

        let mut atlas_rects = vec![GpuAtlasRect::default(); MAX_TEXTURES];
        for tex in registry.textures() {
            let idx = tex.id.0 as usize;
            if idx >= MAX_TEXTURES {
                continue;
            }
            let rect = registry.atlas_uv(tex.id);
            atlas_rects[idx] = GpuAtlasRect {
                x: rect.x as f32,
                y: rect.y as f32,
                w: rect.w as f32,
                h: rect.h as f32,
            };
        }

        for def in registry.all_blocks() {
            let idx = def.id.0 as usize;
            if idx >= MAX_BLOCK_IDS {
                continue;
            }
            block_meta[idx] = block_meta_for_def(def, registry, models, buckets);
        }

        Self {
            block_meta,
            atlas_rects,
            atlas_size: [aw, ah],
        }
    }
}

fn block_meta_for_def(
    def: &BlockDef,
    registry: &BlockRegistry,
    models: &ModelRegistry,
    buckets: &DrawBucketRegistry,
) -> GpuBlockMeta {
    let mut flags = 0u32;
    if def.opaque {
        flags |= BLOCK_FLAG_OPAQUE;
    }
    if def.solid {
        flags |= BLOCK_FLAG_SOLID;
    }
    if def.fluid {
        flags |= BLOCK_FLAG_FLUID;
    }
    if def.transparent {
        flags |= BLOCK_FLAG_TRANSPARENT;
    }
    if def.namespaced_id == "stagcrest:redstone_dust" {
        flags |= BLOCK_FLAG_REDSTONE_DUST;
    }

    let (geometry_kind, render_layer, model_bucket_base) = match def.geometry {
        BlockGeometry::Cube => (
            GpuGeometryKind::Cube as u32,
            GpuRenderLayer::from_protocol(def.render_layer) as u32,
            0,
        ),
        BlockGeometry::Flat => (
            GpuGeometryKind::Wire as u32,
            GpuRenderLayer::Cutout as u32,
            buckets.wire_bucket_id(),
        ),
        BlockGeometry::Cross => (
            GpuGeometryKind::Cross as u32,
            GpuRenderLayer::Cutout as u32,
            buckets.cross_bucket_id(),
        ),
        BlockGeometry::Model(model_id) => {
            let layer = model_layer(model_id, models);
            let base = buckets.model_bucket_base(model_id);
            (
                GpuGeometryKind::Model as u32,
                layer as u32,
                base,
            )
        }
    };

    let faces = registry
        .block_face_textures_for_state(def.id, stagcrest_protocol::BlockState(0))
        .unwrap_or(def.face_textures);

    let (mut texture_top, mut texture_bottom, mut texture_sides) =
        (faces.top.texture.0, faces.bottom.texture.0, faces.sides.texture.0);

    // Redstone dust composes its look from three textures. Pack them into the
    // three texture slots so the wire compute pass can pick per-connection:
    // top = centre dot, bottom = N/S line, sides = E/W line.
    if def.namespaced_id == "stagcrest:redstone_dust" {
        let tex = |name: &str| registry.texture_by_name(name).map(|t| t.0);
        if let Some(dot) = tex("stagcrest:redstone_dust_dot") {
            texture_top = dot;
        }
        texture_bottom = tex("stagcrest:redstone_dust_line").unwrap_or(texture_top);
        texture_sides = tex("stagcrest:redstone_dust_line1")
            .or_else(|| tex("stagcrest:redstone_dust_line"))
            .unwrap_or(texture_top);
    }

    GpuBlockMeta {
        geometry_kind,
        render_layer,
        flags,
        model_bucket_base,
        texture_top,
        texture_bottom,
        texture_sides,
        block_type_id: block_type_hash(&def.namespaced_id),
    }
}

fn model_layer(model_id: ModelId, models: &ModelRegistry) -> GpuRenderLayer {
    let model = models.get(model_id, 0);
    GpuRenderLayer::from_protocol(model.layer)
}

fn block_type_hash(namespaced_id: &str) -> u32 {
    namespaced_id.bytes().fold(0u32, |h, b| {
        h.wrapping_mul(31).wrapping_add(b as u32)
    })
}

/// Resolve model variant at chunk upload time.
pub fn resolve_variant_for_block(
    namespaced_id: &str,
    state: stagcrest_protocol::BlockState,
) -> u16 {
    model_variant_for_block(namespaced_id, state) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_mod_client::{BlockRegistry, ModelRegistry, TextureAtlas};
    use stagcrest_protocol::{
        BlockDef, BlockFaceTextures, BlockGeometry, BlockId, BlockState, FaceTexture,
        ModelRenderLayer, TintKind,
    };

    fn test_registry() -> BlockRegistry {
        let mut reg = BlockRegistry::new();
        let stone_tex = reg.register_texture("stagcrest:stone".into(), 16, 16, vec![0; 16 * 16 * 4]);
        reg.register_block(BlockDef {
            id: BlockId(1),
            namespaced_id: "stagcrest:stone".into(),
            display_name: "Stone".into(),
            face_textures: BlockFaceTextures {
                top: FaceTexture {
                    texture: stone_tex,
                    overlay: None,
                    tint: TintKind::None,
                    overlay_tint: TintKind::None,
                },
                bottom: FaceTexture {
                    texture: stone_tex,
                    overlay: None,
                    tint: TintKind::None,
                    overlay_tint: TintKind::None,
                },
                sides: FaceTexture {
                    texture: stone_tex,
                    overlay: None,
                    tint: TintKind::None,
                    overlay_tint: TintKind::None,
                },
            },
            geometry: BlockGeometry::Cube,
            render_layer: ModelRenderLayer::Opaque,
            opaque: true,
            solid: true,
            transparent: false,
            fluid: false,
            placeable: true,
            hardness: 1.0,
            circuit: None,
            push_reaction: stagcrest_protocol::PushReaction::Block,
        });
        reg
    }

    #[test]
    fn cube_block_meta_is_opaque_cube() {
        let reg = test_registry();
        let models = ModelRegistry::new();
        let buckets = DrawBucketRegistry::build(&models);
        let tables = GpuBlockTables::build(&reg, &models, &buckets);
        let meta = tables.block_meta[1];
        assert_eq!(meta.geometry_kind, GpuGeometryKind::Cube as u32);
        assert_eq!(meta.render_layer, GpuRenderLayer::Opaque as u32);
        assert!(meta.flags & BLOCK_FLAG_OPAQUE != 0);
        assert!(meta.flags & BLOCK_FLAG_SOLID != 0);
    }
}
