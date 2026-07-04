use bevy::prelude::*;
use stagcrest_mesh::block_selection_bounds;
use stagcrest_mod_client::resolve_block_model;
use stagcrest_protocol::{BlockDef, BlockGeometry, BlockId};
use stagcrest_world::{RaycastBounds, RaycastHit};

use crate::game_session::GameCamera;
use crate::world_replica::WorldReplica;

#[derive(Resource, Default)]
pub struct BlockTarget {
    pub hit: Option<RaycastHit>,
}

pub fn is_targetable_block(id: BlockId, def: &BlockDef, air: BlockId) -> bool {
    id != air
        && (def.solid
            || def.placeable && matches!(def.geometry, BlockGeometry::Cube)
            || def.circuit.is_some()
            || matches!(def.geometry, BlockGeometry::Flat | BlockGeometry::Cross))
}

pub fn update_block_target(
    mod_ctx: Option<Res<crate::game::ModContext>>,
    world: Res<WorldReplica>,
    inventory_ui: Option<Res<crate::inventory::InventoryUiState>>,
    camera: Query<(&Transform, &crate::player::FlyCamera), With<GameCamera>>,
    mut target: ResMut<BlockTarget>,
) {
    let Some(ctx) = mod_ctx else {
        target.hit = None;
        return;
    };
    let Ok((cam, fly)) = camera.single() else {
        target.hit = None;
        return;
    };

    if !fly.captured || inventory_ui.is_some_and(|ui| ui.open) {
        target.hit = None;
        return;
    }

    let origin = glam::Vec3::new(cam.translation.x, cam.translation.y, cam.translation.z);
    let dir = glam::Vec3::new(cam.forward().x, cam.forward().y, cam.forward().z);
    let air = world.air();

    target.hit = stagcrest_world::raycast_blocks(
        origin,
        dir,
        8.0,
        |pos| {
            if !world.is_chunk_interactive(pos.chunk_pos()) {
                return false;
            }
            let (id, _) = world.get_block(pos);
            ctx.registry
                .block(id)
                .map(|d| is_targetable_block(id, d, air))
                .unwrap_or(false)
        },
        |pos| {
            let (id, state) = world.get_block(pos);
            let def = ctx.registry.block(id)?;
            if def.solid && matches!(def.geometry, BlockGeometry::Cube) {
                return None;
            }
            let model = match def.geometry {
                BlockGeometry::Model(model_id) => Some(resolve_block_model(
                    &ctx.models,
                    model_id,
                    &def.namespaced_id,
                    state,
                )),
                _ => None,
            };
            let sel = block_selection_bounds(def.geometry, model);
            Some(RaycastBounds {
                min: sel.min,
                max: sel.max,
            })
        },
    );
}
