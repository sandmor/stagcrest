use crate::worldgen::biome::{BiomeRegistry, ResolvedBiome};
use crate::worldgen::climate::ClimateSampler;
use crate::worldgen::config::TerrainConfig;
use crate::worldgen::decorate_snapshot::DecorateSnapshot;
use crate::worldgen::terrain::column::ColumnBlocks;
use crate::worldgen::terrain::density::DensitySampler;
use stagcrest_protocol::manifest::BIOME_GRID_VOLUME;
use stagcrest_protocol::{
    still_water_state, BlockId, BlockPos, BlockState, ChunkPos, LocalBlockPos, CHUNK_SIZE,
    CHUNK_VOLUME,
};

pub struct ChunkFiller<'a> {
    config: &'a TerrainConfig,
    density: DensitySampler<'a>,
    climate: ClimateSampler<'a>,
    biomes: &'a BiomeRegistry,
    blocks: ColumnBlocks,
}

impl<'a> ChunkFiller<'a> {
    pub fn new(
        config: &'a TerrainConfig,
        density: DensitySampler<'a>,
        climate: ClimateSampler<'a>,
        biomes: &'a BiomeRegistry,
        blocks: ColumnBlocks,
    ) -> Self {
        Self {
            config,
            density,
            climate,
            biomes,
            blocks,
        }
    }

    pub fn fill_density(&self, pos: ChunkPos) -> Vec<(BlockPos, BlockId, BlockState)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_y = pos.y * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        let water_state = still_water_state();
        let mut entries = Vec::new();

        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let wx = base_x + lx;
                    let y = base_y + ly;
                    let wz = base_z + lz;
                    let surface_y = self.density.surface_y(wx, wz);

                    let mut block = if y == self.config.world_min_y {
                        self.blocks.bedrock
                    } else if self.density.is_solid_at_y(wx, y, wz, surface_y) {
                        self.blocks.stone
                    } else if y < self.config.sea_level {
                        self.blocks.water
                    } else {
                        continue;
                    };

                    if block == self.blocks.stone && self.density.ore_block(wx, y, wz) {
                        block = self.blocks.iron_ore;
                    }

                    let state = if block == self.blocks.water {
                        water_state
                    } else {
                        BlockState(0)
                    };
                    entries.push((BlockPos::new(wx, y, wz), block, state));
                }
            }
        }

        entries
    }

    pub fn decorate(
        &self,
        snapshot: &DecorateSnapshot,
        pos: ChunkPos,
        density_entries: &[(BlockPos, BlockId, BlockState)],
        biome_grid: &[u8; BIOME_GRID_VOLUME],
    ) -> Vec<(BlockPos, BlockId, BlockState)> {
        let base_x = pos.x * CHUNK_SIZE;
        let base_y = pos.y * CHUNK_SIZE;
        let base_z = pos.z * CHUNK_SIZE;
        let sea = self.config.sea_level;
        let water_state = still_water_state();

        let mut buffer = vec![self.blocks.air; CHUNK_VOLUME];
        let mut states = vec![BlockState(0); CHUNK_VOLUME];
        let chunk_side = CHUNK_SIZE as usize;
        let mut surface_y_map = [[sea as f64; 16]; 16];

        for lz in 0..chunk_side {
            for lx in 0..chunk_side {
                let wx = base_x + lx as i32;
                let wz = base_z + lz as i32;
                surface_y_map[lz][lx] = self.density.surface_y(wx, wz);
            }
        }

        for &(block_pos, id, state) in density_entries {
            let local = block_pos.local();
            let idx = local.index();
            buffer[idx] = id;
            states[idx] = state;
        }

        for ly in (0..CHUNK_SIZE).rev() {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let wx = base_x + lx;
                    let y = base_y + ly;
                    let wz = base_z + lz;
                    let idx = LocalBlockPos {
                        x: lx as u8,
                        y: ly as u8,
                        z: lz as u8,
                    }
                    .index();

                    let id = buffer[idx];
                    if id != self.blocks.stone && id != self.blocks.iron_ore {
                        continue;
                    }

                    if y == self.config.world_min_y {
                        buffer[idx] = self.blocks.bedrock;
                        states[idx] = BlockState(0);
                        continue;
                    }

                    let surface_y = surface_y_map[lz as usize][lx as usize];
                    let biome = self.biome_at_local(lx, ly, lz, biome_grid);
                    let on_island = self.density.sky_islands().is_solid(wx, y, wz);

                    let above_id = if ly < CHUNK_SIZE - 1 {
                        let above_idx = LocalBlockPos {
                            x: lx as u8,
                            y: (ly + 1) as u8,
                            z: lz as u8,
                        }
                        .index();
                        buffer[above_idx]
                    } else {
                        snapshot.face_above(lx, lz)
                    };

                    let below_id = if ly > 0 {
                        let below_idx = LocalBlockPos {
                            x: lx as u8,
                            y: (ly - 1) as u8,
                            z: lz as u8,
                        }
                        .index();
                        buffer[below_idx]
                    } else {
                        snapshot.face_below(lx, lz)
                    };

                    let above_open = above_id == self.blocks.air || above_id == self.blocks.water;
                    let shoreline = y == sea && below_id == self.blocks.water;

                    if above_open || shoreline {
                        let top = if on_island && y as f64 >= surface_y - 2.0 {
                            biome.surface_top
                        } else if y >= sea {
                            if self.density.is_riverbank(wx, wz) {
                                biome.underwater_top.unwrap_or(self.blocks.sand)
                            } else if is_beach(
                                surface_y,
                                sea,
                                biome.climate_downfall,
                                &buffer,
                                snapshot,
                                base_x,
                                base_y,
                                base_z,
                                lx,
                                lz,
                                self.blocks.water,
                            ) {
                                self.blocks.sand
                            } else {
                                biome.surface_top
                            }
                        } else {
                            biome.underwater_top.unwrap_or(self.blocks.sand)
                        };
                        buffer[idx] = top;
                        states[idx] = BlockState(0);
                    } else if y >= sea {
                        let under = if on_island {
                            biome.surface_under
                        } else if above_id == biome.surface_top
                            || above_id == self.blocks.grass
                            || above_id == self.blocks.sand
                            || above_id == self.blocks.dirt
                        {
                            biome.surface_under
                        } else {
                            continue;
                        };

                        let depth = count_surface_depth(
                            snapshot,
                            &buffer,
                            base_y,
                            wx,
                            wz,
                            lx as usize,
                            ly as usize,
                            lz as usize,
                            &self.blocks,
                        );
                        if depth < biome.surface_depth as i32 {
                            buffer[idx] = under;
                            states[idx] = BlockState(0);
                        }
                    }
                }
            }
        }

        // Frozen rivers: ice cap on the top water block (sea_level - 1).
        let water_top = sea - 1;
        let water_top_ly = water_top - base_y;
        if (0..CHUNK_SIZE).contains(&water_top_ly) {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let wx = base_x + lx;
                    let wz = base_z + lz;
                    if !self.density.is_river(wx, wz) {
                        continue;
                    }
                    let idx = LocalBlockPos {
                        x: lx as u8,
                        y: water_top_ly as u8,
                        z: lz as u8,
                    }
                    .index();
                    if buffer[idx] != self.blocks.water {
                        continue;
                    }
                    let surface_y = surface_y_map[lz as usize][lx as usize];
                    let params = self.climate.at_3d(wx, water_top, wz, surface_y);
                    if params.temperature < 0.15 && self.blocks.ice != self.blocks.air {
                        buffer[idx] = self.blocks.ice;
                        states[idx] = BlockState(0);
                    }
                }
            }
        }

        let mut entries = Vec::new();
        for ly in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                for lx in 0..CHUNK_SIZE {
                    let idx = LocalBlockPos {
                        x: lx as u8,
                        y: ly as u8,
                        z: lz as u8,
                    }
                    .index();
                    let id = buffer[idx];
                    if id == self.blocks.air {
                        continue;
                    }
                    let wx = base_x + lx;
                    let y = base_y + ly;
                    let wz = base_z + lz;
                    let state = if id == self.blocks.water {
                        water_state
                    } else {
                        states[idx]
                    };
                    entries.push((BlockPos::new(wx, y, wz), id, state));
                }
            }
        }

        entries
    }

    fn biome_at_local(
        &self,
        lx: i32,
        ly: i32,
        lz: i32,
        grid: &[u8; BIOME_GRID_VOLUME],
    ) -> &ResolvedBiome {
        let gx = (lx / 4).clamp(0, 3) as usize;
        let gy = (ly / 4).clamp(0, 3) as usize;
        let gz = (lz / 4).clamp(0, 3) as usize;
        let idx = gx + gy * 4 + gz * 16;
        let biome_idx = grid[idx] as u16;
        self.biomes
            .biome_by_index(biome_idx)
            .unwrap_or_else(|| self.biomes.default_plains())
    }
}

fn is_beach(
    surface_y: f64,
    sea: i32,
    downfall: f32,
    buffer: &[BlockId],
    snapshot: &DecorateSnapshot,
    base_x: i32,
    base_y: i32,
    base_z: i32,
    lx: i32,
    lz: i32,
    water: BlockId,
) -> bool {
    if downfall <= 0.3 {
        return false;
    }
    let near_sea = surface_y >= (sea - 1) as f64 && surface_y <= (sea + 1) as f64;
    near_sea && has_adjacent_water(buffer, snapshot, base_x, base_y, base_z, lx, lz, sea, water)
}

fn has_adjacent_water(
    buffer: &[BlockId],
    snapshot: &DecorateSnapshot,
    base_x: i32,
    base_y: i32,
    base_z: i32,
    lx: i32,
    lz: i32,
    sea: i32,
    water: BlockId,
) -> bool {
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let nlx = lx + dx;
        let nlz = lz + dz;
        for &y in &[sea, sea - 1] {
            if block_at_local(
                buffer,
                snapshot,
                base_x,
                base_y,
                base_z,
                nlx,
                y - base_y,
                nlz,
            ) == water
            {
                return true;
            }
        }
    }
    false
}

fn block_at_local(
    buffer: &[BlockId],
    snapshot: &DecorateSnapshot,
    base_x: i32,
    base_y: i32,
    base_z: i32,
    lx: i32,
    ly: i32,
    lz: i32,
) -> BlockId {
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE || lz >= CHUNK_SIZE || ly < 0 || ly >= CHUNK_SIZE {
        return snapshot.block_at(BlockPos::new(base_x + lx, base_y + ly, base_z + lz));
    }
    let idx = LocalBlockPos {
        x: lx as u8,
        y: ly as u8,
        z: lz as u8,
    }
    .index();
    buffer[idx]
}

fn count_surface_depth(
    snapshot: &DecorateSnapshot,
    buffer: &[BlockId],
    base_y: i32,
    wx: i32,
    wz: i32,
    lx: usize,
    ly: usize,
    lz: usize,
    blocks: &ColumnBlocks,
) -> i32 {
    let mut depth = 0i32;
    for dy in 1..=4 {
        let above = if ly + dy < CHUNK_SIZE as usize {
            let idx = LocalBlockPos {
                x: lx as u8,
                y: (ly + dy) as u8,
                z: lz as u8,
            }
            .index();
            buffer[idx]
        } else {
            snapshot.block_at(BlockPos::new(wx, base_y + ly as i32 + dy as i32, wz))
        };
        if above == blocks.grass
            || above == blocks.sand
            || above == blocks.dirt
            || above == blocks.short_grass
            || above == blocks.tall_grass
        {
            depth += 1;
        } else {
            break;
        }
    }
    depth
}
