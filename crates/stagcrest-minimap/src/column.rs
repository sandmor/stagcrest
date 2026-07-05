use stagcrest_protocol::{BlockDef, BlockId, BlockState};

use crate::color::{is_map_visible, is_surface_plant};

pub trait ColumnBlockSource {
    fn block_at(&self, wx: i32, wy: i32, wz: i32) -> (BlockId, BlockState);
}

pub fn surface_block_y(
    source: &impl ColumnBlockSource,
    blocks: &[BlockDef],
    wx: i32,
    wz: i32,
    air: BlockId,
    y_min: i32,
    y_max: i32,
) -> Option<(BlockId, BlockState, i32)> {
    for y in (y_min..=y_max).rev() {
        let (id, state) = source.block_at(wx, y, wz);
        let def = blocks.iter().find(|d| d.id == id);
        if let Some(def) = def {
            if is_surface_plant(def) {
                continue;
            }
            if is_map_visible(def, air, id) {
                return Some((id, state, y));
            }
        } else if id != air {
            return Some((id, state, y));
        }
    }
    None
}

pub fn surface_block(
    source: &impl ColumnBlockSource,
    blocks: &[BlockDef],
    wx: i32,
    wz: i32,
    air: BlockId,
    y_min: i32,
    y_max: i32,
) -> Option<(BlockId, BlockState)> {
    surface_block_y(source, blocks, wx, wz, air, y_min, y_max).map(|(id, state, _)| (id, state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::{BlockFaceTextures, BlockGeometry, ModelRenderLayer, TextureId};

    struct Column {
        blocks: Vec<(i32, BlockId)>,
        air: BlockId,
    }

    impl ColumnBlockSource for Column {
        fn block_at(&self, _wx: i32, wy: i32, _wz: i32) -> (BlockId, BlockState) {
            for (y, id) in &self.blocks {
                if *y == wy {
                    return (*id, BlockState(0));
                }
            }
            (self.air, BlockState(0))
        }
    }

    fn defs() -> Vec<BlockDef> {
        let air = BlockId(0);
        let stone = BlockId(1);
        let grass_plant = BlockId(2);
        vec![
            BlockDef {
                id: air,
                namespaced_id: "stagcrest:air".into(),
                display_name: "Air".into(),
                opaque: false,
                transparent: true,
                solid: false,
                hardness: 0.0,
                face_textures: BlockFaceTextures::uniform(TextureId(0)),
                circuit: None,
                placeable: false,
                geometry: BlockGeometry::Cube,
                fluid: false,
                render_layer: ModelRenderLayer::Opaque,
                push_reaction: stagcrest_protocol::PushReaction::Normal,
                map_color: [0, 0, 0],
                redstone_powerable: false,
                light_emission: 0,
                light_emission_when_lit: false,
                light_attenuation: 0,
                blocks_sky_light: None,
            },
            BlockDef {
                id: stone,
                namespaced_id: "stagcrest:stone".into(),
                display_name: "Stone".into(),
                opaque: true,
                transparent: false,
                solid: true,
                hardness: 1.0,
                face_textures: BlockFaceTextures::uniform(TextureId(0)),
                circuit: None,
                placeable: true,
                geometry: BlockGeometry::Cube,
                fluid: false,
                render_layer: ModelRenderLayer::Opaque,
                push_reaction: stagcrest_protocol::PushReaction::Normal,
                map_color: [120, 120, 120],
                redstone_powerable: true,
                light_emission: 0,
                light_emission_when_lit: false,
                light_attenuation: 0,
                blocks_sky_light: None,
            },
            BlockDef {
                id: grass_plant,
                namespaced_id: "stagcrest:short_grass".into(),
                display_name: "Grass".into(),
                opaque: false,
                transparent: true,
                solid: false,
                hardness: 0.0,
                face_textures: BlockFaceTextures::uniform(TextureId(0)),
                circuit: None,
                placeable: false,
                geometry: BlockGeometry::Cross,
                fluid: false,
                render_layer: ModelRenderLayer::Cutout,
                push_reaction: stagcrest_protocol::PushReaction::Normal,
                map_color: [95, 159, 53],
                redstone_powerable: false,
                light_emission: 0,
                light_emission_when_lit: false,
                light_attenuation: 0,
                blocks_sky_light: None,
            },
        ]
    }

    #[test]
    fn skips_cross_plant_to_stone_below() {
        let air = BlockId(0);
        let stone = BlockId(1);
        let plant = BlockId(2);
        let col = Column {
            blocks: vec![(64, plant), (63, stone)],
            air,
        };
        let defs = defs();
        let hit = surface_block(&col, &defs, 0, 0, air, 0, 80).unwrap();
        assert_eq!(hit.0, stone);
    }
}
