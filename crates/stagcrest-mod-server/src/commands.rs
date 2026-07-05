use std::collections::HashMap;

use stagcrest_mod_sdk::RegisterCommandRequest;

/// Live server state exposed to mod command callbacks. Implemented by the
/// server; methods are only invoked while a mod's `_stagcrest_command` export
/// is running (inside [`crate::host::ModHost::invoke_command`]).
pub trait CommandHost {
    /// Set the world day/night time (seconds within the day cycle).
    fn set_world_time(&mut self, time: f64);
    /// Read the world day/night time.
    fn world_time(&self) -> f64;
    /// Send a `System` chat line to a single client (no-op if the client is gone).
    fn send_chat_to(&mut self, client_id: u64, text: String);
}

/// A mod-registered slash command.
#[derive(Debug, Clone)]
pub struct CommandEntry {
    /// Index into [`crate::host::ModHost`] instances vector.
    pub mod_index: usize,
    pub name: String,
    pub description: String,
    pub usage: String,
}

/// Registry of slash commands keyed by lowercase name.
#[derive(Debug, Default)]
pub struct CommandRegistry {
    by_name: HashMap<String, CommandEntry>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate and register a command. Returns `Ok(())` on success or an
    /// error string on validation failure (bad name or duplicate).
    pub fn register(
        &mut self,
        mod_index: usize,
        req: RegisterCommandRequest,
    ) -> Result<(), String> {
        let name = validate_command_name(&req.name)?;
        if self.by_name.contains_key(&name) {
            return Err(format!("command already registered: /{}", name));
        }
        self.by_name.insert(
            name.clone(),
            CommandEntry {
                mod_index,
                name,
                description: req.description,
                usage: req.usage,
            },
        );
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&CommandEntry> {
        self.by_name.get(&name.to_ascii_lowercase())
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_name.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

/// Lowercase and validate a command name: `[a-z0-9_]{1,32}`.
pub fn validate_command_name(raw: &str) -> Result<String, String> {
    let name = raw.trim().to_ascii_lowercase();
    if name.is_empty() || name.len() > 32 {
        return Err("command name must be 1–32 characters".into());
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        return Err("command name may only contain a-z, 0-9, _".into());
    }
    Ok(name)
}

/// Split a `/command args...` body into `(name, args)`.
///
/// `body` is the text after the leading `/`. The name is the leading run of
/// non-whitespace; args are the remainder (trimmed). Returns `None` if the
/// body is empty or whitespace-only.
pub fn split_command(body: &str) -> Option<(&str, &str)> {
    let body = body.trim_start_matches('/');
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (name, rest) = match trimmed.find(char::is_whitespace) {
        Some(idx) => (&trimmed[..idx], trimmed[idx..].trim()),
        None => (trimmed, ""),
    };
    Some((name, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_names() {
        assert_eq!(validate_command_name("Time").unwrap(), "time");
        assert_eq!(validate_command_name("set_time_2").unwrap(), "set_time_2");
        assert!(validate_command_name("").is_err());
        assert!(validate_command_name("hi there").is_err());
        assert!(validate_command_name("café").is_err());
        assert!(validate_command_name(&"x".repeat(33)).is_err());
    }

    #[test]
    fn rejects_duplicates() {
        let mut reg = CommandRegistry::new();
        reg.register(
            0,
            RegisterCommandRequest {
                name: "time".into(),
                description: "".into(),
                usage: "".into(),
            },
        )
        .unwrap();
        assert!(reg
            .register(
                1,
                RegisterCommandRequest {
                    name: "TIME".into(),
                    description: "".into(),
                    usage: "".into(),
                }
            )
            .is_err());
    }

    #[test]
    fn splits_command_body() {
        assert_eq!(split_command("time"), Some(("time", "")));
        assert_eq!(split_command("time 6000"), Some(("time", "6000")));
        assert_eq!(
            split_command("time   day  night"),
            Some(("time", "day  night"))
        );
        assert_eq!(split_command(""), None);
        assert_eq!(split_command("   "), None);
        assert_eq!(split_command("/time day"), Some(("time", "day")));
    }
}
