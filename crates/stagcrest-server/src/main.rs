use std::path::PathBuf;

use clap::Parser;
use stagcrest_server::{run_standalone, ServerConfig};

#[derive(Parser, Debug)]
#[command(name = "stagcrest-server", about = "Stagcrest dedicated game server")]
struct Args {
    /// TCP bind address (host:port).
    #[arg(long, default_value = "0.0.0.0:4242")]
    bind: String,

    /// World name (storage folder under worlds/).
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
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
