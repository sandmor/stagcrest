use bevy::prelude::*;
use stagcrest_mod_host::sample_colormap_rgb;
use stagcrest_protocol::BlockPos;

use crate::game::{ModContext, StagcrestWorldResource, TerrainGen, WorldColormaps};
use crate::player::FlyCamera;
use crate::streaming_pipeline::TerrainBiomes;

const SUBMERSION_LERP_SPEED: f32 = 4.0;

#[derive(Resource)]
pub struct PlayerEnvironment {
    pub submerged: bool,
    pub submersion: f32,
    pub eye_block: BlockPos,
    pub biome_id: Option<String>,
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
            biome_id: None,
            temperature: 0.0,
            downfall: 0.0,
            water_tint: [0.0, 0.0, 0.0],
        }
    }
}

pub fn update_player_environment(
    time: Res<Time>,
    mod_ctx: Option<Res<ModContext>>,
    world: Option<Res<StagcrestWorldResource>>,
    terrain: Option<Res<TerrainGen>>,
    biomes: Option<Res<TerrainBiomes>>,
    colormaps: Option<Res<WorldColormaps>>,
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

    let wx = eye_block.x;
    let wz = eye_block.z;

    if let (Some(terrain), Some(biomes)) = (terrain.as_deref(), biomes.as_deref()) {
        let (temp, downfall) = terrain.0.climate_at(wx, wz);
        env.temperature = temp;
        env.downfall = downfall;
        env.biome_id = Some(biomes.0.biome_at(temp, downfall).namespaced_id.clone());
    } else {
        env.temperature = 0.0;
        env.downfall = 0.0;
        env.biome_id = None;
    }

    if let (Some(terrain), Some(colormaps)) = (terrain.as_deref(), colormaps.as_deref()) {
        let (temp, downfall) = terrain.0.climate_at(wx, wz);
        let cm = &colormaps.0;
        env.water_tint = sample_colormap_rgb(&cm.water, cm.water_w, cm.water_h, temp, downfall);
    }
}
