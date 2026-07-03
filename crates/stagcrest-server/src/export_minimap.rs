use std::path::PathBuf;

use image::{ImageBuffer, Rgba};
use indicatif::{ProgressBar, ProgressStyle};
use stagcrest_minimap::{map_chunk_block_bounds, MapChunkBlob, MAP_CHUNK_SIZE};
use thiserror::Error;

use crate::map_tile_maintenance::MapTileRebuildReport;
use crate::offline_bootstrap::{
    bootstrap_offline, open_offline_world, rebuild_all_minimap_tiles, spinner, BootstrapError,
    WorldSeedPolicy,
};
use crate::session::WorldSession;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("bootstrap: {0}")]
    Bootstrap(#[from] BootstrapError),
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
    #[error("map format: {0}")]
    MapFormat(#[from] stagcrest_minimap::MapFormatError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("image: {0}")]
    Image(#[from] image::ImageError),
    #[error("{0}")]
    Message(String),
}

pub struct ExportMinimapConfig {
    pub world_name: String,
    pub output: PathBuf,
    pub mods_root: PathBuf,
    pub padding: i32,
    pub scale: u32,
    pub rebuild_minimap: bool,
    pub jobs: Option<usize>,
}

pub fn export_minimap(config: ExportMinimapConfig) -> Result<(), ExportError> {
    let session = if config.rebuild_minimap {
        let offline = bootstrap_offline(
            &config.world_name,
            &config.mods_root,
            WorldSeedPolicy::UseStored,
        )?;
        let report = rebuild_all_minimap_tiles(&offline, config.jobs)?;
        print_rebuild_summary(&report);
        offline.session
    } else {
        open_offline_world(&config.world_name, WorldSeedPolicy::UseStored)?
    };

    export_png_from_session(&session, &config)
}

/// Rebuild all minimap tiles for a world (single bootstrap: one mod load, one DB open).
pub fn rebuild_all_map_chunks(
    world_name: &str,
    mods_root: &PathBuf,
    jobs: Option<usize>,
) -> Result<MapTileRebuildReport, ExportError> {
    let offline = bootstrap_offline(world_name, mods_root, WorldSeedPolicy::UseStored)?;
    let report = rebuild_all_minimap_tiles(&offline, jobs)?;
    print_rebuild_summary(&report);
    Ok(report)
}

fn print_rebuild_summary(report: &MapTileRebuildReport) {
    println!(
        "{} world chunks → {} map tiles rebuilt",
        report.world_chunks, report.rebuilt
    );
}

fn export_png_from_session(
    session: &WorldSession,
    config: &ExportMinimapConfig,
) -> Result<(), ExportError> {
    let scan = spinner("Scanning map tiles...");
    let map_coords = session.storage.iter_map_chunk_coords()?;
    if map_coords.is_empty() {
        scan.finish_and_clear();
        return Err(ExportError::Message(
            "no map chunks in world — run with --rebuild-minimap after exploring".into(),
        ));
    }
    scan.finish_with_message(format!("{} map tiles", map_coords.len()));

    let (world_x0, world_z0, world_x1, world_z1) =
        map_bounds_from_chunks(&map_coords, config.padding);
    let width = (world_x1 - world_x0 + 1) as u32;
    let height = (world_z1 - world_z0 + 1) as u32;

    let alloc = spinner(format!("Allocating {width}×{height} image..."));
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for i in (0..rgba.len()).step_by(4) {
        rgba[i + 3] = 255;
    }
    alloc.finish_and_clear();

    let bar = ProgressBar::new(map_coords.len() as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40} {pos}/{len} tiles {elapsed}")
            .unwrap(),
    );

    let tile_count = map_coords.len();
    for (mx, mz) in map_coords {
        let Some(blob) = session.storage.get_map_chunk(mx, mz)? else {
            continue;
        };
        let layers = MapChunkBlob::decode(&blob)?;
        let Some(layer) = layers.first() else {
            continue;
        };
        blit_map_tile(&layer.rgb, mx, mz, &mut rgba, width, height, world_x0, world_z0);
        bar.inc(1);
    }
    bar.finish_with_message("done");

    if config.scale > 1 {
        rgba = downscale_rgba(&rgba, width, height, config.scale);
    }

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

    println!(
        "minimap: {tile_count} tiles, {}×{} px, saved to {}",
        out_w,
        out_h,
        config.output.display()
    );
    Ok(())
}

fn map_bounds_from_chunks(coords: &[(i32, i32)], padding: i32) -> (i32, i32, i32, i32) {
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_z = i32::MAX;
    let mut max_z = i32::MIN;
    for &(mx, mz) in coords {
        let (x0, z0, x1, z1) = map_chunk_block_bounds(mx, mz);
        min_x = min_x.min(x0);
        max_x = max_x.max(x1);
        min_z = min_z.min(z0);
        max_z = max_z.max(z1);
    }
    (
        min_x - padding,
        min_z - padding,
        max_x + padding,
        max_z + padding,
    )
}

fn blit_map_tile(
    rgb: &[u8; stagcrest_minimap::MAP_CHUNK_RGB_BYTES],
    mx: i32,
    mz: i32,
    output: &mut [u8],
    img_width: u32,
    img_height: u32,
    world_x0: i32,
    world_z0: i32,
) {
    let tile_x0 = mx * MAP_CHUNK_SIZE;
    let tile_z0 = mz * MAP_CHUNK_SIZE;
    for lz in 0..MAP_CHUNK_SIZE {
        for lx in 0..MAP_CHUNK_SIZE {
            let wx = tile_x0 + lx;
            let wz = tile_z0 + lz;
            if wx < world_x0
                || wz < world_z0
                || wx >= world_x0 + img_width as i32
                || wz >= world_z0 + img_height as i32
            {
                continue;
            }
            let px = (wx - world_x0) as u32;
            let pz = (wz - world_z0) as u32;
            let si = ((lz * MAP_CHUNK_SIZE + lx) * 3) as usize;
            let oi = ((pz * img_width + px) * 4) as usize;
            if oi + 3 < output.len() {
                output[oi] = rgb[si];
                output[oi + 1] = rgb[si + 1];
                output[oi + 2] = rgb[si + 2];
                output[oi + 3] = 255;
            }
        }
    }
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
