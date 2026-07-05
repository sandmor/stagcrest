//! Per-chunk skylight/blocklight propagation and vertex sampling.

use glam::Vec3;
use stagcrest_mod_client::BlockRegistry;
use stagcrest_protocol::{BlockPos, CHUNK_SIZE};
use stagcrest_world::ChunkBlock;

pub const GRID_SIZE: i32 = CHUNK_SIZE + 2;
pub const GRID_VOLUME: usize = (GRID_SIZE * GRID_SIZE * GRID_SIZE) as usize;
const MAX_LIGHT: u8 = 15;

#[derive(Debug, Clone)]
pub struct ChunkLightGrid {
    pub sky: [u8; GRID_VOLUME],
    pub block: [u8; GRID_VOLUME],
}

impl Default for ChunkLightGrid {
    fn default() -> Self {
        Self {
            sky: [0; GRID_VOLUME],
            block: [0; GRID_VOLUME],
        }
    }
}

impl ChunkLightGrid {
    pub fn idx(lx: i32, ly: i32, lz: i32) -> usize {
        let lx = (lx + 1) as usize;
        let ly = (ly + 1) as usize;
        let lz = (lz + 1) as usize;
        lx + GRID_SIZE as usize * (ly + GRID_SIZE as usize * lz)
    }

    pub fn sky_block(&self, lx: i32, ly: i32, lz: i32) -> (u8, u8) {
        let i = Self::idx(lx, ly, lz);
        (self.sky[i], self.block[i])
    }

    pub fn sample_at_block(&self, lx: i32, ly: i32, lz: i32) -> (u8, u8) {
        self.sky_block(lx, ly, lz)
    }

    /// Sample light grid at entity foot position (world block coords).
    pub fn sample_world(
        &self,
        chunk_base: BlockPos,
        world_pos: BlockPos,
    ) -> (u8, u8) {
        let lx = world_pos.x - chunk_base.x;
        let ly = world_pos.y - chunk_base.y;
        let lz = world_pos.z - chunk_base.z;
        if lx < -1 || lx > CHUNK_SIZE || ly < -1 || ly > CHUNK_SIZE || lz < -1 || lz > CHUNK_SIZE {
            return (15, 0);
        }
        self.sky_block(lx, ly, lz)
    }
}

pub fn encode_normal_axis(normal: Vec3) -> u8 {
    if normal.y > 0.5 {
        4
    } else if normal.y < -0.5 {
        5
    } else if normal.x > 0.5 {
        2
    } else if normal.x < -0.5 {
        3
    } else if normal.z > 0.5 {
        0
    } else {
        1
    }
}

pub fn pack_light(sky: u8, block: u8) -> u8 {
    ((sky.min(15) & 0xF) << 4) | (block.min(15) & 0xF)
}

pub fn vertex_ao(ao: u8) -> u8 {
    ao.min(3)
}

pub struct LightBuildContext<'a> {
    pub registry: &'a BlockRegistry,
    pub air: stagcrest_protocol::BlockId,
    pub block_at: &'a dyn Fn(i32, i32, i32) -> Option<ChunkBlock>,
}

impl LightBuildContext<'_> {
    fn def_at(&self, lx: i32, ly: i32, lz: i32) -> Option<&stagcrest_protocol::BlockDef> {
        let block = (self.block_at)(lx, ly, lz)?;
        if block.id == self.air {
            return None;
        }
        self.registry.block(block.id)
    }

    fn blocks_light(&self, lx: i32, ly: i32, lz: i32) -> bool {
        match self.def_at(lx, ly, lz) {
            Some(def) => def.blocks_skylight(),
            None => false,
        }
    }

    fn emission_at(&self, lx: i32, ly: i32, lz: i32) -> u8 {
        self.def_at(lx, ly, lz)
            .map(|d| d.light_emission.min(15))
            .unwrap_or(0)
    }

    fn attenuation_at(&self, lx: i32, ly: i32, lz: i32) -> u8 {
        match self.def_at(lx, ly, lz) {
            Some(def) => def.effective_light_attenuation(),
            None => 0,
        }
    }

    pub fn build_grid(&self) -> ChunkLightGrid {
        let mut grid = ChunkLightGrid::default();
        self.propagate_skylight(&mut grid);
        self.propagate_block_light(&mut grid);
        grid
    }

    fn propagate_skylight(&self, grid: &mut ChunkLightGrid) {
        for lx in -1..=CHUNK_SIZE {
            for lz in -1..=CHUNK_SIZE {
                let mut light = MAX_LIGHT;
                for ly in (-1..=CHUNK_SIZE).rev() {
                    let i = ChunkLightGrid::idx(lx, ly, lz);
                    if self.blocks_light(lx, ly, lz) {
                        let min = self
                            .def_at(lx, ly, lz)
                            .map(|d| d.effective_light_attenuation().min(15))
                            .unwrap_or(15);
                        light = light.saturating_sub(min.max(1));
                        grid.sky[i] = light;
                        if light == 0 {
                            for y2 in (-1..ly).rev() {
                                grid.sky[ChunkLightGrid::idx(lx, y2, lz)] = 0;
                            }
                            break;
                        }
                    } else {
                        grid.sky[i] = light;
                        let att = self.attenuation_at(lx, ly, lz);
                        if att > 0 {
                            light = light.saturating_sub(att);
                        }
                    }
                }
            }
        }
    }

    fn propagate_block_light(&self, grid: &mut ChunkLightGrid) {
        use std::collections::VecDeque;
        let mut queue = VecDeque::new();
        for lx in -1..=CHUNK_SIZE {
            for ly in -1..=CHUNK_SIZE {
                for lz in -1..=CHUNK_SIZE {
                    let emit = self.emission_at(lx, ly, lz);
                    if emit > 0 {
                        let i = ChunkLightGrid::idx(lx, ly, lz);
                        grid.block[i] = emit;
                        queue.push_back((lx, ly, lz, emit));
                    }
                }
            }
        }
        while let Some((lx, ly, lz, level)) = queue.pop_front() {
            if level <= 1 {
                continue;
            }
            for (dx, dy, dz) in NEIGHBORS {
                let nx = lx + dx;
                let ny = ly + dy;
                let nz = lz + dz;
                if nx < -1 || nx > CHUNK_SIZE || ny < -1 || ny > CHUNK_SIZE || nz < -1 || nz > CHUNK_SIZE
                {
                    continue;
                }
                let att = self.attenuation_at(nx, ny, nz);
                let next = level.saturating_sub(1 + att.min(1));
                if next == 0 {
                    continue;
                }
                let i = ChunkLightGrid::idx(nx, ny, nz);
                if grid.block[i] < next {
                    grid.block[i] = next;
                    queue.push_back((nx, ny, nz, next));
                }
            }
        }
    }
}

const NEIGHBORS: [(i32, i32, i32); 6] = [
    (1, 0, 0),
    (-1, 0, 0),
    (0, 1, 0),
    (0, -1, 0),
    (0, 0, 1),
    (0, 0, -1),
];

pub struct LightSampler<'a> {
    grid: &'a ChunkLightGrid,
    ctx: &'a LightBuildContext<'a>,
}

/// Block-local lighting context for mesh emitters.
pub struct LightingContext<'a> {
    pub sampler: &'a LightSampler<'a>,
    pub lx: i32,
    pub ly: i32,
    pub lz: i32,
}

impl LightingContext<'_> {
    pub fn vertex_shade(&self, normal: Vec3, corner: u8) -> (u8, u8, u8, u8) {
        let (sky, block, ao) = self
            .sampler
            .face_corner_light(self.lx, self.ly, self.lz, normal, corner);
        let mut flags = 0u8;
        if self.sampler.is_emissive_block(self.lx, self.ly, self.lz) {
            flags |= crate::VERTEX_FLAG_EMISSIVE;
        }
        if self.sampler.faces_fluid(self.lx, self.ly, self.lz, normal) {
            flags |= crate::VERTEX_FLAG_FACES_FLUID;
        }
        (
            encode_normal_axis(normal),
            pack_light(sky, block),
            vertex_ao(ao),
            flags,
        )
    }
}

pub fn shade_vertex(
    light: Option<&LightingContext<'_>>,
    normal: Vec3,
    corner: u8,
) -> (u8, u8, u8, u8) {
    if let Some(ctx) = light {
        ctx.vertex_shade(normal, corner)
    } else {
        (
            encode_normal_axis(normal),
            pack_light(15, 0),
            3,
            0,
        )
    }
}

impl<'a> LightSampler<'a> {
    pub fn new(grid: &'a ChunkLightGrid, ctx: &'a LightBuildContext<'a>) -> Self {
        Self { grid, ctx }
    }

    pub fn sky_block_at(&self, lx: i32, ly: i32, lz: i32) -> (u8, u8) {
        if lx < -1 || lx > CHUNK_SIZE || ly < -1 || ly > CHUNK_SIZE || lz < -1 || lz > CHUNK_SIZE {
            return (15, 0);
        }
        self.grid.sky_block(lx, ly, lz)
    }

    pub fn corner_ao(&self, lx: i32, ly: i32, lz: i32, normal: Vec3, corner: u8) -> u8 {
        let (dx, dy, dz) = corner_offset(normal, corner);
        let side1 = self.side_occludes(lx + dx, ly + dy, lz + dz);
        let side2 = self.side_occludes(lx + dz, ly + dy, lz + dx);
        let corner_occ = self.side_occludes(lx + dx + dz, ly + dy, lz + dx + dz);
        let ao = 3u8.saturating_sub(side1 + side2 + corner_occ);
        ao
    }

    fn side_occludes(&self, lx: i32, ly: i32, lz: i32) -> u8 {
        if self.ctx.blocks_light(lx, ly, lz) {
            1
        } else {
            0
        }
    }

    pub fn face_corner_light(
        &self,
        lx: i32,
        ly: i32,
        lz: i32,
        normal: Vec3,
        corner: u8,
    ) -> (u8, u8, u8) {
        let nx = normal.x.round() as i32;
        let ny = normal.y.round() as i32;
        let nz = normal.z.round() as i32;
        let (tangent, bitangent) = face_tangent_basis(normal);
        // Max of the 2×2 air-side neighborhood touching this face corner (Minecraft-style).
        let u0 = if corner & 1 == 0 { -1 } else { 0 };
        let u1 = u0 + 1;
        let v0 = if corner & 2 == 0 { -1 } else { 0 };
        let v1 = v0 + 1;
        let mut sky = 0u8;
        let mut block = 0u8;
        for du in [u0, u1] {
            for dv in [v0, v1] {
                let w = tangent * du as f32 + bitangent * dv as f32;
                let (s, b) = self.sky_block_at(
                    lx + nx + w.x.round() as i32,
                    ly + ny + w.y.round() as i32,
                    lz + nz + w.z.round() as i32,
                );
                sky = sky.max(s);
                block = block.max(b);
            }
        }
        let ao = self.corner_ao(lx, ly, lz, normal, corner);
        (sky, block, ao)
    }

    pub fn faces_fluid(&self, lx: i32, ly: i32, lz: i32, normal: Vec3) -> bool {
        let (dx, dy, dz) = (
            normal.x.round() as i32,
            normal.y.round() as i32,
            normal.z.round() as i32,
        );
        let nx = lx + dx;
        let ny = ly + dy;
        let nz = lz + dz;
        self.ctx
            .def_at(nx, ny, nz)
            .is_some_and(|d| d.fluid)
    }

    pub fn is_emissive_block(&self, lx: i32, ly: i32, lz: i32) -> bool {
        self.ctx.emission_at(lx, ly, lz) > 0
    }
}

fn face_tangent_basis(normal: Vec3) -> (Vec3, Vec3) {
    let n = normal.normalize_or_zero();
    let mut tangent = if n.y.abs() > 0.9 {
        Vec3::X
    } else {
        Vec3::Y
    };
    let bitangent = n.cross(tangent).normalize_or_zero();
    tangent = bitangent.cross(n).normalize_or_zero();
    (tangent, bitangent)
}

fn corner_offset(normal: Vec3, corner: u8) -> (i32, i32, i32) {
    let (tangent, bitangent) = face_tangent_basis(normal);
    let u = if corner & 1 == 0 { -1.0 } else { 1.0 };
    let v = if corner & 2 == 0 { -1.0 } else { 1.0 };
    let offset = tangent * u + bitangent * v;
    (
        offset.x.round() as i32,
        offset.y.round() as i32,
        offset.z.round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_light() {
        assert_eq!(pack_light(15, 7), (15 << 4) | 7);
    }
}
