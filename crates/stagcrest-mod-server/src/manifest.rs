use stagcrest_protocol::manifest::{AtlasSnapshot, ColormapSnapshot, ContentManifest};

use crate::atlas::TextureAtlas;
use crate::colormap::ColormapSet;
use crate::host::ModHost;

impl ModHost {
    pub fn build_content_manifest(&mut self, colormaps: &ColormapSet) -> ContentManifest {
        let atlas = self.finalize_atlas();
        ContentManifest {
            loaded_mods: self.loaded_mods.clone(),
            registry: self.registry.to_snapshot(),
            atlas: AtlasSnapshot {
                width: atlas.width,
                height: atlas.height,
                pixels: atlas.pixels.clone(),
                placements: atlas.placements.clone(),
            },
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
}

impl TextureAtlas {
    pub fn from_snapshot(snap: &AtlasSnapshot) -> Self {
        Self {
            width: snap.width,
            height: snap.height,
            pixels: snap.pixels.clone(),
            placements: snap
                .placements
                .iter()
                .map(|&(id, rect)| (id, rect))
                .collect(),
        }
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
