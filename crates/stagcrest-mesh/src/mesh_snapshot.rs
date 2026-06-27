use crate::MeshClimateTint;
use stagcrest_mod_client::{BlockRegistry, ColormapSet, ModelRegistry, PowerLookup};
use stagcrest_protocol::{BlockPos, ChunkPos, LocalBlockPos, CHUNK_SIZE, CHUNK_VOLUME};
use stagcrest_storage::InactiveChunk;
use stagcrest_world::{ChunkBlock, World};
use std::collections::HashMap;
use std::sync::Arc;

/// Serializable climate inputs for worker-thread mesh tinting.
#[derive(Clone)]
pub struct MeshClimateSnapshot {
    pub colormaps: Arc<ColormapSet>,
    pub temperature: f32,
    pub downfall: f32,
}

impl MeshClimateSnapshot {
    pub fn as_tint(&self) -> MeshClimateTint<'_> {
        MeshClimateTint {
            colormaps: &self.colormaps,
            temperature: self.temperature,
            downfall: self.downfall,
        }
    }
}

/// Read-only chunk + halo context for offline mesh building.
#[derive(Clone)]
pub struct MeshSnapshot {
    pub pos: ChunkPos,
    pub air: stagcrest_protocol::BlockId,
    pub center: InactiveChunk,
    pub halo: HashMap<BlockPos, ChunkBlock>,
    pub power_grid: [u8; CHUNK_VOLUME],
    pub registry: Arc<BlockRegistry>,
    pub models: Arc<ModelRegistry>,
    pub climate: Option<MeshClimateSnapshot>,
}

impl MeshSnapshot {
    pub fn capture(
        pos: ChunkPos,
        world: &World,
        registry: Arc<BlockRegistry>,
        models: Arc<ModelRegistry>,
        power_grid: [u8; CHUNK_VOLUME],
        climate: Option<MeshClimateSnapshot>,
    ) -> Option<Self> {
        let center = world.pack_chunk_for_storage(pos)?;
        let air = world.air();
        Some(Self {
            pos,
            air,
            center,
            halo: capture_halo(world, pos, air),
            power_grid,
            registry,
            models,
            climate,
        })
    }

    pub(crate) fn block_at(&self, lx: i32, ly: i32, lz: i32) -> Option<ChunkBlock> {
        if (0..CHUNK_SIZE).contains(&lx)
            && (0..CHUNK_SIZE).contains(&ly)
            && (0..CHUNK_SIZE).contains(&lz)
        {
            let local = LocalBlockPos {
                x: lx as u8,
                y: ly as u8,
                z: lz as u8,
            };
            let reader = stagcrest_storage::InactiveChunkReader::new(&self.center);
            let (id, state) = reader.block_at(local);
            return Some(ChunkBlock { id, state });
        }
        let wx = self.pos.x * CHUNK_SIZE + lx;
        let wy = self.pos.y * CHUNK_SIZE + ly;
        let wz = self.pos.z * CHUNK_SIZE + lz;
        self.halo.get(&BlockPos::new(wx, wy, wz)).copied()
    }
}

fn capture_halo(
    world: &World,
    pos: ChunkPos,
    air: stagcrest_protocol::BlockId,
) -> HashMap<BlockPos, ChunkBlock> {
    let base_x = pos.x * CHUNK_SIZE;
    let base_y = pos.y * CHUNK_SIZE;
    let base_z = pos.z * CHUNK_SIZE;
    let mut halo = HashMap::new();
    for lx in -1..=CHUNK_SIZE {
        for ly in -1..=CHUNK_SIZE {
            for lz in -1..=CHUNK_SIZE {
                if lx >= 0
                    && lx < CHUNK_SIZE
                    && ly >= 0
                    && ly < CHUNK_SIZE
                    && lz >= 0
                    && lz < CHUNK_SIZE
                {
                    continue;
                }
                let wx = base_x + lx;
                let wy = base_y + ly;
                let wz = base_z + lz;
                if let Some(block) = world.get_block_if_loaded(BlockPos::new(wx, wy, wz)) {
                    if block.id != air {
                        halo.insert(BlockPos::new(wx, wy, wz), block);
                    }
                }
            }
        }
    }
    halo
}

pub fn capture_power_grid(pos: ChunkPos, power: Option<&dyn PowerLookup>) -> [u8; CHUNK_VOLUME] {
    let mut grid = [0u8; CHUNK_VOLUME];
    let Some(power) = power else {
        return grid;
    };
    let base_x = pos.x * CHUNK_SIZE;
    let base_y = pos.y * CHUNK_SIZE;
    let base_z = pos.z * CHUNK_SIZE;
    for y in 0..CHUNK_SIZE {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let local = LocalBlockPos {
                    x: x as u8,
                    y: y as u8,
                    z: z as u8,
                };
                grid[local.index()] =
                    power.power_at(BlockPos::new(base_x + x, base_y + y, base_z + z));
            }
        }
    }
    grid
}

pub(crate) fn mesh_from_snapshot(snapshot: &MeshSnapshot) -> crate::ChunkMesh {
    struct SnapshotPower<'a> {
        grid: &'a [u8; CHUNK_VOLUME],
        pos: ChunkPos,
    }

    impl PowerLookup for SnapshotPower<'_> {
        fn power_at(&self, pos: BlockPos) -> u8 {
            if pos.chunk_pos() != self.pos {
                return 0;
            }
            self.grid[pos.local().index()]
        }
    }

    let power = SnapshotPower {
        grid: &snapshot.power_grid,
        pos: snapshot.pos,
    };
    let climate = snapshot.climate.as_ref().map(|c| c.as_tint());
    let neighbor_at = |lx: i32, ly: i32, lz: i32| snapshot.block_at(lx, ly, lz);
    crate::build_chunk_mesh_neighbors(
        snapshot.pos,
        snapshot.air,
        snapshot.registry.as_ref(),
        snapshot.models.as_ref(),
        Some(&power),
        climate.as_ref(),
        neighbor_at,
    )
}
