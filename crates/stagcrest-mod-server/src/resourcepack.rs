use crate::assets::{AssetError, AssetReader, FsAssetReader};
use stagcrest_content::ContentSettings;
use stagcrest_protocol::TextureAnimation;
use stagcrest_storage::DATA_DIR;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    "redstone_dust_line1",
    "redstone_dust_overlay",
    "redstone_torch_off",
    "redstone_torch",
    "redstone_block",
    "lever",
    "repeater",
    "repeater_on",
    "observer_front",
    "observer_back",
    "observer_side",
    "observer_top",
    "observer_back_on",
    "piston_top",
    "piston_top_sticky",
    "piston_side",
    "piston_bottom",
    "piston_inner",
    "slime_block",
    "honey_block_top",
    "honey_block_side",
    "honey_block_bottom",
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

#[derive(Debug, Clone)]
struct BlockTextureEntry {
    width: u32,
    height: u32,
    png: Vec<u8>,
    animation: Option<TextureAnimation>,
}

#[derive(Debug, serde::Deserialize)]
struct McMetaRoot {
    animation: Option<McMetaAnimation>,
}

#[derive(Debug, serde::Deserialize)]
struct McMetaAnimation {
    #[serde(default)]
    frametime: u32,
    #[serde(default)]
    frames: Vec<McMetaFrame>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum McMetaFrame {
    Index(u32),
    Object { index: u32 },
}

pub struct ResourcePackLoader {
    repo_root: PathBuf,
    pack_roots: Vec<String>,
    block_textures: RefCell<HashMap<String, BlockTextureEntry>>,
}

impl ResourcePackLoader {
    pub fn repo_root(&self) -> &std::path::Path {
        &self.repo_root
    }

    pub fn load(
        repo_root: impl AsRef<Path>,
        _reader: &dyn AssetReader,
    ) -> Result<Self, AssetError> {
        let pack_roots = Self::read_pack_roots(repo_root.as_ref())?;
        Ok(Self {
            repo_root: repo_root.as_ref().to_path_buf(),
            pack_roots,
            block_textures: RefCell::new(HashMap::new()),
        })
    }

    fn read_pack_roots(repo_root: &Path) -> Result<Vec<String>, AssetError> {
        let data_dir = repo_root.join(DATA_DIR);
        let settings = match ContentSettings::load(&data_dir) {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!("failed to load content settings, using no resource packs: {e}");
                return Ok(Vec::new());
            }
        };
        let pack_roots = settings.enabled_pack_asset_paths();
        for path in &pack_roots {
            tracing::info!("resource pack enabled: {path}");
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

    pub fn warm_block_textures(&self, reader: &dyn AssetReader, names: &[&str]) {
        for name in names {
            self.ensure_block_texture(reader, name);
        }
        if !self.pack_roots.is_empty() && self.block_textures.borrow().is_empty() {
            tracing::warn!(
                "resource pack enabled but no block textures loaded; \
                 expected assets/minecraft/textures/block/ or minecraft/textures/block/"
            );
        }
    }

    pub fn ensure_block_texture(&self, reader: &dyn AssetReader, name: &str) {
        let _ = self.load_mc_block_texture_with_reader(reader, name);
    }

    fn texture_filename(name: &str) -> String {
        if name.ends_with(".png") {
            name.to_string()
        } else {
            format!("{name}.png")
        }
    }

    /// Standard `assets/minecraft/...` layout, then legacy root `minecraft/...` (Faithful zip).
    fn block_texture_paths(pack_root: &str, mc_name: &str) -> [String; 2] {
        let filename = Self::texture_filename(mc_name);
        [
            format!("{pack_root}/assets/minecraft/textures/block/{filename}"),
            format!("{pack_root}/minecraft/textures/block/{filename}"),
        ]
    }

    fn colormap_paths(pack_root: &str, name: &str) -> [String; 2] {
        let filename = Self::texture_filename(name);
        [
            format!("{pack_root}/assets/minecraft/textures/colormap/{filename}"),
            format!("{pack_root}/minecraft/textures/colormap/{filename}"),
        ]
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

    /// Minecraft-style vertical animation strip when `.mcmeta` is absent (fluids only).
    pub fn infer_vertical_strip_animation(
        mc_name: &str,
        texture_width: u32,
        texture_height: u32,
    ) -> Option<TextureAnimation> {
        if mc_name != "water_still" && mc_name != "water_flow" {
            return None;
        }
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

    fn try_load_block_texture(&self, reader: &dyn AssetReader, name: &str) {
        if self.block_textures.borrow().contains_key(name) {
            return;
        }
        for pack in &self.pack_roots {
            for path in Self::block_texture_paths(pack, name) {
                if !reader.exists(&path) {
                    continue;
                }
                let Ok(bytes) = reader.read_bytes(&path) else {
                    continue;
                };
                let Some((w, h, _)) = Self::load_rgba_from_bytes(&bytes) else {
                    continue;
                };
                let mcmeta_path = format!("{path}.mcmeta");
                let animation = if reader.exists(&mcmeta_path) {
                    reader
                        .read_bytes(&mcmeta_path)
                        .ok()
                        .and_then(|b| Self::parse_mcmeta_animation(&b, w, h))
                } else {
                    None
                };
                let animation =
                    animation.or_else(|| Self::infer_vertical_strip_animation(name, w, h));
                self.block_textures.borrow_mut().insert(
                    name.to_string(),
                    BlockTextureEntry {
                        width: w,
                        height: h,
                        png: bytes,
                        animation,
                    },
                );
                return;
            }
        }
    }

    pub fn load_mc_block_texture_png(
        &self,
        reader: &dyn AssetReader,
        name: &str,
    ) -> Option<(u32, u32, Vec<u8>, Option<TextureAnimation>)> {
        if let Some(e) = self.block_textures.borrow().get(name) {
            return Some((e.width, e.height, e.png.clone(), e.animation.clone()));
        }
        self.try_load_block_texture(reader, name);
        self.block_textures
            .borrow()
            .get(name)
            .map(|e| (e.width, e.height, e.png.clone(), e.animation.clone()))
    }

    fn load_mc_block_texture_with_reader(
        &self,
        reader: &dyn AssetReader,
        name: &str,
    ) -> Option<(u32, u32, Vec<u8>)> {
        self.load_mc_block_texture_png(reader, name)
            .map(|(w, h, png, _)| (w, h, png))
    }

    pub fn load_mc_block_texture(&self, name: &str) -> Option<(u32, u32, Vec<u8>)> {
        let reader = FsAssetReader::new(&self.repo_root);
        self.load_mc_block_texture_with_reader(&reader, name)
    }

    pub fn load_mc_block_texture_png_for_transfer(
        &self,
        name: &str,
    ) -> Option<(u32, u32, Vec<u8>, Option<TextureAnimation>)> {
        let reader = FsAssetReader::new(&self.repo_root);
        self.load_mc_block_texture_png(&reader, name)
    }

    pub fn animation_for_mc_texture(&self, name: &str) -> Option<TextureAnimation> {
        let _ = self.load_mc_block_texture(name);
        self.block_textures
            .borrow()
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
            for path in Self::colormap_paths(pack, name) {
                if reader.exists(&path) {
                    let bytes = reader.read_bytes(&path).ok()?;
                    return Self::load_rgba_from_bytes(&bytes);
                }
            }
        }
        None
    }
}

/// Minecraft-style vertical animation strip when `.mcmeta` is absent (fluids only).
pub fn infer_vertical_strip_animation(
    mc_name: &str,
    texture_width: u32,
    texture_height: u32,
) -> Option<TextureAnimation> {
    ResourcePackLoader::infer_vertical_strip_animation(mc_name, texture_width, texture_height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::FsAssetReader;
    use image::{ImageBuffer, Rgba};
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn parse_vertical_strip_mcmeta() {
        let json = r#"{"animation":{"frametime":2,"frames":[0,1,2,3]}}"#;
        let anim = ResourcePackLoader::parse_mcmeta_animation(json.as_bytes(), 16, 64).unwrap();
        assert_eq!(anim.frame_width, 16);
        assert_eq!(anim.frame_height, 16);
        assert_eq!(anim.frame_count, 4);
        assert_eq!(anim.frametime_ticks, 2);
    }

    #[test]
    fn loads_block_texture_from_minecraft_root_layout() {
        let dir = TempDir::new().unwrap();
        let pack_dir = dir.path().join("data/resourcepacks/test-pack");
        let block_dir = pack_dir.join("minecraft/textures/block");
        std::fs::create_dir_all(&block_dir).unwrap();
        std::fs::write(
            pack_dir.join("pack.mcmeta"),
            r#"{"pack":{"pack_format":15}}"#,
        )
        .unwrap();
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(32, 32, Rgba([40, 40, 40, 255]));
        img.save(block_dir.join("stone.png")).unwrap();

        let settings = r#"
[content]
resource_pack_order = ["test-pack"]

[[content.resource_packs]]
id = "test-pack"
path = "test-pack"
enabled = true
source = "local"
"#;
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        std::fs::File::create(dir.path().join("data/settings.toml"))
            .unwrap()
            .write_all(settings.as_bytes())
            .unwrap();

        let reader = FsAssetReader::new(dir.path());
        let loader = ResourcePackLoader::load(dir.path(), &reader).unwrap();
        let (w, h, png) = loader
            .load_mc_block_texture_with_reader(&reader, "stone")
            .unwrap();
        assert_eq!((w, h), (32, 32));
        assert!(!png.is_empty());
    }
}
