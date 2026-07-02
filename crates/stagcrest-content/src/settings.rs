use serde::{Deserialize, Serialize};
use stagcrest_storage::DATA_DIR;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

pub const SETTINGS_FILE: &str = "settings.toml";
const RESOURCE_PACKS_DIR: &str = "resourcepacks";

#[derive(Debug, Error)]
pub enum SettingsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
    #[error("unknown resource pack id: {0}")]
    UnknownPack(String),
    #[error("invalid resource pack path: {0}")]
    InvalidPath(String),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentSettingsFile {
    #[serde(default)]
    pub content: ContentSection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContentSection {
    #[serde(default)]
    pub resource_pack_order: Vec<String>,
    #[serde(default)]
    pub resource_pack_setup_dismissed: bool,
    #[serde(default)]
    pub resource_packs: Vec<ResourcePackEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthSource {
    pub project: String,
    pub version_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePackEntry {
    pub id: String,
    pub path: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_source")]
    pub source: ContentSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
}

fn default_enabled() -> bool {
    true
}

fn default_source() -> ContentSource {
    ContentSource::local()
}

#[derive(Debug, Clone)]
pub enum ContentSource {
    Local,
    Modrinth(ModrinthSource),
}

impl ContentSource {
    pub fn local() -> Self {
        ContentSource::Local
    }

    pub fn modrinth(project: impl Into<String>, version_id: impl Into<String>) -> Self {
        ContentSource::Modrinth(ModrinthSource {
            project: project.into(),
            version_id: version_id.into(),
        })
    }

    pub fn is_modrinth(&self) -> bool {
        matches!(self, ContentSource::Modrinth { .. })
    }
}

impl Serialize for ContentSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ContentSource::Local => serializer.serialize_str("local"),
            ContentSource::Modrinth(m) => {
                #[derive(Serialize)]
                struct Wrap<'a> {
                    modrinth: &'a ModrinthSource,
                }
                Wrap { modrinth: m }.serialize(serializer)
            }
        }
    }
}

impl<'de> Deserialize<'de> for ContentSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        use std::fmt;

        struct SourceVisitor;

        impl<'de> Visitor<'de> for SourceVisitor {
            type Value = ContentSource;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a content source string or modrinth table")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v == "local" {
                    Ok(ContentSource::Local)
                } else {
                    Err(de::Error::invalid_value(de::Unexpected::Str(v), &self))
                }
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                #[derive(Deserialize)]
                struct Wrap {
                    modrinth: ModrinthSource,
                }
                let wrap = Wrap::deserialize(de::value::MapAccessDeserializer::new(map))?;
                Ok(ContentSource::Modrinth(wrap.modrinth))
            }
        }

        deserializer.deserialize_any(SourceVisitor)
    }
}

#[derive(Debug, Clone)]
pub struct ContentSettings {
    data_dir: PathBuf,
    file: ContentSettingsFile,
}

impl ContentSettings {
    pub fn load(data_dir: impl AsRef<Path>) -> Result<Self, SettingsError> {
        let data_dir = data_dir.as_ref().to_path_buf();
        let settings_path = data_dir.join(SETTINGS_FILE);

        let mut file = if settings_path.exists() {
            let content = std::fs::read_to_string(&settings_path)?;
            toml::from_str(&content)?
        } else {
            ContentSettingsFile::default()
        };

        Self::sanitize_loaded_packs(&mut file);

        let mut this = Self { data_dir, file };
        this.normalize_order();
        Ok(this)
    }

    /// Reject path traversal and non-single-segment pack directory names.
    pub fn validate_pack_path(path: &str) -> Result<(), SettingsError> {
        if path.is_empty() {
            return Err(SettingsError::InvalidPath("empty path".into()));
        }
        if path.contains('\\') {
            return Err(SettingsError::InvalidPath(
                "backslash not allowed".into(),
            ));
        }
        let p = Path::new(path);
        if p.is_absolute() {
            return Err(SettingsError::InvalidPath("absolute path".into()));
        }
        for comp in p.components() {
            if !matches!(comp, Component::Normal(_)) {
                return Err(SettingsError::InvalidPath(path.into()));
            }
        }
        Ok(())
    }

    fn sanitize_loaded_packs(file: &mut ContentSettingsFile) {
        file.content.resource_packs.retain(|pack| {
            if let Err(e) = Self::validate_pack_path(&pack.path) {
                tracing::warn!("skipping resource pack {}: {e}", pack.id);
                false
            } else {
                true
            }
        });
        let known: std::collections::HashSet<_> = file
            .content
            .resource_packs
            .iter()
            .map(|p| p.id.clone())
            .collect();
        file.content
            .resource_pack_order
            .retain(|id| known.contains(id));
    }

    pub fn empty(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            file: ContentSettingsFile::default(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn content(&self) -> &ContentSection {
        &self.file.content
    }

    pub fn content_mut(&mut self) -> &mut ContentSection {
        &mut self.file.content
    }

    pub fn resource_pack_setup_dismissed(&self) -> bool {
        self.file.content.resource_pack_setup_dismissed
    }

    pub fn set_setup_dismissed(&mut self, dismissed: bool) {
        self.file.content.resource_pack_setup_dismissed = dismissed;
    }

    pub fn has_enabled_pack(&self) -> bool {
        let order = &self.file.content.resource_pack_order;
        self.file
            .content
            .resource_packs
            .iter()
            .any(|p| p.enabled && order.contains(&p.id) && self.pack_dir_exists(p))
    }

    pub fn is_pack_valid(&self, entry: &ResourcePackEntry) -> bool {
        self.pack_dir_exists(entry)
    }

    fn pack_dir_exists(&self, entry: &ResourcePackEntry) -> bool {
        self.resource_pack_root(entry).join("pack.mcmeta").is_file()
    }

    pub fn resource_pack_root(&self, entry: &ResourcePackEntry) -> PathBuf {
        self.data_dir
            .join(RESOURCE_PACKS_DIR)
            .join(&entry.path)
    }

    /// Ordered enabled pack directory paths relative to repo/data root (for AssetReader).
    pub fn enabled_pack_asset_paths(&self) -> Vec<String> {
        let packs_dir = format!("{DATA_DIR}/{RESOURCE_PACKS_DIR}");
        self.file
            .content
            .resource_pack_order
            .iter()
            .filter_map(|id| {
                let entry = self.pack_by_id(id)?;
                if !entry.enabled || !self.pack_dir_exists(entry) {
                    return None;
                }
                Some(format!("{packs_dir}/{}", entry.path))
            })
            .collect()
    }

    /// Absolute paths to enabled pack roots.
    pub fn enabled_pack_paths(&self) -> Vec<PathBuf> {
        self.file
            .content
            .resource_pack_order
            .iter()
            .filter_map(|id| {
                let entry = self.pack_by_id(id)?;
                if !entry.enabled || !self.pack_dir_exists(entry) {
                    return None;
                }
                Some(self.resource_pack_root(entry))
            })
            .collect()
    }

    pub fn pack_by_id(&self, id: &str) -> Option<&ResourcePackEntry> {
        self.file.content.resource_packs.iter().find(|p| p.id == id)
    }

    pub fn pack_by_id_mut(&mut self, id: &str) -> Option<&mut ResourcePackEntry> {
        self.file
            .content
            .resource_packs
            .iter_mut()
            .find(|p| p.id == id)
    }

    pub fn upsert_pack(&mut self, entry: ResourcePackEntry) -> Result<(), SettingsError> {
        Self::validate_pack_path(&entry.path)?;
        if let Some(existing) = self.pack_by_id_mut(&entry.id) {
            *existing = entry;
        } else {
            self.file.content.resource_packs.push(entry.clone());
            if !self.file.content.resource_pack_order.contains(&entry.id) {
                self.file.content.resource_pack_order.push(entry.id);
            }
        }
        self.normalize_order();
        Ok(())
    }

    pub fn remove_pack(&mut self, id: &str) -> Option<ResourcePackEntry> {
        self.file.content.resource_pack_order.retain(|x| x != id);
        let idx = self
            .file
            .content
            .resource_packs
            .iter()
            .position(|p| p.id == id)?;
        Some(self.file.content.resource_packs.remove(idx))
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> Result<(), SettingsError> {
        let entry = self
            .pack_by_id_mut(id)
            .ok_or_else(|| SettingsError::UnknownPack(id.to_string()))?;
        entry.enabled = enabled;
        Ok(())
    }

    pub fn move_pack(&mut self, id: &str, direction: MoveDirection) -> Result<(), SettingsError> {
        let order = &mut self.file.content.resource_pack_order;
        let Some(pos) = order.iter().position(|x| x == id) else {
            return Err(SettingsError::UnknownPack(id.to_string()));
        };
        let new_pos = match direction {
            MoveDirection::Up if pos > 0 => pos - 1,
            MoveDirection::Down if pos + 1 < order.len() => pos + 1,
            _ => return Ok(()),
        };
        order.swap(pos, new_pos);
        Ok(())
    }

    fn normalize_order(&mut self) {
        let known: std::collections::HashSet<_> =
            self.file.content.resource_packs.iter().map(|p| p.id.clone()).collect();
        self.file
            .content
            .resource_pack_order
            .retain(|id| known.contains(id));
        for pack in &self.file.content.resource_packs {
            if !self.file.content.resource_pack_order.contains(&pack.id) {
                self.file.content.resource_pack_order.push(pack.id.clone());
            }
        }
    }

    pub fn save(&self) -> Result<(), SettingsError> {
        let path = self.data_dir.join(SETTINGS_FILE);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(&self.file)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MoveDirection {
    Up,
    Down,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn settings_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut settings = ContentSettings::empty(dir.path());
        settings.upsert_pack(ResourcePackEntry {
            id: "a".into(),
            path: "Pack A".into(),
            enabled: true,
            source: ContentSource::local(),
            installed_at: None,
        })
        .unwrap();
        settings.save().unwrap();

        let loaded = ContentSettings::load(dir.path()).unwrap();
        assert_eq!(loaded.content().resource_packs.len(), 1);
        assert_eq!(loaded.content().resource_pack_order, vec!["a"]);
    }

    #[test]
    fn rejects_traversal_path() {
        let dir = TempDir::new().unwrap();
        let mut settings = ContentSettings::empty(dir.path());
        let err = settings
            .upsert_pack(ResourcePackEntry {
                id: "evil".into(),
                path: "../escape".into(),
                enabled: true,
                source: ContentSource::local(),
                installed_at: None,
            })
            .unwrap_err();
        assert!(matches!(err, SettingsError::InvalidPath(_)));
    }

    #[test]
    fn enabled_paths_skip_missing_mcmeta() {
        let dir = TempDir::new().unwrap();
        let mut settings = ContentSettings::empty(dir.path());
        settings
            .upsert_pack(ResourcePackEntry {
                id: "ghost".into(),
                path: "ghost".into(),
                enabled: true,
                source: ContentSource::local(),
                installed_at: None,
            })
            .unwrap();
        assert!(settings.enabled_pack_asset_paths().is_empty());
    }

    #[test]
    fn reorder_moves_priority() {
        let dir = TempDir::new().unwrap();
        let mut settings = ContentSettings::empty(dir.path());
        for id in ["a", "b", "c"] {
            settings
                .upsert_pack(ResourcePackEntry {
                    id: id.into(),
                    path: id.into(),
                    enabled: true,
                    source: ContentSource::local(),
                    installed_at: None,
                })
                .unwrap();
        }
        settings.move_pack("c", MoveDirection::Up).unwrap();
        settings.move_pack("c", MoveDirection::Up).unwrap();
        assert_eq!(
            settings.content().resource_pack_order,
            vec!["c", "a", "b"]
        );
    }
}
