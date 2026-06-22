use crate::registry::BlockRegistry;
use crate::worldgen::biome::{BiomeRegistry, RegisterBiomeRequest};
use crate::worldgen::terrain::ColumnBlocks;
use stagcrest_protocol::{BlockDef, BlockFaceTextures, BlockGeometry, BlockId, TextureId};

pub fn test_biomes(registry: &mut BlockRegistry) -> BiomeRegistry {
    let mut biomes = BiomeRegistry::default();
    biomes.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:plains".into(),
        temperature: 0.8,
        downfall: 0.4,
        surface_top: "stagcrest:grass_block".into(),
        surface_under: "stagcrest:dirt".into(),
        surface_depth: 3,
        underwater_top: Some("stagcrest:sand".into()),
    });
    biomes.register_biome(RegisterBiomeRequest {
        namespaced_id: "stagcrest:desert".into(),
        temperature: 2.0,
        downfall: 0.0,
        surface_top: "stagcrest:sand".into(),
        surface_under: "stagcrest:sand".into(),
        surface_depth: 4,
        underwater_top: Some("stagcrest:sand".into()),
    });
    biomes.finalize(registry).unwrap();
    biomes
}

pub fn test_blocks() -> ColumnBlocks {
    ColumnBlocks {
        bedrock: BlockId(1),
        stone: BlockId(2),
        dirt: BlockId(3),
        grass: BlockId(4),
        sand: BlockId(6),
        iron_ore: BlockId(7),
        oak_log: BlockId(8),
        oak_leaves: BlockId(15),
        short_grass: BlockId(9),
        tall_grass: BlockId(10),
        dandelion: BlockId(11),
        poppy: BlockId(12),
        cactus: BlockId(13),
        dead_bush: BlockId(14),
        water: BlockId(5),
        air: BlockId(0),
    }
}

pub fn test_registry() -> BlockRegistry {
    let mut reg = BlockRegistry::new();
    let tex = TextureId(0);
    reg.register_texture("stagcrest:stone".into(), 16, 16, vec![0; 16 * 16 * 4]);
    let face = BlockFaceTextures::uniform(tex);

    for (name, id, geometry) in [
        ("stagcrest:air", 0u32, BlockGeometry::Cube),
        ("stagcrest:bedrock", 1, BlockGeometry::Cube),
        ("stagcrest:stone", 2, BlockGeometry::Cube),
        ("stagcrest:dirt", 3, BlockGeometry::Cube),
        ("stagcrest:grass_block", 4, BlockGeometry::Cube),
        ("stagcrest:water", 5, BlockGeometry::Cube),
        ("stagcrest:sand", 6, BlockGeometry::Cube),
        ("stagcrest:iron_ore", 7, BlockGeometry::Cube),
        ("stagcrest:oak_log", 8, BlockGeometry::Cube),
        ("stagcrest:short_grass", 9, BlockGeometry::Cross),
        ("stagcrest:tall_grass", 10, BlockGeometry::Cross),
        ("stagcrest:dandelion", 11, BlockGeometry::Cross),
        ("stagcrest:poppy", 12, BlockGeometry::Cross),
        ("stagcrest:cactus", 13, BlockGeometry::Cube),
        ("stagcrest:dead_bush", 14, BlockGeometry::Cross),
        ("stagcrest:oak_leaves", 15, BlockGeometry::Cube),
    ] {
        let opaque = id != 0 && id != 5 && (id < 9 || id == 13);
        let transparent = id == 5 || (id >= 9 && id <= 12) || id == 14 || id == 15;
        let solid = id != 0 && id != 5 && id < 9 || id == 13;
        reg.register_block(BlockDef {
            id: BlockId(id),
            namespaced_id: name.into(),
            display_name: name.into(),
            opaque,
            transparent,
            solid,
            hardness: 1.0,
            face_textures: face,
            circuit: None,
            placeable: false,
            geometry,
            fluid: id == 5,
            render_layer: if id == 5 {
                stagcrest_protocol::ModelRenderLayer::Blend
            } else if transparent {
                stagcrest_protocol::ModelRenderLayer::Cutout
            } else {
                stagcrest_protocol::ModelRenderLayer::Opaque
            },
        });
    }
    reg
}
