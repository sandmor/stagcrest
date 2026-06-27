use std::io::Cursor;

use image::{ImageBuffer, RgbaImage};
use stagcrest_protocol::manifest::{
    AtlasTransfer, ColormapSnapshot, ContentManifest,
};

use crate::atlas::TextureAtlas;
use crate::colormap::ColormapSet;
use crate::host::ModHost;

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

    pub fn build_atlas_transfer(&mut self) -> AtlasTransfer {
        let atlas = self.finalize_atlas();
        atlas_to_transfer(&atlas)
    }

    /// Finalize the atlas once, then build manifest metadata and PNG transfer together.
    pub fn build_handshake_content(
        &mut self,
        colormaps: &ColormapSet,
    ) -> (ContentManifest, AtlasTransfer) {
        let atlas = self.finalize_atlas();
        let transfer = atlas_to_transfer(&atlas);
        let manifest = self.build_content_manifest(colormaps);
        (manifest, transfer)
    }
}

pub fn atlas_to_transfer(atlas: &TextureAtlas) -> AtlasTransfer {
    let png = encode_atlas_png(atlas.width, atlas.height, &atlas.pixels);
    AtlasTransfer {
        width: atlas.width,
        height: atlas.height,
        placements: atlas.placements.clone(),
        png,
    }
}

fn encode_atlas_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let img: RgbaImage =
        ImageBuffer::from_raw(width, height, pixels.to_vec()).expect("atlas pixel buffer size");
    let mut png = Vec::new();
    img.write_to(
        &mut Cursor::new(&mut png),
        image::ImageFormat::Png,
    )
    .expect("atlas PNG encode");
    png
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
            placements: transfer
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
