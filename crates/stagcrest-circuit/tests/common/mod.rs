use stagcrest_circuit::CircuitWorld;
use stagcrest_mod_server::BlockRegistry;
use stagcrest_protocol::{
    mount_state, torch_state, AttachFace, BlockDef, BlockFaceTextures, BlockGeometry, BlockId,
    BlockPos, BlockState, CircuitKind, CircuitNodeDef, Facing, ModelId, ModelRenderLayer,
    PushReaction, TextureId, TorchAttachment,
};
use stagcrest_world::World;

pub struct TestBlocks {
    pub source: BlockId,
    pub wire: BlockId,
    pub torch: BlockId,
    pub switch: BlockId,
    pub repeater: BlockId,
    pub observer: BlockId,
    pub stone: BlockId,
    pub button: BlockId,
    pub piston: BlockId,
    pub sticky_piston: BlockId,
    pub piston_head: BlockId,
    pub slime: BlockId,
    pub honey: BlockId,
    pub glass: BlockId,
    pub bedrock: BlockId,
}

pub fn setup_registry() -> (BlockRegistry, TestBlocks) {
    let mut reg = BlockRegistry::new();
    let blocks = TestBlocks {
        source: BlockId(1),
        wire: BlockId(2),
        torch: BlockId(3),
        switch: BlockId(4),
        repeater: BlockId(6),
        observer: BlockId(9),
        stone: BlockId(7),
        button: BlockId(8),
        piston: BlockId(10),
        sticky_piston: BlockId(11),
        piston_head: BlockId(12),
        slime: BlockId(13),
        honey: BlockId(14),
        glass: BlockId(16),
        bedrock: BlockId(15),
    };

    reg.register_block(test_block(blocks.source, CircuitKind::Source { level: 15 }));
    reg.register_block(test_block(blocks.wire, CircuitKind::Wire { falloff: 1 }));
    reg.register_block(test_block(
        blocks.switch,
        CircuitKind::Switch { output: 15 },
    ));
    reg.register_block(test_block_with_geometry(
        blocks.repeater,
        CircuitKind::Repeater { output: 15 },
        BlockGeometry::Model(ModelId::Repeater),
    ));
    reg.register_block(test_block_with_geometry(
        blocks.observer,
        CircuitKind::Observer { output: 15 },
        BlockGeometry::Model(ModelId::Observer),
    ));
    reg.register_block(test_block_with_geometry(
        blocks.torch,
        CircuitKind::Inverter { output: 15 },
        BlockGeometry::Model(ModelId::RedstoneTorch),
    ));
    reg.register_block(test_block_with_geometry(
        blocks.button,
        CircuitKind::Switch { output: 15 },
        BlockGeometry::Model(ModelId::Button),
    ));
    reg.register_block(test_piston_block(
        blocks.piston,
        CircuitKind::Piston { sticky: false },
        "test:piston",
    ));
    reg.register_block(test_piston_block(
        blocks.sticky_piston,
        CircuitKind::Piston { sticky: true },
        "test:sticky_piston",
    ));
    reg.register_block(BlockDef {
        id: blocks.piston_head,
        namespaced_id: "test:piston_head".into(),
        display_name: "Piston Head".into(),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 1.0,
        face_textures: BlockFaceTextures::uniform(TextureId(0)),
        circuit: None,
        placeable: false,
        geometry: BlockGeometry::Model(ModelId::PistonHead),
        fluid: false,
        render_layer: ModelRenderLayer::Opaque,
        push_reaction: PushReaction::Normal,
        map_color: [128, 128, 128],
        redstone_powerable: false,
    });
    reg.register_block(transparent_conductor_block(
        blocks.slime,
        "test:slime_block",
        PushReaction::Normal,
    ));
    reg.register_block(transparent_conductor_block(
        blocks.honey,
        "test:honey_block",
        PushReaction::Normal,
    ));
    reg.register_block(transparent_insulator_block(blocks.glass, "test:glass"));
    reg.register_block(cube_block(
        blocks.bedrock,
        "test:bedrock",
        PushReaction::Block,
    ));
    reg.register_block(BlockDef {
        id: blocks.stone,
        namespaced_id: "test:stone".into(),
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
        map_color: [128, 128, 128],
        redstone_powerable: true,
    });

    (reg, blocks)
}

fn test_piston_block(id: BlockId, kind: CircuitKind, name: &str) -> BlockDef {
    BlockDef {
        id,
        namespaced_id: name.into(),
        display_name: "Piston".into(),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 1.0,
        face_textures: BlockFaceTextures::uniform(TextureId(0)),
        circuit: Some(CircuitNodeDef { kind }),
        placeable: true,
        geometry: BlockGeometry::Model(ModelId::Piston),
        fluid: false,
        render_layer: ModelRenderLayer::Opaque,
        push_reaction: PushReaction::Normal,
        map_color: [128, 128, 128],
        redstone_powerable: false,
    }
}

fn cube_block(id: BlockId, name: &str, push_reaction: PushReaction) -> BlockDef {
    BlockDef {
        id,
        namespaced_id: name.into(),
        display_name: name.into(),
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
        push_reaction,
        map_color: [128, 128, 128],
        redstone_powerable: true,
    }
}

fn transparent_insulator_block(id: BlockId, name: &str) -> BlockDef {
    BlockDef {
        id,
        namespaced_id: name.into(),
        display_name: name.into(),
        opaque: false,
        transparent: true,
        solid: true,
        hardness: 1.0,
        face_textures: BlockFaceTextures::uniform(TextureId(0)),
        circuit: None,
        placeable: true,
        geometry: BlockGeometry::Cube,
        fluid: false,
        render_layer: ModelRenderLayer::Blend,
        push_reaction: PushReaction::Normal,
        map_color: [128, 128, 128],
        redstone_powerable: false,
    }
}

fn transparent_conductor_block(
    id: BlockId,
    name: &str,
    push_reaction: PushReaction,
) -> BlockDef {
    BlockDef {
        id,
        namespaced_id: name.into(),
        display_name: name.into(),
        opaque: true,
        transparent: true,
        solid: true,
        hardness: 1.0,
        face_textures: BlockFaceTextures::uniform(TextureId(0)),
        circuit: None,
        placeable: true,
        geometry: BlockGeometry::Cube,
        fluid: false,
        render_layer: ModelRenderLayer::Blend,
        push_reaction,
        map_color: [128, 128, 128],
        redstone_powerable: true,
    }
}

fn test_block(id: BlockId, kind: CircuitKind) -> BlockDef {
    test_block_with_geometry(id, kind, BlockGeometry::Cube)
}

fn test_block_with_geometry(id: BlockId, kind: CircuitKind, geometry: BlockGeometry) -> BlockDef {
    BlockDef {
        id,
        namespaced_id: format!("test:{id:?}"),
        display_name: "Test".into(),
        opaque: true,
        transparent: false,
        solid: true,
        hardness: 1.0,
        face_textures: BlockFaceTextures::uniform(TextureId(0)),
        circuit: Some(CircuitNodeDef { kind }),
        placeable: true,
        geometry,
        fluid: false,
        render_layer: ModelRenderLayer::Opaque,
        push_reaction: stagcrest_protocol::PushReaction::Normal,
        map_color: [128, 128, 128],
        redstone_powerable: false,
    }
}

pub fn settle(circuit: &mut CircuitWorld, world: &mut World, reg: &BlockRegistry, ticks: u64) {
    for _ in 0..ticks {
        circuit.tick(world, reg);
    }
}

pub fn populate_chunks(world: &mut World, blocks: &[BlockPos]) {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    for pos in blocks {
        let cpos = pos.chunk_pos();
        if seen.insert(cpos) {
            world.finalize_generated_chunk(cpos);
        }
    }
}

pub fn place_wall_torch_not_gate(
    world: &mut World,
    blocks: &TestBlocks,
    lever_on: bool,
) -> (BlockPos, BlockPos) {
    let lever_pos = BlockPos::new(1, 0, 0);
    let block_pos = BlockPos::new(2, 0, 0);
    let torch_pos = BlockPos::new(3, 0, 0);

    world.set_block(
        lever_pos,
        blocks.switch,
        mount_state(lever_on, AttachFace::Wall, Facing::West),
    );
    world.set_block(block_pos, blocks.stone, BlockState(0));
    world.set_block(
        torch_pos,
        blocks.torch,
        torch_state(false, TorchAttachment::WallEast),
    );
    populate_chunks(world, &[lever_pos, block_pos, torch_pos]);

    (torch_pos, lever_pos)
}
