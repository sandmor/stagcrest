#[cfg(target_arch = "wasm32")]
use crate::guest::{
    command_args, command_name, command_reply, get_world_time, set_world_time, HostRegistrar,
};

#[cfg(target_arch = "wasm32")]
use stagcrest_mod_sdk::{ContentRegistrar, RegisterCommandRequest};

/// Register this mod's slash commands with the host.
pub fn register_commands() {
    #[cfg(target_arch = "wasm32")]
    {
        let mut reg = HostRegistrar;
        reg.register_command(RegisterCommandRequest {
            name: "time".into(),
            description: "Set or query the world day/night time.".into(),
            usage: "/time [<value|day|night|noon|midnight>]".into(),
        });
    }
}

/// Entry point invoked by the host when a registered command is dispatched.
#[cfg(target_arch = "wasm32")]
pub fn handle_command() -> i32 {
    let name = match command_name() {
        Some(n) => n,
        None => return 1,
    };
    let args = command_args().unwrap_or_default();

    match name.as_str() {
        "time" => handle_time(args.trim()),
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

#[cfg(target_arch = "wasm32")]
fn parse_time_arg(arg: &str) -> Option<f64> {
    let lower = arg.trim().to_ascii_lowercase();
    match lower.as_str() {
        "midnight" => Some(0.0),
        "night" => Some(100.0),
        "sunrise" | "day" => Some(300.0),
        "noon" => Some(600.0),
        "sunset" => Some(900.0),
        _ => lower.parse::<f64>().ok(),
    }
}

#[cfg(test)]
mod tests {
    use stagcrest_protocol::DAY_LENGTH_SECS;

    fn parse_time_arg(arg: &str) -> Option<f64> {
        let lower = arg.trim().to_ascii_lowercase();
        match lower.as_str() {
            "midnight" => Some(0.0),
            "night" => Some(100.0),
            "sunrise" | "day" => Some(300.0),
            "noon" => Some(600.0),
            "sunset" => Some(900.0),
            _ => lower.parse::<f64>().ok(),
        }
    }

    #[test]
    fn time_presets_match_day_length() {
        assert_eq!(parse_time_arg("noon").unwrap(), DAY_LENGTH_SECS * 0.5);
        assert_eq!(parse_time_arg("midnight").unwrap(), 0.0);
        assert_eq!(parse_time_arg("day").unwrap(), DAY_LENGTH_SECS * 0.25);
    }
}
