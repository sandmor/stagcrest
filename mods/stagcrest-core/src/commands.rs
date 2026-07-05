#[cfg(target_arch = "wasm32")]
use stagcrest_mod_sdk::RegisterCommandRequest;

#[cfg(target_arch = "wasm32")]
use stagcrest_mod_sdk::{
    command_args, command_name, command_reply, get_world_time, register_command, set_world_time,
};

#[cfg(target_arch = "wasm32")]
const ARG_BUF_LEN: usize = 256;
#[cfg(target_arch = "wasm32")]
const NAME_BUF_LEN: usize = 64;

/// Register this mod's slash commands with the host. No-op outside WASM (the
/// host imports are only linked under `wasm32`).
pub fn register_commands() {
    #[cfg(target_arch = "wasm32")]
    register_command(RegisterCommandRequest {
        name: "time".into(),
        description: "Set or query the world day/night time.".into(),
        usage: "/time [<value|day|night|noon|midnight>]".into(),
    });
}

/// Entry point invoked by the host when a registered command is dispatched.
/// Pulls the command name and args from the host, dispatches, and returns
/// `0` on success or nonzero on a handled error (after replying with usage).
#[cfg(target_arch = "wasm32")]
pub fn handle_command() -> i32 {
    let mut name_buf = [0u8; NAME_BUF_LEN];
    let mut arg_buf = [0u8; ARG_BUF_LEN];
    let name = match command_name(&mut name_buf) {
        Some(n) => std::str::from_utf8(&name_buf[..n]).unwrap_or(""),
        None => return 1,
    };
    let args = match command_args(&mut arg_buf) {
        Some(n) => std::str::from_utf8(&arg_buf[..n]).unwrap_or("").trim(),
        None => "",
    };

    match name {
        "time" => handle_time(args),
        other => {
            command_reply(&format!("unknown command: /{}", other));
            1
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn handle_time(args: &str) -> i32 {
    if args.is_empty() {
        let t = get_world_time();
        command_reply(&format!("World time: {t:.0}"));
        return 0;
    }
    match parse_time_arg(args) {
        Some(t) => {
            set_world_time(t);
            command_reply(&format!("Time set to {t:.0}"));
            0
        }
        None => {
            command_reply("Usage: /time [<value|day|night|noon|midnight>]");
            1
        }
    }
}

/// Map a textual time argument to a world-time value (seconds within the
/// day cycle). Accepts a raw float, or named presets.
#[cfg(target_arch = "wasm32")]
fn parse_time_arg(arg: &str) -> Option<f64> {
    let lower = arg.trim().to_ascii_lowercase();
    match lower.as_str() {
        "day" => Some(1000.0),
        "noon" => Some(6000.0),
        "night" => Some(13000.0),
        "midnight" => Some(18000.0),
        _ => lower.parse::<f64>().ok(),
    }
}
