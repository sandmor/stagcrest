use crate::assets::{AssetError, AssetReader};
use serde::Deserialize;
use stagcrest_protocol::TextureAnimation;
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
    "water_still",
    "water_flow",
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
    "sand",
    "iron_ore",
    "oak_log",
    "oak_log_top",
    "oak_leaves",
    "short_grass",
    "tall_grass_top",
    "tall_grass_bottom",
    "dandelion",
    "poppy",
    "cactus_side",
    "cactus_top",
    "dead_bush",
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

#[derive(Debug, Clone)]
struct BlockTextureEntry {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    animation: Option<TextureAnimation>,
}

#[derive(Debug, Deserialize)]
struct McMetaRoot {
    animation: Option<McMetaAnimation>,
}

#[derive(Debug, Deserialize)]
struct McMetaAnimation {
    #[serde(default)]
    frametime: u32,
    #[serde(default)]
    frames: Vec<McMetaFrame>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum McMetaFrame {
    Index(u32),
    Object { index: u32 },
}

pub struct ResourcePackLoader {
    pack_roots: Vec<String>,
    block_textures: HashMap<String, BlockTextureEntry>,
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
            self.ensure_block_texture(reader, name);
        }
    }

    pub fn ensure_block_texture(&mut self, reader: &dyn AssetReader, name: &str) {
        if !self.block_textures.contains_key(name) {
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

    fn block_texture_mcmeta_path(pack_root: &str, mc_name: &str) -> String {
        format!("{}.mcmeta", Self::block_texture_path(pack_root, mc_name))
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

    pub fn parse_mcmeta_animation(
        mcmeta_bytes: &[u8],
        texture_width: u32,
        texture_height: u32,
    ) -> Option<TextureAnimation> {
        let root: McMetaRoot = serde_json::from_slice(mcmeta_bytes).ok()?;
        let anim = root.animation?;
        let frame_width = texture_width.max(1);
        let frame_height = if anim.frames.is_empty() {
            frame_width
        } else {
            frame_width
        };
        let frame_count = if anim.frames.is_empty() {
            (texture_height / frame_height).max(1)
        } else {
            anim.frames.len() as u32
        };
        Some(TextureAnimation {
            frame_width,
            frame_height,
            frame_count,
            frametime_ticks: anim.frametime.max(1),
        })
    }

    /// Minecraft-style vertical animation strip when `.mcmeta` is absent.
    pub fn infer_vertical_strip_animation(
        texture_width: u32,
        texture_height: u32,
    ) -> Option<TextureAnimation> {
        if texture_width == 0 || texture_height <= texture_width {
            return None;
        }
        if texture_height % texture_width != 0 {
            return None;
        }
        Some(TextureAnimation {
            frame_width: texture_width,
            frame_height: texture_width,
            frame_count: texture_height / texture_width,
            frametime_ticks: 20,
        })
    }

    fn try_load_block_texture(&mut self, reader: &dyn AssetReader, name: &str) {
        if self.block_textures.contains_key(name) {
            return;
        }
        for pack in &self.pack_roots {
            let path = Self::block_texture_path(pack, name);
            if reader.exists(&path) {
                if let Ok(bytes) = reader.read_bytes(&path) {
                    if let Some((w, h, rgba)) = Self::load_rgba_from_bytes(&bytes) {
                        let mcmeta_path = Self::block_texture_mcmeta_path(pack, name);
                        let mcmeta_exists = reader.exists(&mcmeta_path);
                        let animation = if mcmeta_exists {
                            reader
                                .read_bytes(&mcmeta_path)
                                .ok()
                                .and_then(|b| Self::parse_mcmeta_animation(&b, w, h))
                        } else {
                            None
                        };
                        let animation =
                            animation.or_else(|| Self::infer_vertical_strip_animation(w, h));
                        self.block_textures.insert(
                            name.to_string(),
                            BlockTextureEntry {
                                width: w,
                                height: h,
                                rgba,
                                animation,
                            },
                        );
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
                    if let Some((w, h, rgba)) = Self::load_rgba_from_bytes(&bytes) {
                        let mcmeta_path = Self::block_texture_mcmeta_path(pack, name);
                        let animation = if reader.exists_async(&mcmeta_path).await {
                            reader
                                .read_bytes_async(&mcmeta_path)
                                .await
                                .ok()
                                .and_then(|b| Self::parse_mcmeta_animation(&b, w, h))
                        } else {
                            None
                        };
                        let animation =
                            animation.or_else(|| Self::infer_vertical_strip_animation(w, h));
                        self.block_textures.insert(
                            name.to_string(),
                            BlockTextureEntry {
                                width: w,
                                height: h,
                                rgba,
                                animation,
                            },
                        );
                        return;
                    }
                }
            }
        }
    }

    pub fn load_mc_block_texture(&self, name: &str) -> Option<(u32, u32, Vec<u8>)> {
        self.block_textures
            .get(name)
            .map(|e| (e.width, e.height, e.rgba.clone()))
    }

    pub fn animation_for_mc_texture(&self, name: &str) -> Option<TextureAnimation> {
        self.block_textures
            .get(name)
            .and_then(|e| e.animation.clone())
    }

    pub fn animation_for_stagcrest_texture(&self, namespaced_id: &str) -> Option<TextureAnimation> {
        let mc_name = match namespaced_id {
            "stagcrest:water_still" => "water_still",
            "stagcrest:water_flow" => "water_flow",
            _ => return None,
        };
        self.animation_for_mc_texture(mc_name)
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

/// Minecraft-style vertical animation strip when `.mcmeta` is absent.
pub fn infer_vertical_strip_animation(
    texture_width: u32,
    texture_height: u32,
) -> Option<TextureAnimation> {
    ResourcePackLoader::infer_vertical_strip_animation(texture_width, texture_height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_vertical_strip_mcmeta() {
        let json = r#"{"animation":{"frametime":2,"frames":[0,1,2,3]}}"#;
        let anim = ResourcePackLoader::parse_mcmeta_animation(json.as_bytes(), 16, 64).unwrap();
        assert_eq!(anim.frame_width, 16);
        assert_eq!(anim.frame_height, 16);
        assert_eq!(anim.frame_count, 4);
        assert_eq!(anim.frametime_ticks, 2);
    }
}
