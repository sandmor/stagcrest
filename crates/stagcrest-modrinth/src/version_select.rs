use crate::types::{Version, VersionType};

/// Default Minecraft version used when picking Modrinth pack versions.
pub const DEFAULT_TARGET_GAME_VERSION: &str = "1.21.1";

/// Pick the best version for install: newest release matching `target_game_version`,
/// falling back to newest release overall.
pub fn pick_best_version<'a>(
    versions: &'a [Version],
    target_game_version: &str,
) -> Option<&'a Version> {
    let releases: Vec<&Version> = versions
        .iter()
        .filter(|v| v.version_type == VersionType::Release)
        .collect();

    if releases.is_empty() {
        return versions.first();
    }

    let matching: Vec<&Version> = releases
        .iter()
        .copied()
        .filter(|v| v.game_versions.iter().any(|gv| gv == target_game_version))
        .collect();

    let pool = if matching.is_empty() {
        releases
    } else {
        matching
    };

    pool.into_iter()
        .max_by(|a, b| a.date_published.cmp(&b.date_published))
}

/// Resolve the primary downloadable zip file from a version.
pub fn primary_zip_file(version: &Version) -> Option<&crate::types::VersionFile> {
    version
        .files
        .iter()
        .find(|f| f.primary && f.filename.ends_with(".zip"))
        .or_else(|| version.files.iter().find(|f| f.filename.ends_with(".zip")))
}
