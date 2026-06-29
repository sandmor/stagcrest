use stagcrest_circuit::CircuitWorld;
use stagcrest_mod_server::BlockRegistry;
use stagcrest_protocol::{
    torch_state, BlockDef, BlockFaceTextures, BlockGeometry, BlockId, BlockPos, BlockState,
    CircuitKind, CircuitNodeDef, ModelId, ModelRenderLayer, TextureId, TorchAttachment,
};
use stagcrest_world::World;

pub struct TestBlocks {
    pub source: BlockId,
    pub wire: BlockId,
    pub torch: BlockId,
    pub switch: BlockId,
    pub repeater: BlockId,
    pub stone: BlockId,
    pub button: BlockId,
}

pub fn setup_registry() -> (BlockRegistry, TestBlocks) {
    let mut reg = BlockRegistry::new();
    let blocks = TestBlocks {
        source: BlockId(1),
        wire: BlockId(2),
        torch: BlockId(3),
        switch: BlockId(4),
        repeater: BlockId(6),
        stone: BlockId(7),
        button: BlockId(8),
    };

    reg.register_block(test_block(blocks.source, CircuitKind::Source { level: 15 }));
    reg.register_block(test_block(blocks.wire, CircuitKind::Wire { falloff: 1 }));
    reg.register_block(test_block(blocks.switch, CircuitKind::Switch { output: 15 }));
    reg.register_block(test_block_with_geometry(
        blocks.repeater,
        CircuitKind::Repeater { output: 15 },
        BlockGeometry::Model(ModelId::Repeater),
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
    });

    (reg, blocks)
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
    let lever_pos = BlockPos::new(0, 0, 0);
    let wire_pos = BlockPos::new(1, 0, 0);
    let block_pos = BlockPos::new(2, 0, 0);
    let torch_pos = BlockPos::new(3, 0, 0);

    world.set_block(
        lever_pos,
        blocks.switch,
        BlockState(u16::from(lever_on)),
    );
    world.set_block(wire_pos, blocks.wire, BlockState(0));
    world.set_block(block_pos, blocks.stone, BlockState(0));
    world.set_block(
        torch_pos,
        blocks.torch,
        torch_state(false, TorchAttachment::WallEast),
    );
    populate_chunks(
        world,
        &[lever_pos, wire_pos, block_pos, torch_pos],
    );

    (torch_pos, lever_pos)
}
