use crate::types::{Project, SearchResult, Version};
use crate::version_select::{pick_best_version, primary_zip_file, DEFAULT_TARGET_GAME_VERSION};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use sha1::{Digest, Sha1};
use thiserror::Error;

const API_BASE: &str = "https://api.modrinth.com/v2";
const USER_AGENT: &str = concat!("stagcrest/", env!("CARGO_PKG_VERSION"));
/// Maximum allowed resource pack download size (256 MiB).
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

/// Characters encoded in Modrinth search query parameters.
const QUERY_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b':')
    .add(b'[')
    .add(b']')
    .add(b'`')
    .add(b'{')
    .add(b'}');

#[derive(Debug, Error)]
pub enum ModrinthError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("API error ({status}): {body}")]
    Api { status: u16, body: String },
    #[error("no suitable version found for project {0}")]
    NoVersion(String),
    #[error("no downloadable file for version {0}")]
    NoFile(String),
    #[error("download exceeds size limit ({size} bytes)")]
    DownloadTooLarge { size: u64 },
    #[error("download size mismatch (expected {expected}, got {actual})")]
    SizeMismatch { expected: u64, actual: usize },
    #[error("missing sha1 hash for version file")]
    MissingHash,
    #[error("hash mismatch (expected {expected}, got {actual})")]
    HashMismatch { expected: String, actual: String },
}

pub struct ModrinthClient {
    http: reqwest::blocking::Client,
    target_game_version: String,
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ModrinthClient {
    pub fn new() -> Self {
        let http = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .expect("reqwest client");
        Self {
            http,
            target_game_version: DEFAULT_TARGET_GAME_VERSION.to_string(),
        }
    }

    pub fn with_target_game_version(mut self, version: impl Into<String>) -> Self {
        self.target_game_version = version.into();
        self
    }

    pub fn get_project(&self, slug_or_id: &str) -> Result<Project, ModrinthError> {
        let url = format!("{API_BASE}/project/{slug_or_id}");
        self.get_json(&url)
    }

    pub fn list_versions(&self, slug_or_id: &str) -> Result<Vec<Version>, ModrinthError> {
        let url = format!("{API_BASE}/project/{slug_or_id}/version?loaders=[\"minecraft\"]");
        self.get_json(&url)
    }

    pub fn best_version(&self, slug_or_id: &str) -> Result<Version, ModrinthError> {
        let versions = self.list_versions(slug_or_id)?;
        pick_best_version(&versions, &self.target_game_version)
            .cloned()
            .ok_or_else(|| ModrinthError::NoVersion(slug_or_id.to_string()))
    }

    pub fn search_resource_packs(
        &self,
        query: &str,
        limit: u32,
        offset: u32,
    ) -> Result<SearchResult, ModrinthError> {
        let facets = r#"[["project_type:resourcepack"]]"#;
        let url = format!(
            "{API_BASE}/search?query={}&facets={}&limit={}&offset={}",
            encode_query_component(query),
            encode_query_component(facets),
            limit,
            offset
        );
        self.get_json(&url)
    }

    pub fn download_version_bytes(&self, version: &Version) -> Result<Vec<u8>, ModrinthError> {
        let file =
            primary_zip_file(version).ok_or_else(|| ModrinthError::NoFile(version.id.clone()))?;

        if file.size > MAX_DOWNLOAD_BYTES {
            return Err(ModrinthError::DownloadTooLarge { size: file.size });
        }

        let response = self.http.get(&file.url).send()?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(ModrinthError::Api { status, body });
        }

        let bytes = response.bytes()?.to_vec();
        Self::verify_downloaded_file(file, &bytes)?;
        Ok(bytes)
    }

    fn verify_downloaded_file(
        file: &crate::types::VersionFile,
        bytes: &[u8],
    ) -> Result<(), ModrinthError> {
        if file.size > MAX_DOWNLOAD_BYTES {
            return Err(ModrinthError::DownloadTooLarge {
                size: bytes.len() as u64,
            });
        }

        if file.size > 0 && bytes.len() as u64 != file.size {
            return Err(ModrinthError::SizeMismatch {
                expected: file.size,
                actual: bytes.len(),
            });
        }

        let expected = file.hashes.get("sha1").ok_or(ModrinthError::MissingHash)?;
        let actual = hex_sha1(bytes);
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(ModrinthError::HashMismatch {
                expected: expected.clone(),
                actual,
            });
        }

        Ok(())
    }

    pub fn install_resource_pack(
        &self,
        slug_or_id: &str,
    ) -> Result<(Project, Version, Vec<u8>), ModrinthError> {
        let project = self.get_project(slug_or_id)?;
        let version = self.best_version(&project.slug)?;
        let bytes = self.download_version_bytes(&version)?;
        Ok((project, version, bytes))
    }

    fn get_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T, ModrinthError> {
        let response = self.http.get(url).send()?;
        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(ModrinthError::Api { status, body });
        }
        Ok(response.json()?)
    }
}

fn encode_query_component(value: &str) -> String {
    utf8_percent_encode(value, QUERY_ENCODE_SET).to_string()
}

fn hex_sha1(bytes: &[u8]) -> String {
    let digest = Sha1::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
