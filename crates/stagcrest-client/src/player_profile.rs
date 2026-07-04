use bevy::prelude::*;
use stagcrest_content::{ContentSettings, SettingsError};
use stagcrest_storage::DATA_DIR;

/// Default display name for single-player / local sessions.
pub const LOCAL_PLAYER_NAME: &str = "Player";

/// Player identity used when connecting to a multiplayer server.
#[derive(Resource, Clone)]
pub struct PlayerProfile {
    pub username: String,
}

impl Default for PlayerProfile {
    fn default() -> Self {
        Self {
            username: LOCAL_PLAYER_NAME.to_string(),
        }
    }
}

impl PlayerProfile {
    pub fn save_username(&mut self, username: String) -> Result<(), SettingsError> {
        let mut settings =
            ContentSettings::load(DATA_DIR).unwrap_or_else(|_| ContentSettings::empty(DATA_DIR));
        settings.set_username(username.clone())?;
        settings.save()?;
        self.username = username;
        Ok(())
    }
}
