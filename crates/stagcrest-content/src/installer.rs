use crate::settings::{ContentSettings, ContentSource, ResourcePackEntry, SettingsError};
use stagcrest_modrinth::ModrinthClient;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("settings error: {0}")]
    Settings(#[from] SettingsError),
    #[error("modrinth error: {0}")]
    Modrinth(#[from] stagcrest_modrinth::ModrinthError),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid resource pack: {0}")]
    InvalidPack(String),
    #[error("unknown pack id: {0}")]
    UnknownPack(String),
}

#[derive(Debug, Clone)]
pub struct InstalledPack {
    pub id: String,
    pub path: String,
    pub project_title: String,
    pub version_id: String,
}

pub struct ContentInstaller {
    data_dir: PathBuf,
    modrinth: ModrinthClient,
}

impl ContentInstaller {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
            modrinth: ModrinthClient::new(),
        }
    }

    pub fn with_modrinth_client(mut self, client: ModrinthClient) -> Self {
        self.modrinth = client;
        self
    }

    pub fn resource_packs_dir(&self) -> PathBuf {
        self.data_dir.join("resourcepacks")
    }

    pub fn validate_pack_dir(&self, path: &Path) -> Result<(), ContentError> {
        if path.join("pack.mcmeta").is_file() {
            Ok(())
        } else {
            Err(ContentError::InvalidPack(format!(
                "missing pack.mcmeta in {}",
                path.display()
            )))
        }
    }

    pub fn install_modrinth_pack(
        &self,
        project_slug: &str,
    ) -> Result<(InstalledPack, ContentSettings), ContentError> {
        let (project, version, bytes) = self.modrinth.install_resource_pack(project_slug)?;

        let id = project.slug.clone();
        let dest = self.resource_packs_dir().join(&id);
        let tmp = self.resource_packs_dir().join(format!(".install-{id}"));

        if tmp.exists() {
            std::fs::remove_dir_all(&tmp)?;
        }
        std::fs::create_dir_all(&tmp)?;

        let install_result = (|| {
            self.extract_zip_to(&bytes, &tmp)?;
            self.validate_pack_dir(&tmp)?;
            if dest.exists() {
                std::fs::remove_dir_all(&dest)?;
            }
            std::fs::rename(&tmp, &dest)?;
            Ok::<(), ContentError>(())
        })();

        if install_result.is_err() {
            let _ = std::fs::remove_dir_all(&tmp);
            install_result?;
        }

        let mut settings = ContentSettings::load(&self.data_dir)?;
        let entry = ResourcePackEntry {
            id: id.clone(),
            path: id.clone(),
            enabled: true,
            source: ContentSource::modrinth(project.slug.clone(), version.id.clone()),
            installed_at: Some(version.date_published.clone()),
        };
        settings.upsert_pack(entry)?;
        settings.save()?;

        Ok((
            InstalledPack {
                id,
                path: project.slug,
                project_title: project.title,
                version_id: version.id,
            },
            settings,
        ))
    }

    pub fn remove_pack(&self, id: &str) -> Result<ContentSettings, ContentError> {
        let mut settings = ContentSettings::load(&self.data_dir)?;
        let Some(entry) = settings.remove_pack(id) else {
            return Err(ContentError::UnknownPack(id.to_string()));
        };
        let pack_dir = settings.resource_pack_root(&entry);
        if pack_dir.exists() {
            std::fs::remove_dir_all(pack_dir)?;
        }
        settings.save()?;
        Ok(settings)
    }

    fn safe_relative_path(rel: &str) -> Result<PathBuf, ContentError> {
        if rel.is_empty() || rel.contains('\\') {
            return Err(ContentError::InvalidPack(format!("unsafe zip path: {rel}")));
        }
        let p = Path::new(rel);
        if p.is_absolute() {
            return Err(ContentError::InvalidPack(format!("unsafe zip path: {rel}")));
        }
        for comp in p.components() {
            if !matches!(comp, Component::Normal(_)) {
                return Err(ContentError::InvalidPack(format!("unsafe zip path: {rel}")));
            }
        }
        Ok(p.to_path_buf())
    }

    fn extract_zip_to(&self, bytes: &[u8], dest: &Path) -> Result<(), ContentError> {
        let cursor = std::io::Cursor::new(bytes);
        let mut archive = ZipArchive::new(cursor)?;

        // Detect single top-level folder (common for MC packs).
        let mut top_prefix: Option<String> = None;
        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name().replace('\\', "/");
            if let Some(first) = name.split('/').next() {
                if !first.is_empty() && name.contains('/') {
                    match &top_prefix {
                        None => top_prefix = Some(first.to_string()),
                        Some(p) if p == first => {}
                        _ => {
                            top_prefix = None;
                            break;
                        }
                    }
                }
            }
        }

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().replace('\\', "/");
            let rel = if let Some(ref prefix) = top_prefix {
                name.strip_prefix(prefix)
                    .and_then(|s| s.strip_prefix('/'))
                    .unwrap_or(name.as_str())
            } else {
                name.as_str()
            };

            if rel.is_empty() || rel.ends_with('/') {
                if !rel.is_empty() {
                    let dir = dest.join(Self::safe_relative_path(rel)?);
                    if !dir.starts_with(dest) {
                        return Err(ContentError::InvalidPack(format!(
                            "zip path escapes destination: {rel}"
                        )));
                    }
                    std::fs::create_dir_all(dir)?;
                }
                continue;
            }

            let rel_path = Self::safe_relative_path(rel)?;
            let out_path = dest.join(&rel_path);
            if !out_path.starts_with(dest) {
                return Err(ContentError::InvalidPack(format!(
                    "zip path escapes destination: {rel}"
                )));
            }
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            let mut buf = Vec::new();
            file.read_to_end(&mut buf)?;
            out.write_all(&buf)?;
        }

        Ok(())
    }
}
