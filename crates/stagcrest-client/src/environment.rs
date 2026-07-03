use bevy::prelude::*;
use stagcrest_mod_client::{sample_colormap_rgb, BiomeRegistryClient, InterpolatedEnvironment};
use stagcrest_protocol::BlockPos;

use crate::chunk_streaming::BiomeGridCache;
use crate::game::ModContext;
use crate::game_session::GameCamera;
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
    pub fog_color: [f32; 3],
    pub fog_density: f32,
    pub sky_color: [f32; 3],
    pub grass_tint: [f32; 3],
    pub foliage_tint: [f32; 3],
}

impl Default for PlayerEnvironment {
    fn default() -> Self {
        Self {
            submerged: false,
            submersion: 0.0,
            eye_block: BlockPos::new(0, 0, 0),
            temperature: 0.8,
            downfall: 0.4,
            water_tint: [0.25, 0.45, 0.9],
            fog_color: [0.75, 0.85, 1.0],
            fog_density: 0.01,
            sky_color: [0.47, 0.65, 1.0],
            grass_tint: [0.57, 0.74, 0.35],
            foliage_tint: [0.48, 0.78, 0.35],
        }
    }
}

pub fn update_player_environment(
    time: Res<Time>,
    mod_ctx: Option<Res<ModContext>>,
    world: Option<Res<WorldReplica>>,
    biome_cache: Option<Res<BiomeGridCache>>,
    camera: Query<&Transform, With<GameCamera>>,
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
            let (id, _) = world.get_block(eye_block);
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

    if let (Some(ctx), Some(_world), Some(biome_cache)) =
        (mod_ctx.as_deref(), world.as_deref(), biome_cache.as_deref())
    {
        let interp = sample_biome_environment(&ctx.biomes, biome_cache, eye_block);
        env.temperature = interp.temperature;
        env.downfall = interp.downfall;
        env.fog_color = interp.fog_color;
        env.fog_density = interp.fog_density;
        env.sky_color = interp.sky_color;
        env.water_tint = if env.submersion > 0.5 {
            interp.water_fog_color
        } else {
            interp.water_color
        };

        let cm = &ctx.colormaps;
        env.grass_tint = interp.grass_color.unwrap_or_else(|| {
            sample_colormap_rgb(
                &cm.grass,
                cm.grass_w,
                cm.grass_h,
                interp.temperature,
                interp.downfall,
            )
        });
        env.foliage_tint = interp.foliage_color.unwrap_or_else(|| {
            sample_colormap_rgb(
                &cm.foliage,
                cm.foliage_w,
                cm.foliage_h,
                interp.temperature,
                interp.downfall,
            )
        });
    }
}

fn sample_biome_environment(
    biomes: &BiomeRegistryClient,
    biome_cache: &BiomeGridCache,
    pos: BlockPos,
) -> InterpolatedEnvironment {
    biomes.interpolate_at(pos, |chunk| biome_cache.biome_grid(chunk).map(|g| *g))
}
