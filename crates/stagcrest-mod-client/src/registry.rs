use stagcrest_protocol::{
    lamp_lit, observer_powered, piston_extended, piston_head_sticky, repeater_powered, torch_lit,
    AtlasRect, BehaviorRef, BlockDef, BlockFaceTextures, BlockId, BlockState, BlockTextures,
    FaceTexture, NativeBehaviorId, TextureAnimation, TextureDef, TextureId, TintKind,
};
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct BlockRegistry {
    blocks: HashMap<BlockId, BlockDef>,
    by_namespaced: HashMap<String, BlockId>,
    textures: HashMap<TextureId, TextureDef>,
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

    pub fn primary_atlas_dimensions(&self) -> (u32, u32) {
        self.atlas_dimensions(0)
    }

    pub fn apply_atlas_set(&mut self, set: &stagcrest_atlas::AtlasSet) {
        self.atlas_pages = set
            .pages
            .iter()
            .map(|page| (page.width, page.height))
            .collect();
        self.atlas_uvs.clear();
        for placement in &set.placements {
            self.atlas_uvs.insert(placement.id, placement.rect);
        }
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
        match def.behavior {
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::RedstoneInverter { .. },
            }) if torch_lit(state) => self.resolve_face_textures(
                "stagcrest:redstone_torch_on",
                "stagcrest:redstone_torch_on",
                "stagcrest:redstone_torch_on",
            ),
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::RedstoneInverter { .. },
            }) if !torch_lit(state) => Some(def.face_textures),
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::RedstoneLamp,
            }) if lamp_lit(state) => self.resolve_face_textures(
                "stagcrest:redstone_lamp_on",
                "stagcrest:redstone_lamp_on",
                "stagcrest:redstone_lamp_on",
            ),
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::RedstoneRepeater { .. },
            }) if repeater_powered(state) => self.resolve_face_textures(
                "stagcrest:repeater",
                "stagcrest:smooth_stone",
                "stagcrest:redstone_torch_on",
            ),
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::RedstoneObserver { .. },
            }) if observer_powered(state) => {
                let output_lit = self
                    .texture_by_name("stagcrest:observer_back_on")
                    .map(|_| "stagcrest:observer_back_on")
                    .unwrap_or("stagcrest:redstone_torch_on");
                self.resolve_face_textures(
                    output_lit,
                    "stagcrest:observer_side",
                    "stagcrest:observer_front",
                )
            }
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::RedstonePiston { .. },
            }) if piston_extended(state) => self.resolve_face_textures(
                "stagcrest:piston_inner",
                "stagcrest:piston_bottom",
                "stagcrest:piston_side",
            ),
            Some(BehaviorRef::Native {
                id: NativeBehaviorId::PistonHead,
            }) if piston_head_sticky(state) => self.resolve_face_textures(
                "stagcrest:piston_top_sticky",
                "stagcrest:piston_bottom",
                "stagcrest:piston_side",
            ),
            _ => Some(def.face_textures),
        }
    }

    pub fn tint_for_kind(kind: TintKind) -> f32 {
        kind.as_f32()
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

    pub fn from_wire_snapshot(snap: stagcrest_protocol::RegistryWireSnapshot) -> Self {
        use stagcrest_protocol::{manifest::TextureWireDef, TextureDef};
        let mut registry = Self {
            next_block_id: snap.next_block_id,
            next_texture_id: snap.next_texture_id,
            ..Default::default()
        };
        for tex in snap.textures {
            let TextureWireDef {
                id,
                namespaced_id,
                width,
                height,
                animation,
            } = tex;
            registry.texture_by_name.insert(namespaced_id.clone(), id);
            registry.textures.insert(
                id,
                TextureDef {
                    id,
                    namespaced_id,
                    width,
                    height,
                    rgba: Vec::new(),
                    animation,
                },
            );
        }
        for def in snap.blocks {
            let mut def = def;
            crate::block_tints::apply_block_face_tints(
                &def.namespaced_id,
                def.fluid,
                &mut def.face_textures,
                &registry,
            );
            registry
                .by_namespaced
                .insert(def.namespaced_id.clone(), def.id);
            registry.blocks.insert(def.id, def);
        }
        registry.placeable = snap.placeable;
        registry
    }
}
