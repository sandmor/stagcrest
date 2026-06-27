use stagcrest_protocol::manifest::{AtlasTransfer, ContentManifest};

use crate::atlas::TextureAtlas;
use crate::biome::BiomeRegistryClient;
use crate::colormap::ColormapSet;
use crate::models::ModelRegistry;
use crate::registry::BlockRegistry;

/// Client-side content rebuilt from server handshake messages.
pub struct ContentRuntime {
    pub registry: BlockRegistry,
    pub atlas: TextureAtlas,
    pub models: ModelRegistry,
    pub colormaps: ColormapSet,
    pub biomes: BiomeRegistryClient,
    pub loaded_mods: Vec<String>,
}

impl ContentRuntime {
    pub fn from_parts(manifest: ContentManifest, atlas: AtlasTransfer) -> Self {
        let registry = BlockRegistry::from_wire_snapshot(manifest.registry);
        let atlas = TextureAtlas::from_transfer(&atlas);
        let colormaps = ColormapSet::from_snapshot(&manifest.colormaps);
        let biomes = BiomeRegistryClient::from_snapshot(manifest.biomes);
        Self {
            loaded_mods: manifest.loaded_mods,
            registry,
            atlas,
            models: ModelRegistry::new(),
            colormaps,
            biomes,
        }
    }
}

impl TextureAtlas {
    pub fn from_transfer(transfer: &AtlasTransfer) -> Self {
        let img = image::load_from_memory(&transfer.png).expect("atlas PNG decode");
        let rgba = img.to_rgba8();
        assert_eq!(
            rgba.width(),
            transfer.width,
            "atlas PNG width mismatch"
        );
        assert_eq!(
            rgba.height(),
            transfer.height,
            "atlas PNG height mismatch"
        );
        Self {
            width: transfer.width,
            height: transfer.height,
            pixels: rgba.into_raw(),
            placements: transfer.placements.clone(),
        }
    }
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
