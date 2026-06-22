use crate::assets::{AssetError, AssetReader};
use serde::Deserialize;
use std::collections::HashMap;

/// Minecraft block texture names referenced by bundled mods (used for web preload).
pub const DEFAULT_MC_BLOCK_TEXTURES: &[&str] = &[
    "stone",
    "dirt",
    "grass_block_top",
    "grass_block_side",
    "grass_block_side_overlay",
    "cobblestone",
    "oak_planks",
    "glass",
    "bedrock",
    "redstone_dust_dot",
    "redstone_dust_line0",
    "redstone_dust_corner0",
    "redstone_dust_t0",
    "redstone_dust_cross0",
    "redstone_torch_off",
    "redstone_torch",
    "redstone_block",
    "lever",
    "repeater",
    "repeater_on",
    "smooth_stone",
];

#[derive(Debug, Deserialize)]
struct ResourcePacksManifest {
    packs: Vec<PackEntry>,
}

#[derive(Debug, Deserialize)]
struct PackEntry {
    id: String,
    path: String,
    enabled: bool,
}

pub struct ResourcePackLoader {
    pack_roots: Vec<String>,
    block_textures: HashMap<String, (u32, u32, Vec<u8>)>,
}

impl ResourcePackLoader {
    pub fn load(reader: &dyn AssetReader) -> Result<Self, AssetError> {
        let pack_roots = Self::read_manifest(reader)?;
        Ok(Self {
            pack_roots,
            block_textures: HashMap::new(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn load_async(reader: &crate::assets::HttpAssetReader) -> Result<Self, AssetError> {
        let pack_roots = Self::read_manifest_async(reader).await?;
        Ok(Self {
            pack_roots,
            block_textures: HashMap::new(),
        })
    }

    fn read_manifest(reader: &dyn AssetReader) -> Result<Vec<String>, AssetError> {
        let manifest_path = "resourcepacks/resourcepacks.toml";
        let mut pack_roots = Vec::new();

        if reader.exists(manifest_path) {
            let content = reader.read_bytes(manifest_path)?;
            pack_roots = Self::parse_manifest(&content)?;
        }

        Ok(pack_roots)
    }

    #[cfg(target_arch = "wasm32")]
    async fn read_manifest_async(
        reader: &crate::assets::HttpAssetReader,
    ) -> Result<Vec<String>, AssetError> {
        let manifest_path = "resourcepacks/resourcepacks.toml";
        let mut pack_roots = Vec::new();

        if reader.exists_async(manifest_path).await {
            let content = reader.read_bytes_async(manifest_path).await?;
            pack_roots = Self::parse_manifest(&content)?;
        }

        Ok(pack_roots)
    }

    fn parse_manifest(content: &[u8]) -> Result<Vec<String>, AssetError> {
        let manifest: ResourcePacksManifest =
            toml::from_str(std::str::from_utf8(content).map_err(|e| {
                AssetError::Io(format!("resourcepacks.toml is not valid UTF-8: {e}"))
            })?)
            .map_err(|e| AssetError::Io(format!("invalid resourcepacks.toml: {e}")))?;
        let mut pack_roots = Vec::new();
        for entry in manifest.packs {
            if !entry.enabled {
                continue;
            }
            pack_roots.push(format!("resourcepacks/{}", entry.path));
            tracing::info!("resource pack enabled: {}", entry.id);
        }
        Ok(pack_roots)
    }

    pub fn validate(&self, reader: &dyn AssetReader) {
        for root in &self.pack_roots {
            let marker = format!("{root}/pack.mcmeta");
            if !reader.exists(&marker) {
                tracing::warn!("resource pack missing pack.mcmeta: {root}");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn validate_async(&self, reader: &crate::assets::HttpAssetReader) {
        for root in &self.pack_roots {
            let marker = format!("{root}/pack.mcmeta");
            if !reader.exists_async(&marker).await {
                tracing::warn!("resource pack missing pack.mcmeta: {root}");
            }
        }
    }

    pub fn warm_block_textures(&mut self, reader: &dyn AssetReader, names: &[&str]) {
        for name in names {
            self.try_load_block_texture(reader, name);
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn warm_block_textures_async(
        &mut self,
        reader: &crate::assets::HttpAssetReader,
        names: &[&str],
    ) {
        for name in names {
            self.try_load_block_texture_async(reader, name).await;
        }
    }

    fn block_texture_path(pack_root: &str, mc_name: &str) -> String {
        let filename = if mc_name.ends_with(".png") {
            mc_name.to_string()
        } else {
            format!("{mc_name}.png")
        };
        format!("{pack_root}/assets/minecraft/textures/block/{filename}")
    }

    pub(crate) fn colormap_path(pack_root: &str, name: &str) -> String {
        let filename = if name.ends_with(".png") {
            name.to_string()
        } else {
            format!("{name}.png")
        };
        format!("{pack_root}/assets/minecraft/textures/colormap/{filename}")
    }

    pub(crate) fn load_rgba_from_bytes(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
        let img = image::load_from_memory(bytes).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        Some((w, h, rgba.into_raw()))
    }

    fn try_load_block_texture(&mut self, reader: &dyn AssetReader, name: &str) {
        if self.block_textures.contains_key(name) {
            return;
        }
        for pack in &self.pack_roots {
            let path = Self::block_texture_path(pack, name);
            if reader.exists(&path) {
                if let Ok(bytes) = reader.read_bytes(&path) {
                    if let Some(tex) = Self::load_rgba_from_bytes(&bytes) {
                        self.block_textures.insert(name.to_string(), tex);
                        return;
                    }
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    async fn try_load_block_texture_async(
        &mut self,
        reader: &crate::assets::HttpAssetReader,
        name: &str,
    ) {
        if self.block_textures.contains_key(name) {
            return;
        }
        for pack in &self.pack_roots {
            let path = Self::block_texture_path(pack, name);
            if reader.exists_async(&path).await {
                if let Ok(bytes) = reader.read_bytes_async(&path).await {
                    if let Some(tex) = Self::load_rgba_from_bytes(&bytes) {
                        self.block_textures.insert(name.to_string(), tex);
                        return;
                    }
                }
            }
        }
    }

    pub fn load_mc_block_texture(&self, name: &str) -> Option<(u32, u32, Vec<u8>)> {
        self.block_textures.get(name).cloned()
    }

    pub fn load_colormap(
        &self,
        reader: &dyn AssetReader,
        name: &str,
    ) -> Option<(u32, u32, Vec<u8>)> {
        for pack in &self.pack_roots {
            let path = Self::colormap_path(pack, name);
            if reader.exists(&path) {
                let bytes = reader.read_bytes(&path).ok()?;
                return Self::load_rgba_from_bytes(&bytes);
            }
        }
        None
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn load_colormap_async(
        &self,
        reader: &crate::assets::HttpAssetReader,
        name: &str,
    ) -> Option<(u32, u32, Vec<u8>)> {
        for pack in &self.pack_roots {
            let path = Self::colormap_path(pack, name);
            if reader.exists_async(&path).await {
                let bytes = reader.read_bytes_async(&path).await.ok()?;
                return Self::load_rgba_from_bytes(&bytes);
            }
        }
        None
    }
}
