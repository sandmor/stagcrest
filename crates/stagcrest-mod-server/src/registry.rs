use stagcrest_protocol::{
    observer_powered, piston_extended, piston_head_sticky, repeater_powered, torch_lit, AtlasRect,
    BlockDef, BlockFaceTextures, BlockId, BlockState, BlockTextures, FaceTexture, TextureAnimation,
    TextureDef, TextureId, TintKind,
};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct BlockRegistry {
    blocks: HashMap<BlockId, BlockDef>,
    by_namespaced: HashMap<String, BlockId>,
    textures: HashMap<TextureId, TextureDef>,
    texture_pngs: HashMap<TextureId, Vec<u8>>,
    texture_by_name: HashMap<String, TextureId>,
    atlas_uvs: HashMap<TextureId, AtlasRect>,
    atlas_pages: Vec<(u32, u32)>,
    placeable: Vec<BlockId>,
    next_block_id: u32,
    next_texture_id: u32,
}

impl BlockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_texture(
        &mut self,
        namespaced_id: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> TextureId {
        self.register_texture_with_animation(namespaced_id, width, height, rgba, None)
    }

    pub fn register_texture_with_animation(
        &mut self,
        namespaced_id: String,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        animation: Option<TextureAnimation>,
    ) -> TextureId {
        if let Some(&id) = self.texture_by_name.get(&namespaced_id) {
            return id;
        }
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;
        self.textures.insert(
            id,
            TextureDef {
                id,
                namespaced_id: namespaced_id.clone(),
                width,
                height,
                rgba,
                animation,
            },
        );
        self.texture_by_name.insert(namespaced_id, id);
        id
    }

    /// Register a texture from verbatim PNG file bytes (resource pack).
    pub fn register_texture_from_png(
        &mut self,
        namespaced_id: String,
        png: Vec<u8>,
        width: u32,
        height: u32,
        animation: Option<TextureAnimation>,
    ) -> TextureId {
        if let Some(&id) = self.texture_by_name.get(&namespaced_id) {
            return id;
        }
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;
        self.texture_pngs.insert(id, png);
        self.textures.insert(
            id,
            TextureDef {
                id,
                namespaced_id: namespaced_id.clone(),
                width,
                height,
                rgba: Vec::new(),
                animation,
            },
        );
        self.texture_by_name.insert(namespaced_id, id);
        id
    }

    pub fn texture_png(&self, id: TextureId) -> Option<&[u8]> {
        self.texture_pngs.get(&id).map(Vec::as_slice)
    }

    pub fn texture_animation(&self, id: TextureId) -> Option<&TextureAnimation> {
        self.textures.get(&id).and_then(|t| t.animation.as_ref())
    }

    pub fn register_block(&mut self, def: BlockDef) -> BlockId {
        let id = def.id;
        if def.placeable {
            self.placeable.push(id);
        }
        self.by_namespaced.insert(def.namespaced_id.clone(), id);
        self.blocks.insert(id, def);
        id
    }

    pub fn allocate_block_id(&mut self) -> BlockId {
        let id = BlockId(self.next_block_id);
        self.next_block_id += 1;
        id
    }

    pub fn block(&self, id: BlockId) -> Option<&BlockDef> {
        self.blocks.get(&id)
    }

    pub fn block_ids(&self) -> Vec<BlockId> {
        self.blocks.keys().copied().collect()
    }

    pub fn block_by_name(&self, name: &str) -> Option<BlockId> {
        self.by_namespaced.get(name).copied()
    }

    pub fn texture_by_name(&self, name: &str) -> Option<TextureId> {
        self.texture_by_name.get(name).copied()
    }

    pub fn textures(&self) -> impl Iterator<Item = &TextureDef> {
        self.textures.values()
    }

    pub fn set_atlas_uv(&mut self, tex: TextureId, rect: AtlasRect) {
        self.atlas_uvs.insert(tex, rect);
    }

    pub fn set_atlas_pages(&mut self, pages: Vec<(u32, u32)>) {
        self.atlas_pages = pages;
    }

    pub fn atlas_pages(&self) -> &[(u32, u32)] {
        &self.atlas_pages
    }

    pub fn atlas_dimensions(&self, atlas_index: u8) -> (u32, u32) {
        self.atlas_pages
            .get(atlas_index as usize)
            .copied()
            .unwrap_or((1, 1))
    }

    /// Legacy: dimensions of atlas page 0 (or 1x1).
    pub fn primary_atlas_dimensions(&self) -> (u32, u32) {
        self.atlas_dimensions(0)
    }

    pub fn atlas_uv(&self, tex: TextureId) -> AtlasRect {
        self.atlas_uvs.get(&tex).copied().unwrap_or_else(|| {
            tracing::warn!("missing atlas UV for texture {:?}", tex.0);
            AtlasRect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
                atlas_index: 0,
            }
        })
    }

    pub fn build_texture_assets(&self) -> Vec<stagcrest_protocol::manifest::TextureAssetTransfer> {
        use stagcrest_protocol::manifest::TextureAssetTransfer;
        let mut out: Vec<TextureAssetTransfer> = self
            .textures
            .values()
            .map(|tex| {
                let png = self
                    .texture_pngs
                    .get(&tex.id)
                    .cloned()
                    .unwrap_or_else(|| encode_rgba_png(tex.width, tex.height, &tex.rgba));
                TextureAssetTransfer {
                    id: tex.id,
                    png,
                    animation: tex.animation.clone(),
                }
            })
            .collect();
        out.sort_by_key(|t| t.id.0);
        out
    }

    pub fn placeable_blocks(&self) -> &[BlockId] {
        &self.placeable
    }

    pub fn all_blocks(&self) -> impl Iterator<Item = &BlockDef> {
        self.blocks.values()
    }

    pub fn resolve_textures(&self, top: &str, bottom: &str, sides: &str) -> Option<BlockTextures> {
        Some(BlockTextures {
            top: self.texture_by_name(top)?,
            bottom: self.texture_by_name(bottom)?,
            sides: self.texture_by_name(sides)?,
        })
    }

    pub fn resolve_face_textures(
        &self,
        top: &str,
        bottom: &str,
        sides: &str,
    ) -> Option<BlockFaceTextures> {
        let uniform = |name: &str| {
            Some(FaceTexture {
                texture: self.texture_by_name(name)?,
                overlay: None,
                tint: TintKind::None,
                overlay_tint: TintKind::None,
            })
        };
        Some(BlockFaceTextures {
            top: uniform(top)?,
            bottom: uniform(bottom)?,
            sides: uniform(sides)?,
        })
    }

    pub fn block_face_textures_for_state(
        &self,
        id: BlockId,
        state: BlockState,
    ) -> Option<BlockFaceTextures> {
        let def = self.block(id)?;
        if def.namespaced_id == "stagcrest:redstone_torch" {
            if !torch_lit(state) {
                return Some(def.face_textures);
            }
            return self.resolve_face_textures(
                "stagcrest:redstone_torch_on",
                "stagcrest:redstone_torch_on",
                "stagcrest:redstone_torch_on",
            );
        }
        if def.namespaced_id == "stagcrest:repeater" && repeater_powered(state) {
            // Top stays `repeater`: `repeater_on` on the full slab cap reads as a flat
            // red wash. Lit state is shown by the powered model variant + torch sides.
            return self.resolve_face_textures(
                "stagcrest:repeater",
                "stagcrest:smooth_stone",
                "stagcrest:redstone_torch_on",
            );
        }
        if def.namespaced_id == "stagcrest:observer" && observer_powered(state) {
            let output_lit = self
                .texture_by_name("stagcrest:observer_back_on")
                .map(|_| "stagcrest:observer_back_on")
                .unwrap_or("stagcrest:redstone_torch_on");
            return self.resolve_face_textures(
                output_lit,
                "stagcrest:observer_side",
                "stagcrest:observer_front",
            );
        }
        if (def.namespaced_id == "stagcrest:piston"
            || def.namespaced_id == "stagcrest:sticky_piston")
            && piston_extended(state)
        {
            return self.resolve_face_textures(
                "stagcrest:piston_inner",
                "stagcrest:piston_bottom",
                "stagcrest:piston_side",
            );
        }
        if def.namespaced_id == "stagcrest:piston_head" && piston_head_sticky(state) {
            return self.resolve_face_textures(
                "stagcrest:piston_top_sticky",
                "stagcrest:piston_bottom",
                "stagcrest:piston_side",
            );
        }
        if state.0 == 0 {
            return Some(def.face_textures);
        }
        Some(def.face_textures)
    }

    pub fn tint_for_kind(kind: TintKind) -> f32 {
        kind.as_f32()
    }

    pub fn to_wire_snapshot(&self) -> stagcrest_protocol::RegistryWireSnapshot {
        use stagcrest_protocol::manifest::TextureWireDef;
        stagcrest_protocol::RegistryWireSnapshot {
            blocks: self.blocks.values().cloned().collect(),
            textures: self
                .textures
                .values()
                .map(|tex| TextureWireDef {
                    id: tex.id,
                    namespaced_id: tex.namespaced_id.clone(),
                    width: tex.width,
                    height: tex.height,
                    animation: tex.animation.clone(),
                })
                .collect(),
            placeable: self.placeable.clone(),
            next_block_id: self.next_block_id,
            next_texture_id: self.next_texture_id,
        }
    }

    pub fn to_snapshot(&self) -> stagcrest_protocol::RegistrySnapshot {
        stagcrest_protocol::RegistrySnapshot {
            blocks: self.blocks.values().cloned().collect(),
            textures: self.textures.values().cloned().collect(),
            placeable: self.placeable.clone(),
            atlas_uvs: self
                .atlas_uvs
                .iter()
                .map(|(&id, &rect)| (id, rect))
                .collect(),
            atlas_pages: self.atlas_pages.clone(),
            next_block_id: self.next_block_id,
            next_texture_id: self.next_texture_id,
        }
    }

    pub fn from_snapshot(snap: stagcrest_protocol::RegistrySnapshot) -> Self {
        let mut registry = Self {
            next_block_id: snap.next_block_id,
            next_texture_id: snap.next_texture_id,
            atlas_pages: snap.atlas_pages,
            ..Default::default()
        };
        for tex in snap.textures {
            registry
                .texture_by_name
                .insert(tex.namespaced_id.clone(), tex.id);
            registry.textures.insert(tex.id, tex);
        }
        for def in snap.blocks {
            registry
                .by_namespaced
                .insert(def.namespaced_id.clone(), def.id);
            registry.blocks.insert(def.id, def);
        }
        for (id, rect) in snap.atlas_uvs {
            registry.atlas_uvs.insert(id, rect);
        }
        registry.placeable = snap.placeable;
        registry
    }
}

fn encode_rgba_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use image::{ImageBuffer, Rgba};
    use std::io::Cursor;
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(width, height, rgba.to_vec())
        .unwrap_or_else(|| {
            ImageBuffer::from_pixel(width.max(1), height.max(1), Rgba([0, 0, 0, 255]))
        });
    let mut png = Vec::new();
    let _ = img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png);
    png
}
