use bevy::prelude::*;
use stagcrest_mesh::block_selection_bounds;
use stagcrest_mod_host::resolve_block_model;
use stagcrest_protocol::{BlockGeometry, BlockPos, BlockState};
use stagcrest_render::{block_outline_mesh, BlockOutlineMarker};

use crate::targeting::{is_targetable_block, BlockTarget};

const OUTLINE_INFLATE: f32 = 0.005;

/// Clears the active target and hides the outline overlay.
pub fn hide_block_outline(
    target: &mut BlockTarget,
    outline: &mut Query<&mut Visibility, With<BlockOutlineMarker>>,
) {
    target.hit = None;
    for mut visibility in outline {
        *visibility = Visibility::Hidden;
    }
}

pub(crate) fn sync_block_outline(
    target: Res<BlockTarget>,
    mod_ctx: Option<Res<crate::game::ModContext>>,
    world: Res<crate::game::StagcrestWorldResource>,
    mut last: Local<(Option<BlockPos>, Option<BlockState>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut outline: Query<(&mut Mesh3d, &mut Visibility), With<BlockOutlineMarker>>,
) {
    let Ok((mut mesh3d, mut visibility)) = outline.single_mut() else {
        return;
    };

    let Some(hit) = target.hit else {
        *visibility = Visibility::Hidden;
        *last = (None, None);
        return;
    };

    let Some(ctx) = mod_ctx else {
        *visibility = Visibility::Hidden;
        *last = (None, None);
        return;
    };

    let air = world.0.air();
    let (id, state) = world.0.get_block(hit.block);
    let Some(def) = ctx.registry.block(id) else {
        *visibility = Visibility::Hidden;
        *last = (None, None);
        return;
    };

    if !is_targetable_block(id, def, air) {
        *visibility = Visibility::Hidden;
        *last = (None, None);
        return;
    }

    let key_changed = last.0 != Some(hit.block) || last.1 != Some(state);
    if !key_changed {
        *visibility = Visibility::Visible;
        return;
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

    let bounds = block_selection_bounds(def.geometry, model);
    let mesh_id = mesh3d.0.id();
    let updated = block_outline_mesh(bounds, hit.block, OUTLINE_INFLATE);
    if let Some(existing) = meshes.get_mut(mesh_id) {
        *existing = updated;
    } else {
        mesh3d.0 = meshes.add(updated);
    }

    *last = (Some(hit.block), Some(state));
    *visibility = Visibility::Visible;
}

pub fn despawn_block_outline(
    commands: &mut Commands,
    query: &Query<Entity, With<BlockOutlineMarker>>,
) {
    for entity in query {
        commands.entity(entity).despawn();
    }
}
