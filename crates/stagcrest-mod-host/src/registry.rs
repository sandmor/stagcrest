use stagcrest_protocol::{
    AtlasRect, BlockDef, BlockFaceTextures, BlockId, BlockState, BlockTextures, FaceTexture,
    TextureDef, TextureId, TintKind, torch_lit,
};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct BlockRegistry {
    blocks: HashMap<BlockId, BlockDef>,
    by_namespaced: HashMap<String, BlockId>,
    textures: HashMap<TextureId, TextureDef>,
    texture_by_name: HashMap<String, TextureId>,
    atlas_uvs: HashMap<TextureId, AtlasRect>,
    atlas_width: u32,
    atlas_height: u32,
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
            },
        );
        self.texture_by_name.insert(namespaced_id, id);
        id
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

    pub fn set_atlas_dimensions(&mut self, width: u32, height: u32) {
        self.atlas_width = width;
        self.atlas_height = height;
    }

    pub fn atlas_dimensions(&self) -> (u32, u32) {
        (self.atlas_width.max(1), self.atlas_height.max(1))
    }

    pub fn atlas_uv(&self, tex: TextureId) -> AtlasRect {
        self.atlas_uvs.get(&tex).copied().unwrap_or_else(|| {
            tracing::warn!("missing atlas UV for texture {:?}", tex.0);
            AtlasRect {
                x: 0,
                y: 0,
                w: 1,
                h: 1,
            }
        })
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
        Some(BlockFaceTextures {
            top: FaceTexture::uniform(self.texture_by_name(top)?),
            bottom: FaceTexture::uniform(self.texture_by_name(bottom)?),
            sides: FaceTexture::uniform(self.texture_by_name(sides)?),
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
        if state.0 == 0 {
            return Some(def.face_textures);
        }
        Some(def.face_textures)
    }

    pub fn tint_for_kind(kind: TintKind) -> f32 {
        kind.as_f32()
    }
}
