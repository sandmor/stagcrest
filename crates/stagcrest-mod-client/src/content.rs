use stagcrest_atlas::{build_atlas_set, texture_def_from_png, AtlasLimits, AtlasPage, AtlasSet};
use stagcrest_protocol::manifest::{ContentManifest, TextureAssetTransfer};

use crate::biome::BiomeRegistryClient;
use crate::colormap::ColormapSet;
use crate::models::ModelRegistry;
use crate::registry::BlockRegistry;

/// Client-side content rebuilt from server handshake messages.
pub struct ContentRuntime {
    pub registry: BlockRegistry,
    pub atlases: Vec<TextureAtlas>,
    pub models: ModelRegistry,
    pub colormaps: ColormapSet,
    pub biomes: BiomeRegistryClient,
    pub loaded_mods: Vec<String>,
}

pub type TextureAtlas = AtlasPage;

impl ContentRuntime {
    pub fn from_parts(
        manifest: ContentManifest,
        texture_assets: &[TextureAssetTransfer],
        limits: AtlasLimits,
    ) -> Result<Self, String> {
        let mut registry = BlockRegistry::from_wire_snapshot(manifest.registry);
        let atlas_set = build_client_atlas(&registry, texture_assets, limits)?;
        registry.apply_atlas_set(&atlas_set);
        let atlases = atlas_set.pages;
        let colormaps = ColormapSet::from_snapshot(&manifest.colormaps);
        let biomes = BiomeRegistryClient::from_snapshot(manifest.biomes);
        Ok(Self {
            loaded_mods: manifest.loaded_mods,
            registry,
            atlases,
            models: ModelRegistry::new(),
            colormaps,
            biomes,
        })
    }
}

fn build_client_atlas(
    registry: &BlockRegistry,
    assets: &[TextureAssetTransfer],
    limits: AtlasLimits,
) -> Result<AtlasSet, String> {
    let mut decoded = Vec::with_capacity(assets.len());
    for asset in assets {
        let meta = registry
            .textures()
            .find(|t| t.id == asset.id)
            .ok_or_else(|| format!("unknown texture id {:?}", asset.id.0))?;
        let tex = texture_def_from_png(
            asset.id,
            meta.namespaced_id.clone(),
            &asset.png,
            asset.animation.clone().or(meta.animation.clone()),
        )
        .map_err(|e| e.to_string())?;
        decoded.push(tex);
    }
    build_atlas_set(decoded, limits).map_err(|e| e.to_string())
}

impl ColormapSet {
    pub fn from_snapshot(snap: &stagcrest_protocol::manifest::ColormapSnapshot) -> Self {
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
