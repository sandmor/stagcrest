use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use image::{ImageBuffer, Rgba};
use indicatif::{ProgressBar, ProgressStyle};
use stagcrest_minimap::{
    build_map_chunk_blob, map_chunk_block_bounds, MapChunkBlob, MapChunkLoadInput,
    MapResolveContext, MAP_CHUNK_SIZE,
};
use stagcrest_mod_server::{
    load_mods, world_chunk_y_bounds, ColormapSet, FsAssetReader, TerrainConfig,
};
use stagcrest_storage::WorldMeta;
use thiserror::Error;

use crate::map_generation::make_map_resolve_context;
use crate::session::WorldSession;

fn spinner(msg: impl Into<String>) -> ProgressBar {
    let bar = ProgressBar::new_spinner();
    bar.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner} {msg}")
            .unwrap(),
    );
    bar.enable_steady_tick(Duration::from_millis(100));
    bar.set_message(msg.into());
    bar
}

/// Mod/colormap context required to rasterize map chunks from world data.
pub struct MapExportSetup {
    pub map_ctx: MapResolveContext,
    pub y_chunks: Vec<i32>,
}

pub fn load_map_export_setup(mods_root: &Path) -> Result<MapExportSetup, ExportError> {
    let spin = spinner("Loading mods and colormaps...");
    let host = load_mods(mods_root)?;
    let colormaps = ColormapSet::load(&FsAssetReader::new(mods_root), None);
    let air = host.air_block();
    let terrain = TerrainConfig::default();
    let map_ctx = make_map_resolve_context(
        &host.registry,
        &colormaps,
        &host.biome_registry,
        air,
        &terrain,
    );
    let y_chunks: Vec<i32> = world_chunk_y_bounds(&terrain).collect();
    spin.finish_with_message("mods loaded");
    Ok(MapExportSetup {
        map_ctx,
        y_chunks,
    })
}

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
    #[error("mod: {0}")]
    Mod(#[from] stagcrest_mod_server::ModError),
    #[error("map: {0}")]
    Map(#[from] stagcrest_minimap::MapBuildError),
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
    pub output: std::path::PathBuf,
    pub mods_root: std::path::PathBuf,
    pub padding: i32,
    pub scale: u32,
    pub rebuild_map: bool,
}

pub fn open_world_session(world_name: &str) -> Result<WorldSession, ExportError> {
    let spin = spinner(format!("Opening world '{world_name}'..."));
    let session = WorldSession::open(world_name, 0)?;
    spin.finish_with_message(format!("opened world '{world_name}'"));
    Ok(session)
}

pub fn export_minimap(config: ExportMinimapConfig) -> Result<(), ExportError> {
    let session = open_world_session(&config.world_name)?;
    let _meta = WorldMeta::load(session.storage.as_ref())?;

    if config.rebuild_map {
        let setup = load_map_export_setup(&config.mods_root)?;
        rebuild_all_map_chunks(&session, &setup.map_ctx, &setup.y_chunks)?;
    }

    let scan = spinner("Scanning map tiles...");
    let map_coords = session.storage.iter_map_chunk_coords()?;
    if map_coords.is_empty() {
        scan.finish_and_clear();
        return Err(ExportError::Message(
            "no map chunks in world — run with --rebuild-map after exploring".into(),
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

pub fn rebuild_all_map_chunks(
    session: &WorldSession,
    map_ctx: &MapResolveContext,
    y_chunks: &[i32],
) -> Result<(), ExportError> {
    let scan = spinner("Scanning world chunks...");
    let positions = session.storage.iter_chunk_positions()?;
    if positions.is_empty() {
        scan.finish_and_clear();
        return Err(ExportError::Message(
            "no saved chunks in world — explore the world first".into(),
        ));
    }

    let mut map_coords: HashSet<(i32, i32)> = HashSet::new();
    for pos in &positions {
        map_coords.insert(stagcrest_minimap::world_chunk_to_map_chunk(pos.x, pos.z));
    }
    scan.finish_with_message(format!(
        "{} world chunks → {} map tiles",
        positions.len(),
        map_coords.len()
    ));

    let bar = ProgressBar::new(map_coords.len() as u64);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{bar:40} {pos}/{len} map chunks {elapsed}")
            .unwrap(),
    );

    let empty_overrides = std::collections::HashMap::new();
    for (mx, mz) in map_coords {
        let input = MapChunkLoadInput {
            storage: session.storage.as_ref(),
            world: None,
            y_chunks,
            mx,
            mz,
            overrides: &empty_overrides,
        };
        let blob = build_map_chunk_blob(&input, map_ctx)?;
        session.storage.put_map_chunk(mx, mz, &blob)?;
        bar.inc(1);
    }
    bar.finish_with_message("map chunks rebuilt");
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
