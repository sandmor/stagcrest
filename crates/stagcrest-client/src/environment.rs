use bevy::prelude::*;
use stagcrest_mod_client::sample_colormap_rgb;
use stagcrest_protocol::BlockPos;

use crate::game::ModContext;
use crate::player::FlyCamera;
use crate::world_replica::WorldReplica;

const SUBMERSION_LERP_SPEED: f32 = 4.0;

#[derive(Resource)]
pub struct PlayerEnvironment {
    pub submerged: bool,
    pub submersion: f32,
    pub eye_block: BlockPos,
    pub temperature: f32,
    pub downfall: f32,
    pub water_tint: [f32; 3],
}

impl Default for PlayerEnvironment {
    fn default() -> Self {
        Self {
            submerged: false,
            submersion: 0.0,
            eye_block: BlockPos::new(0, 0, 0),
            temperature: 0.8,
            downfall: 0.4,
            water_tint: [0.2, 0.4, 0.7],
        }
    }
}

pub fn update_player_environment(
    time: Res<Time>,
    mod_ctx: Option<Res<ModContext>>,
    world: Option<Res<WorldReplica>>,
    camera: Query<&Transform, With<FlyCamera>>,
    mut env: ResMut<PlayerEnvironment>,
) {
    let Ok(transform) = camera.single() else {
        return;
    };

    let pos = transform.translation;
    let eye_block = BlockPos::new(
        pos.x.floor() as i32,
        pos.y.floor() as i32,
        pos.z.floor() as i32,
    );
    env.eye_block = eye_block;

    env.submerged = match (mod_ctx.as_deref(), world.as_deref()) {
        (Some(ctx), Some(world)) => {
            let (id, _) = world.0.get_block(eye_block);
            ctx.registry.block(id).is_some_and(|def| def.fluid)
        }
        _ => false,
    };

    let target = if env.submerged { 1.0 } else { 0.0 };
    let dt = time.delta_secs();
    if env.submersion < target {
        env.submersion = (env.submersion + SUBMERSION_LERP_SPEED * dt).min(target);
    } else if env.submersion > target {
        env.submersion = (env.submersion - SUBMERSION_LERP_SPEED * dt).max(target);
    }

    if let Some(ctx) = mod_ctx.as_deref() {
        let cm = &ctx.colormaps;
        env.water_tint = sample_colormap_rgb(
            &cm.water,
            cm.water_w,
            cm.water_h,
            env.temperature,
            env.downfall,
        );
    }
}
