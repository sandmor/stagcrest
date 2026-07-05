use std::path::Path;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use stagcrest_minimap::MapResolveContext;
use stagcrest_mod_server::{
    load_mods, world_chunk_y_bounds, BiomeRegistry, BlockRegistry, ColormapSet, ColumnBlocks,
    FsAssetReader, ModHost, TerrainConfig,
};
use stagcrest_protocol::BlockId;
use stagcrest_storage::BlockIdRemap;
use thiserror::Error;

use crate::map_generation::make_map_resolve_context;
use crate::map_tile_maintenance::{
    rebuild_all_map_tiles, with_rayon_pool, MapTileError, MapTileRebuildReport,
};
use crate::session::WorldSession;

/// How offline tools resolve the world generation seed when opening storage.
#[derive(Debug, Clone, Copy)]
pub enum WorldSeedPolicy {
    /// Keep seed stored in world meta (or 42 when unset).
    UseStored,
    /// Override with an explicit seed (e.g. `build-map --seed`).
    Override(u64),
}

/// Mods, colormaps, and map-raster context shared by offline worldgen / minimap tools.
pub struct WorldgenContext {
    pub registry: BlockRegistry,
    pub biomes: BiomeRegistry,
    pub colormaps: ColormapSet,
    pub air: BlockId,
    pub column_blocks: ColumnBlocks,
    terrain_config: TerrainConfig,
}

impl WorldgenContext {
    /// Terrain config after biome river settings are applied (matches live server worldgen).
    pub fn resolved_terrain_config(&self) -> TerrainConfig {
        let mut config = self.terrain_config.clone();
        self.biomes
            .river_config()
            .apply_to_terrain_config(&mut config);
        config
    }

    /// Vertical chunk indices scanned when rasterizing minimap tiles offline.
    pub fn map_y_chunks(&self) -> Vec<i32> {
        world_chunk_y_bounds(&self.resolved_terrain_config()).collect()
    }

    /// Color/block resolve context for offline minimap rasterization.
    pub fn map_ctx(&self) -> MapResolveContext {
        make_map_resolve_context(
            &self.registry,
            &self.colormaps,
            &self.biomes,
            self.air,
            &self.resolved_terrain_config(),
        )
    }
}

/// Single mod load + single world DB open for offline commands.
pub struct OfflineWorld {
    pub session: WorldSession,
    pub worldgen: WorldgenContext,
    pub block_remap: Option<BlockIdRemap>,
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("storage: {0}")]
    Storage(#[from] stagcrest_storage::StorageError),
    #[error("mod: {0}")]
    Mod(#[from] stagcrest_mod_server::ModError),
    #[error("map tile: {0}")]
    Map(#[from] MapTileError),
    #[error("{0}")]
    Message(String),
}

pub fn spinner(msg: impl Into<String>) -> ProgressBar {
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

/// Open world storage only (no mod load). Use for read-only map export.
pub fn open_offline_world(
    world_name: &str,
    seed: WorldSeedPolicy,
) -> Result<WorldSession, BootstrapError> {
    let spin = spinner(format!("Opening world '{world_name}'..."));
    let session = match seed {
        WorldSeedPolicy::UseStored => WorldSession::open_with_resolved_seed(world_name, None)?,
        WorldSeedPolicy::Override(s) => WorldSession::open(world_name, s)?,
    };
    spin.finish_with_message(format!("opened world '{world_name}'"));
    Ok(session)
}

/// Load mods and colormaps once.
pub fn load_worldgen_context(mods_root: &Path) -> Result<WorldgenContext, BootstrapError> {
    let spin = spinner("Loading mods and colormaps...");
    let host = load_mods(mods_root)?;
    let ctx = worldgen_context_from_host(&host, mods_root)?;
    spin.finish_with_message("mods loaded");
    Ok(ctx)
}

fn worldgen_context_from_host(
    host: &ModHost,
    mods_root: &Path,
) -> Result<WorldgenContext, BootstrapError> {
    let colormaps = ColormapSet::load(&FsAssetReader::new(mods_root), None);
    let air = host.air_block();
    let terrain_config = TerrainConfig::default();
    let column_blocks = ColumnBlocks::resolve(&host.registry, air);
    Ok(WorldgenContext {
        registry: host.registry.clone(),
        biomes: host.biome_registry.clone(),
        colormaps,
        air,
        column_blocks,
        terrain_config,
    })
}

pub fn block_remap_from_meta(
    host: &ModHost,
    saved: &[stagcrest_storage::BlockRegistryEntry],
) -> Option<BlockIdRemap> {
    if saved.is_empty() {
        None
    } else {
        Some(host.build_id_remap(saved))
    }
}

/// One mod load and one DB open — used by `build-map` and minimap rebuild paths.
pub fn bootstrap_offline(
    world_name: &str,
    mods_root: &Path,
    seed: WorldSeedPolicy,
) -> Result<OfflineWorld, BootstrapError> {
    let spin = spinner("Loading mods and colormaps...");
    let host = load_mods(mods_root)?;
    let worldgen = worldgen_context_from_host(&host, mods_root)?;
    spin.finish_with_message("mods loaded");
    let session = open_offline_world(world_name, seed)?;
    let block_remap = block_remap_from_meta(&host, &session.meta.block_registry);
    Ok(OfflineWorld {
        session,
        worldgen,
        block_remap,
    })
}

/// Rebuild every map tile covering saved world chunks (single storage scan).
pub fn rebuild_all_minimap_tiles(
    offline: &OfflineWorld,
    jobs: Option<usize>,
) -> Result<MapTileRebuildReport, BootstrapError> {
    let storage = offline.session.storage.as_ref();
    let map_ctx = offline.worldgen.map_ctx();
    let y_chunks = offline.worldgen.map_y_chunks();
    let report = with_rayon_pool(jobs, || {
        rebuild_all_map_tiles(
            storage,
            &map_ctx,
            &y_chunks,
            offline.worldgen.air,
            offline.block_remap.as_ref(),
        )
    })?;
    Ok(report)
}
