use std::collections::HashMap;

use crate::{
    validate_mount_placement, validate_observer_placement, validate_piston_placement,
    validate_repeater_placement, validate_torch_placement, BlockRegistry,
};
use stagcrest_protocol::{
    BehaviorRef, BlockDef, BlockId, BlockPos, BlockState, NativeBehaviorId,
};
use stagcrest_world::World;

use super::native::{
    break_positions_for, default_push_reaction, dynamic_light_for, redstone_powerable_for,
};
use super::BlockBehavior;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorResult {
    Ok,
    Cancel,
    SetState(BlockState),
    SetBlock(BlockId, BlockState),
}

pub struct BehaviorCtx<'a> {
    pub pos: BlockPos,
    pub block_id: BlockId,
    pub state: BlockState,
    pub neighbor: Option<BlockPos>,
    pub world: &'a mut World,
    pub registry: &'a BlockRegistry,
    pub air: BlockId,
    pub face_normal: Option<[i32; 3]>,
    pub player_yaw_pitch: Option<(f32, f32)>,
}

impl<'a> BehaviorCtx<'a> {
    pub fn is_solid_at(&self, x: i32, y: i32, z: i32) -> bool {
        let (id, _) = self.world.get_block(BlockPos::new(x, y, z));
        self.registry.block(id).map(|d| d.solid).unwrap_or(false) && id != self.air
    }
}

pub struct BehaviorRegistry {
    by_block: HashMap<BlockId, Box<dyn BlockBehavior>>,
    wasm_blocks: HashMap<BlockId, u32>,
}

impl Default for BehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BehaviorRegistry {
    pub fn new() -> Self {
        Self {
            by_block: HashMap::new(),
            wasm_blocks: HashMap::new(),
        }
    }

    pub fn register_from_block(&mut self, def: &BlockDef) {
        let Some(behavior) = def.behavior else {
            return;
        };
        match behavior {
            BehaviorRef::Native { id } => {
                self.by_block
                    .insert(def.id, Box::new(NativeBlockBehavior { id }));
            }
            BehaviorRef::Wasm { mod_index } => {
                self.wasm_blocks.insert(def.id, mod_index);
            }
        }
    }

    pub fn rebuild(&mut self, registry: &BlockRegistry) {
        self.by_block.clear();
        self.wasm_blocks.clear();
        for def in registry.all_blocks() {
            self.register_from_block(def);
        }
    }

    pub fn wasm_mod_index(&self, block_id: BlockId) -> Option<u32> {
        self.wasm_blocks.get(&block_id).copied()
    }

    pub fn on_break(&self, ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        if let Some(native) = self.native_for(ctx.block_id) {
            return native.on_break(ctx);
        }
        BehaviorResult::Ok
    }

    pub fn state_for_place(&self, def: &BlockDef, ctx: &BehaviorCtx<'_>) -> Option<BlockState> {
        if let Some(native) = self.by_block.get(&def.id) {
            if let Some(face) = ctx.face_normal {
                if let Some(state) = native.state_for_place(ctx, face) {
                    return Some(state);
                }
            }
        }
        native_state_for_place(def, ctx)
    }

    pub fn dynamic_light(&self, def: &BlockDef, state: BlockState) -> u8 {
        dynamic_light_for(def, state)
    }

    pub fn break_positions(&self, def: &BlockDef, pos: BlockPos, state: BlockState) -> Vec<BlockPos> {
        break_positions_for(def, pos, state)
    }

    pub fn push_reaction(&self, def: &BlockDef) -> stagcrest_protocol::PushReaction {
        if let Some(native) = self.by_block.get(&def.id) {
            return native.push_reaction(def);
        }
        default_push_reaction(def)
    }

    pub fn redstone_powerable(&self, def: &BlockDef) -> bool {
        if let Some(native) = self.by_block.get(&def.id) {
            return native.redstone_powerable(def);
        }
        redstone_powerable_for(def)
    }

    fn native_for(&self, block_id: BlockId) -> Option<&dyn BlockBehavior> {
        self.by_block.get(&block_id).map(|b| b.as_ref())
    }
}

struct NativeBlockBehavior {
    id: NativeBehaviorId,
}

impl BlockBehavior for NativeBlockBehavior {
    fn on_break(&self, _ctx: &mut BehaviorCtx<'_>) -> BehaviorResult {
        match self.id {
            NativeBehaviorId::Bedrock => BehaviorResult::Cancel,
            _ => BehaviorResult::Ok,
        }
    }

    fn state_for_place(
        &self,
        ctx: &BehaviorCtx<'_>,
        face_normal: [i32; 3],
    ) -> Option<BlockState> {
        native_state_for_place_id(self.id, ctx, face_normal)
    }
}

fn native_state_for_place(def: &BlockDef, ctx: &BehaviorCtx<'_>) -> Option<BlockState> {
    let BehaviorRef::Native { id } = def.behavior? else {
        return None;
    };
    let face = ctx.face_normal?;
    native_state_for_place_id(id, ctx, face)
}

fn native_state_for_place_id(
    id: NativeBehaviorId,
    ctx: &BehaviorCtx<'_>,
    face_normal: [i32; 3],
) -> Option<BlockState> {
    let (nx, ny, nz) = (face_normal[0], face_normal[1], face_normal[2]);
    let is_solid_at = |x: i32, y: i32, z: i32| ctx.is_solid_at(x, y, z);
    let place_pos = ctx.pos;
    let (dir_x, dir_z) = ctx
        .player_yaw_pitch
        .map(|(yaw, _)| (yaw.sin(), yaw.cos()))
        .unwrap_or((0.0, 1.0));

    match id {
        NativeBehaviorId::RedstoneInverter { .. } => {
            validate_torch_placement(is_solid_at, place_pos, nx, ny, nz)
        }
        NativeBehaviorId::RedstoneSwitch { .. } => validate_mount_placement(
            is_solid_at,
            place_pos,
            nx,
            ny,
            nz,
            dir_x,
            dir_z,
        ),
        NativeBehaviorId::RedstoneRepeater { .. } => validate_repeater_placement(
            is_solid_at,
            place_pos,
            nx,
            ny,
            nz,
            dir_x,
            dir_z,
        ),
        NativeBehaviorId::RedstoneObserver { .. } => validate_observer_placement(
            is_solid_at,
            place_pos,
            nx,
            ny,
            nz,
            dir_x,
            dir_z,
        ),
        NativeBehaviorId::RedstonePiston { .. } => {
            let (_, dir_y, _) = ctx.player_yaw_pitch.map(|(yaw, pitch)| {
                (
                    yaw.sin() * pitch.cos(),
                    -pitch.sin(),
                    yaw.cos() * pitch.cos(),
                )
            })?;
            validate_piston_placement(place_pos, dir_x, dir_y, dir_z)
        }
        _ => None,
    }
}
