use std::path::PathBuf;

use clap::{Parser, Subcommand};
use stagcrest_server::{
    build_world_region, export_minimap, rebuild_all_map_chunks, run_standalone, BuildMapConfig,
    ExportMinimapConfig, ServerConfig,
};

#[derive(Parser, Debug)]
#[command(name = "stagcrest-server", about = "Stagcrest dedicated game server")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// TCP bind address (host:port).
    #[arg(long, default_value = "0.0.0.0:4242")]
    bind: String,

    /// World name (storage folder under data/worlds/).
    #[arg(long, default_value = "default")]
    world: String,

    /// World generation seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// Root directory containing mods/ and assets.
    #[arg(long, default_value = ".")]
    mods_dir: PathBuf,

    /// Artificial network latency in milliseconds (dev testing).
    #[arg(long, default_value_t = 0)]
    net_sim_latency_ms: u64,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Export a PNG minimap of all saved chunks in a world.
    ExportMinimap {
        /// World name (storage folder under data/worlds/).
        #[arg(long, default_value = "default")]
        world: String,

        /// Output PNG path.
        #[arg(long)]
        output: PathBuf,

        /// Root directory containing mods/ and assets.
        #[arg(long, default_value = ".")]
        mods_dir: PathBuf,

        /// Pixels per block (1 = full resolution; 2 = half, etc.).
        #[arg(long, default_value_t = 1)]
        scale: u32,

        /// Extra blocks padding around saved map tile bounding box.
        #[arg(long, default_value_t = 64)]
        padding: i32,

        /// Rebuild map chunks from saved world data before exporting.
        #[arg(long)]
        rebuild_minimap: bool,

        /// Rayon thread count for parallel map-tile rebuild (default: all cores).
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// Rebuild map chunk tiles in world storage without exporting PNG.
    RebuildMinimap {
        /// World name (storage folder under data/worlds/).
        #[arg(long, default_value = "default")]
        world: String,

        /// Root directory containing mods/ and assets.
        #[arg(long, default_value = ".")]
        mods_dir: PathBuf,

        /// Rayon thread count for parallel map-tile rebuild (default: all cores).
        #[arg(long)]
        jobs: Option<usize>,
    },
    /// Procedurally generate world chunks in a circular region (full vertical span).
    BuildMap {
        /// World name (storage folder under data/worlds/).
        #[arg(long, default_value = "default")]
        world: String,

        /// Root directory containing mods/ and assets.
        #[arg(long, default_value = ".")]
        mods_dir: PathBuf,

        /// World generation seed (default: stored world seed, or 42).
        #[arg(long)]
        seed: Option<u64>,

        /// Circle center block X (default: spawn).
        #[arg(long, default_value_t = 8)]
        center_x: i32,

        /// Circle center block Z (default: spawn).
        #[arg(long, default_value_t = 8)]
        center_z: i32,

        /// Horizontal radius in chunks.
        #[arg(long, default_value_t = 16)]
        radius: i32,

        /// Regenerate chunks even if already saved.
        #[arg(long)]
        force: bool,

        /// Rayon thread count for parallel generation (default: all cores).
        #[arg(long)]
        jobs: Option<usize>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();

    if let Some(Command::ExportMinimap {
        world,
        output,
        mods_dir,
        scale,
        padding,
        rebuild_minimap,
        jobs,
    }) = args.command
    {
        export_minimap(ExportMinimapConfig {
            world_name: world,
            output,
            mods_root: mods_dir,
            padding,
            scale: scale.max(1),
            rebuild_minimap,
            jobs,
        })?;
        return Ok(());
    }

    if let Some(Command::RebuildMinimap {
        world,
        mods_dir,
        jobs,
    }) = args.command
    {
        rebuild_all_map_chunks(&world, &mods_dir, jobs)?;
        println!("map chunks rebuilt for world {world}");
        return Ok(());
    }

    if let Some(Command::BuildMap {
        world,
        mods_dir,
        seed,
        center_x,
        center_z,
        radius,
        force,
        jobs,
    }) = args.command
    {
        let report = build_world_region(BuildMapConfig {
            world_name: world.clone(),
            mods_root: mods_dir,
            center_x,
            center_z,
            radius_chunks: radius,
            seed,
            force,
            jobs,
        })?;
        println!(
            "build-map: world {world} — generated {} chunks, skipped {}, rebuilt {} map tiles",
            report.generated, report.skipped, report.map_tiles_rebuilt
        );
        return Ok(());
    }

    let config = ServerConfig {
        bind: Some(args.bind),
        world_name: args.world,
        world_seed: args.seed,
        mods_root: args.mods_dir,
        render_distance: 8,
        vertical_render_distance: 4,
        net_sim_latency_ms: args.net_sim_latency_ms,
        max_clients: 16,
    };

    run_standalone(config).await
}
