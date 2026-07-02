//! Modrinth API v2 client for searching and downloading resource packs.

mod client;
mod types;
mod version_select;

pub use client::{ModrinthClient, ModrinthError};
pub use types::{
    Project, RecommendedPack, SearchHit, SearchResult, Version, VersionFile, VersionType,
};
pub use version_select::{pick_best_version, DEFAULT_TARGET_GAME_VERSION};

/// Curated resource packs suggested to new users.
pub const RECOMMENDED: &[RecommendedPack] = &[
    RecommendedPack {
        slug: "faithful-32x",
        title: "Faithful 32x",
        blurb: "The go-to 32x vanilla-style texture pack.",
    },
    RecommendedPack {
        slug: "faithful-64x",
        title: "Faithful 64x",
        blurb: "Higher-detail vanilla-style textures at 64x resolution.",
    },
];
