use std::collections::HashSet;

use image::{ImageBuffer, Rgba};
use indicatif::{ProgressBar, ProgressStyle};
use stagcrest_minimap::{
    write_strip_to_rgba, BlockDefTable, ColumnResolver, MinimapBiomeClimate, MinimapColorCtx,
    StorageStripSource,
};
use stagcrest_mod_server::{
    load_mods, world_chunk_y_bounds, BiomeRegistry, ColormapSet, FsAssetReader, ModHost,
    TerrainConfig,
};
use stagcrest_protocol::{BlockDef, ChunkPos, CHUNK_SIZE};
use stagcrest_storage::WorldMeta;
use thiserror::Error;

use crate::session::WorldSession;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
    #[error("mod: {0}")]
    Mod(#[from] stagcrest_mod_server::ModError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("image: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Message(String),
}

pub struct ExportMinimapConfig {
    pub world_name: String,
    pub output: std::path::PathBuf,
    pub mods_root: std::path::PathBuf,
    pub padding: i32,
    pub scale: u32,
}

pub fn export_minimap(config: ExportMinimapConfig) -> Result<(), ExportError> {
    let session = WorldSession::open(&config.world_name, 0)?;
    let _meta = WorldMeta::load(session.storage.as_ref())?;
    let positions = session.storage.iter_chunk_positions()?;
    if positions.is_empty() {
        return Err(ExportError::Message(
            "no saved chunks in world — explore the world first".into(),
        ));
    }

    let scan = ProgressBar::new_spinner();
    scan.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} Scanning world... {msg}")
            .unwrap(),
    );
    scan.set_message(format!("{} chunks", positions.len()));

    let (world_x0, world_z0, world_x1, world_z1) =
        world_bounds_from_chunks(&positions, config.padding);
    let width = (world_x1 - world_x0 + 1) as u32;
    let height = (world_z1 - world_z0 + 1) as u32;
    scan.finish_with_message(format!(
        "bounds x=[{world_x0},{world_x1}] z=[{world_z0},{world_z1}] ({width}×{height} px)"
    ));

    let host = load_mods(&config.mods_root)?;
    let colormaps = ColormapSet::load(&FsAssetReader::new(&config.mods_root), None);
    let block_defs = collect_block_defs(&host);
    let table = BlockDefTable::new(block_defs);
    let air = host.air_block();
    let terrain = TerrainConfig::default();
    let y_bounds = world_chunk_y_bounds(&terrain);
    let y_min = terrain.world_min_y;
    let y_max = terrain.world_max_y;

    let color_ctx = MinimapColorCtx {
        grass: &colormaps.grass,
        grass_w: colormaps.grass_w,
        grass_h: colormaps.grass_h,
        foliage: &colormaps.foliage,
        foliage_w: colormaps.foliage_w,
        foliage_h: colormaps.foliage_h,
        water: &colormaps.water,
        water_w: colormaps.water_w,
        water_h: colormaps.water_h,
    };

    let resolver = ColumnResolver {
        table: &table,
        air,
        y_min,
        y_max,
        color_ctx,
    };

    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for i in (0..rgba.len()).step_by(4) {
        rgba[i + 3] = 255;
    }

    let strip_coords: HashSet<(i32, i32)> = positions.iter().map(|p| (p.x, p.z)).collect();
    let cx_min = strip_coords.iter().map(|(x, _)| *x).min().unwrap();
    let cx_max = strip_coords.iter().map(|(x, _)| *x).max().unwrap();
    let cz_min = strip_coords.iter().map(|(_, z)| *z).min().unwrap();
    let cz_max = strip_coords.iter().map(|(_, z)| *z).max().unwrap();
    let total_strips = ((cx_max - cx_min + 1) * (cz_max - cz_min + 1)) as u64;

    let bar = ProgressBar::new(total_strips);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40} {pos}/{len} strips {elapsed}")
            .unwrap(),
    );

    let climate_for = |idx: u16| biome_climate(&host.biome_registry, idx);

    for cz in cz_min..=cz_max {
        for cx in cx_min..=cx_max {
            let mut strip = StorageStripSource::new(cx, cz);
            if strip_coords.contains(&(cx, cz)) {
                for cy in y_bounds.clone() {
                    let pos = ChunkPos {
                        x: cx,
                        y: cy,
                        z: cz,
                    };
                    if let Some(chunk) = session.storage.load_chunk(pos)? {
                        strip.insert(cy, chunk);
                    }
                }
            }
            let biome_at =
                |wx: i32, wy: i32, wz: i32| {
                    stagcrest_minimap::strip_biome_at(&strip, wx, wy, wz, &climate_for)
                };
            let strip_cache = resolver.resolve_strip(&strip, cx, cz, &biome_at);
            write_strip_to_rgba(
                &strip_cache,
                cx,
                cz,
                &mut rgba,
                width,
                height,
                world_x0,
                world_z0,
            );
            bar.inc(1);
        }
    }
    bar.finish_with_message("done");

    if config.scale > 1 {
        rgba = downscale_rgba(&rgba, width, height, config.scale);
    }

    let write_bar = ProgressBar::new_spinner();
    write_bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} Writing PNG...")
            .unwrap(),
    );
    let out_w = if config.scale > 1 {
        width.div_ceil(config.scale)
    } else {
        width
    };
    let out_h = if config.scale > 1 {
        height.div_ceil(config.scale)
    } else {
        height
    };
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(out_w, out_h, rgba)
        .ok_or_else(|| ExportError::Message("invalid image buffer dimensions".into()))?;
    if let Some(parent) = config.output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    img.save(&config.output)?;
    write_bar.finish_with_message(format!("wrote {}", config.output.display()));

    println!(
        "minimap: {} chunks, {}×{} px, saved to {}",
        positions.len(),
        out_w,
        out_h,
        config.output.display()
    );
    Ok(())
}

fn collect_block_defs(host: &ModHost) -> Vec<BlockDef> {
    host.registry
        .block_ids()
        .into_iter()
        .filter_map(|id| host.registry.block(id).cloned())
        .collect()
}

fn biome_climate(biomes: &BiomeRegistry, index: u16) -> MinimapBiomeClimate {
    let Some(b) = biomes.biome_by_index(index) else {
        return MinimapBiomeClimate::default();
    };
    MinimapBiomeClimate {
        temperature: b.climate_temp,
        downfall: b.climate_downfall,
        grass_color: b.environment.grass_color,
        foliage_color: b.environment.foliage_color,
        water_color: Some(b.environment.water_color),
    }
}

fn world_bounds_from_chunks(positions: &[ChunkPos], padding: i32) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_z = i32::MAX;
    let mut max_z = i32::MIN;
    for p in positions {
        min_x = min_x.min(p.x * CHUNK_SIZE);
        max_x = max_x.max((p.x + 1) * CHUNK_SIZE - 1);
        min_z = min_z.min(p.z * CHUNK_SIZE);
        max_z = max_z.max((p.z + 1) * CHUNK_SIZE - 1);
    }
    (
        min_x - padding,
        min_z - padding,
        max_x + padding,
        max_z + padding,
    )
}

fn downscale_rgba(src: &[u8], width: u32, height: u32, scale: u32) -> Vec<u8> {
    let out_w = width.div_ceil(scale);
    let out_h = height.div_ceil(scale);
    let mut out = vec![0u8; (out_w * out_h * 4) as usize];
    for pz in 0..out_h {
        for px in 0..out_w {
            let mut r = 0u32;
            let mut g = 0u32;
            let mut b = 0u32;
            let mut n = 0u32;
            for dz in 0..scale {
                for dx in 0..scale {
                    let sx = px * scale + dx;
                    let sz = pz * scale + dz;
                    if sx >= width || sz >= height {
                        continue;
                    }
                    let i = ((sz * width + sx) * 4) as usize;
                    r += src[i] as u32;
                    g += src[i + 1] as u32;
                    b += src[i + 2] as u32;
                    n += 1;
                }
            }
            if n > 0 {
                let o = ((pz * out_w + px) * 4) as usize;
                out[o] = (r / n) as u8;
                out[o + 1] = (g / n) as u8;
                out[o + 2] = (b / n) as u8;
                out[o + 3] = 255;
            }
        }
    }
    out
}
