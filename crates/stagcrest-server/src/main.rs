use std::path::PathBuf;

use clap::{Parser, Subcommand};
use stagcrest_server::{export_minimap, run_standalone, ExportMinimapConfig, ServerConfig};

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

        /// Extra blocks padding around saved chunk bounding box.
        #[arg(long, default_value_t = 64)]
        padding: i32,
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
    }) = args.command
    {
        export_minimap(ExportMinimapConfig {
            world_name: world,
            output,
            mods_root: mods_dir,
            padding,
            scale: scale.max(1),
        })?;
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
    };

    run_standalone(config).await
}
