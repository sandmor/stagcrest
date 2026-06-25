use stagcrest_protocol::manifest::ContentManifest;

use crate::atlas::TextureAtlas;
use crate::colormap::ColormapSet;
use crate::models::ModelRegistry;
use crate::registry::BlockRegistry;

/// Client-side content rebuilt from a server `ContentManifest`.
pub struct ContentRuntime {
    pub registry: BlockRegistry,
    pub atlas: TextureAtlas,
    pub models: ModelRegistry,
    pub colormaps: ColormapSet,
    pub loaded_mods: Vec<String>,
}

impl ContentRuntime {
    pub fn from_manifest(manifest: ContentManifest) -> Self {
        let registry = BlockRegistry::from_snapshot(manifest.registry);
        let atlas = TextureAtlas::from_snapshot(&manifest.atlas);
        let colormaps = ColormapSet::from_snapshot(&manifest.colormaps);
        Self {
            loaded_mods: manifest.loaded_mods,
            registry,
            atlas,
            models: ModelRegistry::new(),
            colormaps,
        }
    }
}

impl TextureAtlas {
    pub fn from_snapshot(snap: &stagcrest_protocol::AtlasSnapshot) -> Self {
        Self {
            width: snap.width,
            height: snap.height,
            pixels: snap.pixels.clone(),
            placements: snap.placements.clone(),
        }
    }
}

impl ColormapSet {
    pub fn from_snapshot(snap: &stagcrest_protocol::ColormapSnapshot) -> Self {
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
