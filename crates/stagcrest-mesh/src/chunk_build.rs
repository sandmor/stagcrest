use stagcrest_mod_client::{
    compute_wire_connections, is_wire_line_block, is_wire_line_neighbor, BlockRegistry,
    ModelRegistry, PowerLookup,
};
use stagcrest_protocol::{fluid_flowing, BlockId, BlockPos, ChunkPos, CHUNK_SIZE};
use stagcrest_world::ChunkBlock;

use crate::greedy_mesh::{emit_greedy_cubes, is_greedy_eligible, GreedyGrid};
use crate::mesh_snapshot::{self, MeshSnapshot};
use crate::{
    build_column_tint_cache, emit_block_geometry, fluid_flow_textures, mesh_bucket_for_layer,
    should_cull_face, ChunkMesh, LightBuildContext, LightSampler, LightingContext, MeshClimateTint,
};

pub fn build_chunk_mesh_snapshot(snapshot: &MeshSnapshot) -> ChunkMesh {
    mesh_snapshot::mesh_from_snapshot(snapshot)
}

pub(crate) fn build_chunk_mesh_neighbors(
    chunk_pos: ChunkPos,
    air: BlockId,
    registry: &BlockRegistry,
    models: &ModelRegistry,
    power: Option<&dyn PowerLookup>,
    climate: Option<&MeshClimateTint<'_>>,
    neighbor_at: impl Fn(i32, i32, i32) -> Option<ChunkBlock>,
) -> ChunkMesh {
    let mut mesh = ChunkMesh::default();
    let base_x = chunk_pos.x * CHUNK_SIZE;
    let base_y = chunk_pos.y * CHUNK_SIZE;
    let base_z = chunk_pos.z * CHUNK_SIZE;

    let column_tints = climate.map(build_column_tint_cache);
    let mut greedy_grid = GreedyGrid::new();

    let light_ctx = LightBuildContext {
        registry,
        air,
        block_at: &|lx, ly, lz| neighbor_at(lx, ly, lz),
    };
    let light_grid = light_ctx.build_grid();
    let light_sampler = LightSampler::new(&light_grid, &light_ctx);

    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let Some(block) = neighbor_at(x, y, z) else {
                    continue;
                };
                if block.id == air {
                    continue;
                }
                let Some(def) = registry.block(block.id) else {
                    continue;
                };
                if !def.solid && !def.opaque && !def.transparent {
                    continue;
                }

                let wx = base_x + x;
                let wy = base_y + y;
                let wz = base_z + z;
                let origin = [wx as f32, wy as f32, wz as f32];
                let block_power = power
                    .map(|p| p.power_at(BlockPos::new(wx, wy, wz)))
                    .unwrap_or(0);

                let mut face_textures = registry
                    .block_face_textures_for_state(block.id, block.state)
                    .unwrap_or(def.face_textures);

                if def.fluid && fluid_flowing(block.state) {
                    if let Some(flow_tex) = registry.texture_by_name("stagcrest:water_flow") {
                        face_textures = fluid_flow_textures(face_textures, flow_tex);
                    }
                }

                let bucket = mesh_bucket_for_layer(def.render_layer);
                let lighting = LightingContext {
                    sampler: &light_sampler,
                    lx: x,
                    ly: y,
                    lz: z,
                };

                if is_greedy_eligible(def.geometry, def.fluid) {
                    greedy_grid.insert(
                        x as usize,
                        y as usize,
                        z as usize,
                        block,
                        face_textures,
                        bucket,
                    );
                    continue;
                }

                let wire_connections = if is_wire_line_block(registry, block.id) {
                    Some(compute_wire_connections(
                        |dx, dy, dz| {
                            let Some(neighbor) = neighbor_at(x + dx, y + dy, z + dz) else {
                                return false;
                            };
                            neighbor.id != air
                                && is_wire_line_neighbor(
                                    registry,
                                    neighbor.id,
                                    neighbor.state,
                                    -dx,
                                    -dz,
                                )
                        },
                        |dx, dy, dz| {
                            let Some(neighbor) = neighbor_at(x + dx, y + dy, z + dz) else {
                                return false;
                            };
                            if neighbor.id == air {
                                return false;
                            }
                            registry
                                .block(neighbor.id)
                                .is_some_and(|n| n.opaque && n.solid)
                        },
                    ))
                } else {
                    None
                };

                emit_block_geometry(
                    &mut mesh,
                    origin,
                    def.geometry,
                    &def.namespaced_id,
                    &face_textures,
                    bucket,
                    registry,
                    models,
                    block_power,
                    block.state,
                    x as i32,
                    y as i32,
                    z as i32,
                    climate,
                    column_tints.as_ref(),
                    |normal| {
                        should_cull_face(
                            def,
                            neighbor_at(
                                x + normal.x as i32,
                                y + normal.y as i32,
                                z + normal.z as i32,
                            ),
                            air,
                            registry,
                            normal,
                        )
                    },
                    wire_connections,
                    Some(&lighting),
                );
            }
        }
    }

    emit_greedy_cubes(
        &mut mesh,
        &greedy_grid,
        chunk_pos,
        air,
        registry,
        &neighbor_at,
        climate,
        column_tints.as_ref(),
        Some(&light_sampler),
    );

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_mod_client::{BlockRegistry, ModelRegistry};
    use stagcrest_protocol::{
        BlockDef, BlockFaceTextures, BlockGeometry, BlockId, BlockState, FaceTexture,
        ModelRenderLayer, TintKind, CHUNK_VOLUME,
    };
    use stagcrest_storage::InactiveChunk;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn stone_registry() -> BlockRegistry {
        let mut reg = BlockRegistry::new();
        let stone_tex =
            reg.register_texture("stagcrest:stone".into(), 16, 16, vec![0; 16 * 16 * 4]);
        reg.register_block(BlockDef {
            id: BlockId(1),
            namespaced_id: "stagcrest:stone".into(),
            display_name: "Stone".into(),
            face_textures: BlockFaceTextures {
                top: FaceTexture {
                    texture: stone_tex,
                    overlay: None,
                    tint: TintKind::None,
                    overlay_tint: TintKind::None,
                },
                bottom: FaceTexture {
                    texture: stone_tex,
                    overlay: None,
                    tint: TintKind::None,
                    overlay_tint: TintKind::None,
                },
                sides: FaceTexture {
                    texture: stone_tex,
                    overlay: None,
                    tint: TintKind::None,
                    overlay_tint: TintKind::None,
                },
            },
            geometry: BlockGeometry::Cube,
            render_layer: ModelRenderLayer::Opaque,
            opaque: true,
            solid: true,
            transparent: false,
            fluid: false,
            placeable: true,
            hardness: 1.0,
            behavior: None,
            callbacks: stagcrest_protocol::CallbackFlags::default(),
            map_color: [128, 128, 128],
            light_emission: 0,
            light_attenuation: 0,
        });
        reg
    }

    fn stone_chunk(registry: &BlockRegistry) -> MeshSnapshot {
        let air = BlockId(0);
        let stone = registry
            .block_by_name("stagcrest:stone")
            .unwrap_or(BlockId(1));
        let palette_ids = vec![air, stone];
        let palette_states = vec![BlockState(0), BlockState(0)];
        let indices = vec![1u16; CHUNK_VOLUME];
        let center =
            InactiveChunk::from_indices(palette_ids, palette_states, &indices).expect("chunk");
        MeshSnapshot {
            pos: ChunkPos { x: 0, y: 0, z: 0 },
            air,
            center,
            halo: HashMap::new(),
            power_grid: [0; CHUNK_VOLUME],
            registry: Arc::new(registry.clone()),
            models: Arc::new(ModelRegistry::new()),
            climate: None,
        }
    }

    #[test]
    fn solid_stone_chunk_builds_chunk_shell() {
        let registry = stone_registry();
        let snapshot = stone_chunk(&registry);
        let mesh = build_chunk_mesh_snapshot(&snapshot);
        assert!(!mesh.opaque_vertices.is_empty());
        assert_eq!(mesh.opaque_vertices.len() % 4, 0);
        // Six 16×16 external faces, four vertices per block face.
        assert_eq!(mesh.opaque_vertices.len(), 6 * 16 * 16 * 4);
    }

    #[test]
    fn redstone_dust_uses_wire_emitter() {
        let mut reg = BlockRegistry::new();
        let dot = reg.register_texture("stagcrest:dot".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let _line = reg.register_texture("stagcrest:line".into(), 16, 16, vec![0; 16 * 16 * 4]);
        let dust_id = BlockId(2);
        reg.register_block(BlockDef {
            id: dust_id,
            namespaced_id: "stagcrest:redstone_dust".into(),
            display_name: "Dust".into(),
            face_textures: BlockFaceTextures {
                top: FaceTexture {
                    texture: dot,
                    overlay: None,
                    tint: TintKind::PowerLevel,
                    overlay_tint: TintKind::None,
                },
                bottom: FaceTexture {
                    texture: dot,
                    overlay: None,
                    tint: TintKind::PowerLevel,
                    overlay_tint: TintKind::None,
                },
                sides: FaceTexture {
                    texture: dot,
                    overlay: None,
                    tint: TintKind::PowerLevel,
                    overlay_tint: TintKind::None,
                },
            },
            geometry: BlockGeometry::Flat,
            render_layer: ModelRenderLayer::Cutout,
            opaque: false,
            solid: false,
            transparent: true,
            fluid: false,
            placeable: true,
            hardness: 0.0,
            behavior: Some(stagcrest_protocol::BehaviorRef::Native {
                id: stagcrest_protocol::NativeBehaviorId::RedstoneWire { falloff: 1 },
            }),
            callbacks: stagcrest_protocol::CallbackFlags::default(),
            map_color: [128, 128, 128],
            light_emission: 0,
            light_attenuation: 0,
        });

        let air = BlockId(0);
        let palette_ids = vec![air, dust_id];
        let palette_states = vec![BlockState(0), BlockState(0)];
        let indices = vec![1u16; CHUNK_VOLUME];
        let center =
            InactiveChunk::from_indices(palette_ids, palette_states, &indices).expect("chunk");
        let snapshot = MeshSnapshot {
            pos: ChunkPos { x: 0, y: 0, z: 0 },
            air,
            center,
            halo: HashMap::new(),
            power_grid: [15u8; CHUNK_VOLUME],
            registry: Arc::new(reg),
            models: Arc::new(ModelRegistry::new()),
            climate: None,
        };
        let mesh = build_chunk_mesh_snapshot(&snapshot);
        assert!(!mesh.cutout_vertices.is_empty());
    }
}
