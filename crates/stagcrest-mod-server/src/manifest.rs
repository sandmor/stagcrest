use stagcrest_protocol::manifest::{ColormapSnapshot, ContentManifest, TextureAssetsChunk};
use stagcrest_protocol::{EntityAssetsChunk, EntityManifest};

use crate::colormap::ColormapSet;
use crate::host::ModHost;

const TEXTURES_PER_CHUNK: usize = 32;

impl ModHost {
    pub fn build_content_manifest(&mut self, colormaps: &ColormapSet) -> ContentManifest {
        ContentManifest {
            loaded_mods: self.loaded_mods.clone(),
            registry: self.registry.to_wire_snapshot(),
            colormaps: ColormapSnapshot {
                grass: colormaps.grass.clone(),
                grass_w: colormaps.grass_w,
                grass_h: colormaps.grass_h,
                foliage: colormaps.foliage.clone(),
                foliage_w: colormaps.foliage_w,
                foliage_h: colormaps.foliage_h,
                water: colormaps.water.clone(),
                water_w: colormaps.water_w,
                water_h: colormaps.water_h,
            },
            biomes: self.biome_registry.to_client_snapshot(),
        }
    }

    pub fn build_texture_asset_chunks(&self) -> Vec<TextureAssetsChunk> {
        let assets = self.registry.build_texture_assets();
        if assets.is_empty() {
            return vec![TextureAssetsChunk {
                index: 0,
                total: 1,
                textures: Vec::new(),
            }];
        }
        let chunks: Vec<_> = assets
            .chunks(TEXTURES_PER_CHUNK)
            .map(|c| c.to_vec())
            .collect();
        let total = chunks.len() as u32;
        chunks
            .into_iter()
            .enumerate()
            .map(|(index, textures)| TextureAssetsChunk {
                index: index as u32,
                total,
                textures,
            })
            .collect()
    }

    pub fn build_handshake_content(
        &mut self,
        colormaps: &ColormapSet,
    ) -> (ContentManifest, Vec<TextureAssetsChunk>) {
        let manifest = self.build_content_manifest(colormaps);
        let textures = self.build_texture_asset_chunks();
        (manifest, textures)
    }

    /// Entity type manifest + chunked Bedrock asset transfers for the handshake.
    pub fn build_entity_content(&self) -> (EntityManifest, Vec<EntityAssetsChunk>) {
        (
            self.entity_registry.to_manifest(),
            self.entity_registry.build_asset_chunks(),
        )
    }
}

impl ColormapSet {
    pub fn from_snapshot(snap: &ColormapSnapshot) -> Self {
        Self {
            grass: snap.grass.clone(),
            grass_w: snap.grass_w,
            grass_h: snap.grass_h,
            foliage: snap.foliage.clone(),
            foliage_w: snap.foliage_w,
            foliage_h: snap.foliage_h,
            water: snap.water.clone(),
            water_w: snap.water_w,
            water_h: snap.water_h,
        }
    }
}
