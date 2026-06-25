use clap::Parser;
use stagcrest_client::LaunchConfig;

#[derive(Parser, Debug)]
#[command(name = "stagcrest-client", about = "Stagcrest game client")]
struct Args {
    /// Connect to a remote server (host:port). Omit for embedded single-player.
    #[arg(long)]
    connect: Option<String>,

    /// Artificial network latency in milliseconds (dev testing).
    #[arg(long, default_value_t = 0)]
    net_sim_latency_ms: u64,
}

fn main() {
    let args = Args::parse();
    stagcrest_client::run_app(LaunchConfig {
        connect: args.connect,
        net_sim_latency_ms: args.net_sim_latency_ms,
    });
}
