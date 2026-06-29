use crate::worldgen::config::{HydrologyMode, TerrainConfig};
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::TerrainLayer;
use crate::worldgen::terrain::shaping::TerrainShaper;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiverSegment {
    Pool,
    Waterfall,
}

#[derive(Debug, Clone, Copy)]
pub struct RiverColumn {
    pub strength: f32,
    pub water_surface_y: i32,
    pub bed_y: i32,
    pub segment: RiverSegment,
    pub flow: (i8, i8),
    pub fall_drop: i32,
    pub downstream_water_y: i32,
}

#[derive(Debug, Clone, Copy, Default)]
struct DrainageCell {
    min_surface_y: f64,
    river_mask: bool,
    flow_to: Option<(i32, i32)>,
    water_level: f64,
    is_mouth: bool,
}

struct DrainageGridCache {
    cells: HashMap<(i32, i32), DrainageCell>,
}

pub struct HydrologySampler<'a> {
    config: &'a TerrainConfig,
    shaper: TerrainShaper<'a>,
    noise: &'a NoiseBank,
    cache: RefCell<DrainageGridCache>,
}

impl<'a> HydrologySampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank) -> Self {
        Self {
            config,
            shaper: TerrainShaper::new(config, noise),
            noise,
            cache: RefCell::new(DrainageGridCache {
                cells: HashMap::new(),
            }),
        }
    }

    pub fn shaper(&self) -> &TerrainShaper<'a> {
        &self.shaper
    }

    /// Precompute drainage cells covering a world region (chunk + halo).
    pub fn ensure_region(&self, wx0: i32, wz0: i32, wx1: i32, wz1: i32) {
        if self.config.hydrology_mode != HydrologyMode::DrainageGrid {
            return;
        }
        let cell_size = self.config.drainage_cell_size.max(1);
        let cx0 = floor_div(wx0, cell_size) - 1;
        let cz0 = floor_div(wz0, cell_size) - 1;
        let cx1 = floor_div(wx1, cell_size) + 1;
        let cz1 = floor_div(wz1, cell_size) + 1;

        for cz in cz0..=cz1 {
            for cx in cx0..=cx1 {
                self.ensure_cell(cx, cz);
            }
        }

        self.relax_region(cx0, cz0, cx1, cz1);
    }

    pub fn river_column(&self, wx: i32, wz: i32) -> Option<RiverColumn> {
        let erosion = self.erosion_at(wx, wz);
        let strength = self.shaper.rivers().river_strength(wx, wz, erosion);
        if strength <= 0.5 {
            return None;
        }

        let surface = self.shaper.surface_y(wx, wz);
        if surface < self.config.sea_level as f64 {
            return None;
        }
        if self.is_open_ocean(wx, wz, surface) {
            return None;
        }

        match self.config.hydrology_mode {
            HydrologyMode::Terrace => Some(self.terrace_column(wx, wz, strength, erosion)),
            HydrologyMode::DrainageGrid => self.drainage_column(wx, wz, strength, erosion),
        }
    }

    fn terrace_column(&self, wx: i32, wz: i32, strength: f32, erosion: f32) -> RiverColumn {
        let surface = self.shaper.surface_y(wx, wz);
        let channel = self.config.river_channel_depth.max(0);
        let mut water_surface_y = self.pool_water_y(surface);
        water_surface_y = self.coastal_clamp(wx, wz, water_surface_y);
        let bed_y = water_surface_y - channel;

        let (flow, downstream_water_y, fall_drop) =
            self.flow_and_drop(wx, wz, water_surface_y, erosion, |nx, nz| {
                self.coastal_clamp(
                    nx,
                    nz,
                    self.pool_water_y(self.shaper.surface_y(nx, nz)),
                )
            });

        let segment = self.segment_for_fall_drop(fall_drop);

        RiverColumn {
            strength,
            water_surface_y,
            bed_y,
            segment,
            flow,
            fall_drop,
            downstream_water_y,
        }
    }

    /// Terrace steps produce at-most-`river_terrace_step` drops between neighbors;
    /// only steeper falls count as waterfalls for features/decoration.
    fn segment_for_fall_drop(&self, fall_drop: i32) -> RiverSegment {
        let terrace = self.config.river_terrace_step.max(1);
        if fall_drop > terrace && fall_drop >= self.config.waterfall_min_drop {
            RiverSegment::Waterfall
        } else {
            RiverSegment::Pool
        }
    }

    /// When a river column borders submerged terrain or open-ocean shelf, merge water to sea level.
    fn coastal_clamp(&self, wx: i32, wz: i32, water_y: i32) -> i32 {
        let sea = self.config.sea_level;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            if self.shaper.surface_y(wx + dx, wz + dz) < sea as f64 {
                return sea;
            }
        }
        let surface = self.shaper.surface_y(wx, wz);
        if self.is_open_ocean(wx, wz, surface) {
            return sea;
        }
        water_y
    }

    /// Open-ocean tiles: shallow submerged shelf or deep ocean basin — no carved rivers.
    fn is_open_ocean(&self, wx: i32, wz: i32, surface: f64) -> bool {
        let cont = self.continentalness_at_world(wx, wz);
        if cont >= OPEN_OCEAN_CONT {
            return false;
        }
        let sea = self.config.sea_level as f64;
        let shelf_band =
            sea + self.config.mouth_sea_margin as f64 + self.config.max_channel_carve as f64 + 4.0;
        surface <= shelf_band
    }

    fn continentalness_at_world(&self, wx: i32, wz: i32) -> f64 {
        self.noise.get(TerrainLayer::Continentalness).sample2d(
            wx as f64 * self.config.continentalness_frequency,
            wz as f64 * self.config.continentalness_frequency,
        )
    }

    /// Band-floored pool level from local surface. Water top sits near the ground
    /// surface; `river_channel_depth` controls depth below that, not how far below
    /// terrain the water surface is placed.
    fn pool_water_y(&self, surface: f64) -> i32 {
        let step = self.config.river_terrace_step.max(1);
        let offset = self.config.river_terrace_offset;
        let max_carve = self.config.max_channel_carve.max(0);
        let surface_floor = surface.floor() as i32;
        const SURFACE_LIP: i32 = 1;

        let band = floor_div(surface_floor - offset, step);
        let pool = band * step + offset + (step - 1);
        let max_y = surface_floor - SURFACE_LIP;
        let min_y = (surface_floor - max_carve).min(max_y);
        pool.clamp(min_y, max_y)
    }

    fn drainage_column(&self, wx: i32, wz: i32, strength: f32, erosion: f32) -> Option<RiverColumn> {
        let cell_size = self.config.drainage_cell_size.max(1);
        let cx = floor_div(wx, cell_size);
        let cz = floor_div(wz, cell_size);
        self.ensure_region(
            wx - cell_size,
            wz - cell_size,
            wx + cell_size,
            wz + cell_size,
        );

        let cell = self.cache.borrow().cells.get(&(cx, cz)).copied()?;

        let surface = self.shaper.surface_y(wx, wz);
        let channel = self.config.river_channel_depth.max(0);
        let mut water_surface_y = self.pool_water_y(surface);
        water_surface_y = self.coastal_clamp(wx, wz, water_surface_y);
        let bed_y = water_surface_y - channel;

        let (flow, downstream_water_y, local_drop) =
            self.flow_and_drop(wx, wz, water_surface_y, erosion, |nx, nz| {
                self.coastal_clamp(
                    nx,
                    nz,
                    self.pool_water_y(self.shaper.surface_y(nx, nz)),
                )
            });

        let cell_drop = if cell.river_mask {
            if let Some((fcx, fcz)) = cell.flow_to {
                let cell_size = self.config.drainage_cell_size.max(1);
                let down_wx = fcx * cell_size + cell_size / 2;
                let down_wz = fcz * cell_size + cell_size / 2;
                let down_surface = self.shaper.surface_y(down_wx, down_wz);
                let down_water = self.pool_water_y(down_surface);
                water_surface_y - down_water
            } else if cell.is_mouth {
                water_surface_y - self.config.sea_level
            } else {
                0
            }
        } else {
            0
        };

        let fall_drop = local_drop.max(cell_drop);
        let segment = self.segment_for_fall_drop(local_drop);

        Some(RiverColumn {
            strength,
            water_surface_y,
            bed_y,
            segment,
            flow,
            fall_drop,
            downstream_water_y,
        })
    }

    fn flow_and_drop(
        &self,
        wx: i32,
        wz: i32,
        water_surface_y: i32,
        _erosion: f32,
        neighbor_water: impl Fn(i32, i32) -> i32,
    ) -> ((i8, i8), i32, i32) {
        let mut best_flow = (0i8, 0i8);
        let mut best_drop = 0i32;
        let mut best_down_y = water_surface_y;

        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = wx + dx;
            let nz = wz + dz;
            let n_erosion = self.erosion_at(nx, nz);
            if self.shaper.rivers().river_strength(nx, nz, n_erosion) <= 0.5 {
                continue;
            }
            let down_y = neighbor_water(nx, nz);
            let drop = water_surface_y - down_y;
            if drop > best_drop || (drop == best_drop && hash_tie(wx, wz, nx, nz)) {
                best_drop = drop;
                best_flow = (dx as i8, dz as i8);
                best_down_y = down_y;
            }
        }

        if best_flow == (0, 0) {
            let surface = self.shaper.surface_y(wx, wz);
            for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nx = wx + dx;
                let nz = wz + dz;
                let ns = self.shaper.surface_y(nx, nz);
                if ns < surface && hash_tie(wx, wz, nx, nz) {
                    best_flow = (dx as i8, dz as i8);
                    best_down_y = neighbor_water(nx, nz);
                    best_drop = water_surface_y - best_down_y;
                    break;
                }
            }
        }

        (best_flow, best_down_y.max(0), best_drop.max(0))
    }

    fn ensure_cell(&self, cx: i32, cz: i32) {
        if self.cache.borrow().cells.contains_key(&(cx, cz)) {
            return;
        }

        let cell_size = self.config.drainage_cell_size.max(1);
        let (river_mask, min_surface_y) = self.sample_cell(cx, cz, cell_size);

        let mut cell = DrainageCell {
            min_surface_y,
            river_mask,
            flow_to: None,
            water_level: self.config.sea_level as f64,
            is_mouth: false,
        };

        if cell.river_mask {
            let mouth_band = self.config.sea_level as f64
                + self.config.mouth_sea_margin as f64
                + self.config.max_channel_carve as f64;
            cell.is_mouth = cell.min_surface_y <= mouth_band;
            cell.flow_to = self.pick_flow_to(cx, cz, cell.min_surface_y);
            if cell.is_mouth {
                cell.water_level = self.config.sea_level as f64;
            } else {
                cell.water_level = cell.min_surface_y - self.config.river_channel_depth as f64;
            }
        }

        self.cache.borrow_mut().cells.insert((cx, cz), cell);
    }

    fn sample_cell(&self, cx: i32, cz: i32, cell_size: i32) -> (bool, f64) {
        let samples = 5;
        let mut river_mask = false;
        let mut min_surface_y = f64::MAX;

        for iz in 0..samples {
            for ix in 0..samples {
                let wx = cx * cell_size + (ix * cell_size / samples) + cell_size / (2 * samples);
                let wz = cz * cell_size + (iz * cell_size / samples) + cell_size / (2 * samples);
                let erosion = self.erosion_at(wx, wz);
                let strength = self.shaper.rivers().river_strength(wx, wz, erosion);
                if strength > 0.5 {
                    river_mask = true;
                    min_surface_y = min_surface_y.min(self.shaper.surface_y(wx, wz));
                }
            }
        }

        if river_mask && min_surface_y == f64::MAX {
            min_surface_y = self
                .shaper
                .surface_y(cx * cell_size, cz * cell_size);
        }

        (river_mask, min_surface_y)
    }

    fn pick_flow_to(&self, cx: i32, cz: i32, cell_min_y: f64) -> Option<(i32, i32)> {
        let cell_size = self.config.drainage_cell_size.max(1);
        let mut best: Option<(i32, i32)> = None;
        let mut best_y = f64::MAX;
        let mut best_cont = f64::MAX;

        for (dx, dz) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let ncx = cx + dx;
            let ncz = cz + dz;
            let (has_river, min_y) = self.sample_cell(ncx, ncz, cell_size);
            if !has_river {
                continue;
            }
            let cont = self.continentalness_at(ncx, ncz);
            if min_y < best_y
                || (min_y == best_y && cont < best_cont)
                || (min_y == best_y && cont == best_cont && hash_tie(cx, cz, ncx, ncz))
            {
                best_y = min_y;
                best_cont = cont;
                best = Some((ncx, ncz));
            }
        }

        if best_y >= cell_min_y {
            return None;
        }
        best
    }

    fn relax_region(&self, cx0: i32, cz0: i32, cx1: i32, cz1: i32) {
        for _ in 0..self.config.drainage_relax_passes {
            for cz in cz0..=cz1 {
                for cx in cx0..=cx1 {
                    let Some(cell) = self.cache.borrow().cells.get(&(cx, cz)).copied() else {
                        continue;
                    };
                    if !cell.river_mask || cell.is_mouth {
                        continue;
                    }
                    let Some((fcx, fcz)) = cell.flow_to else {
                        continue;
                    };
                    let downstream = self
                        .cache
                        .borrow()
                        .cells
                        .get(&(fcx, fcz))
                        .copied()
                        .unwrap_or(cell);
                    let local_cap =
                        cell.min_surface_y - self.config.river_channel_depth as f64;
                    let new_level = local_cap.min(downstream.water_level);
                    if let Some(c) = self.cache.borrow_mut().cells.get_mut(&(cx, cz)) {
                        c.water_level = new_level;
                    }
                }
            }
        }
    }

    fn erosion_at(&self, wx: i32, wz: i32) -> f32 {
        self.noise
            .get(TerrainLayer::Erosion)
            .sample2d(
                wx as f64 * self.config.erosion_frequency,
                wz as f64 * self.config.erosion_frequency,
            ) as f32
    }

    fn continentalness_at(&self, cx: i32, cz: i32) -> f64 {
        let cell_size = self.config.drainage_cell_size.max(1);
        let wx = cx * cell_size + cell_size / 2;
        let wz = cz * cell_size + cell_size / 2;
        self.noise.get(TerrainLayer::Continentalness).sample2d(
            wx as f64 * self.config.continentalness_frequency,
            wz as f64 * self.config.continentalness_frequency,
        )
    }
}

/// Continentalness below this is treated as open ocean for river mouths / suppression.
const OPEN_OCEAN_CONT: f64 = -0.2;

fn floor_div(a: i32, b: i32) -> i32 {
    if a >= 0 {
        a / b
    } else {
        (a - (b - 1)) / b
    }
}

fn hash_tie(ax: i32, az: i32, bx: i32, bz: i32) -> bool {
    let h = (ax as u64)
        .wrapping_mul(0x9E37_79B9)
        .wrapping_add(az as u64)
        .wrapping_add(bx as u64)
        .wrapping_mul(0x517C_C1B7)
        .wrapping_add(bz as u64);
    h % 2 == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::biome::BiomeRegistry;
    use crate::worldgen::climate::ClimateSampler;
    use crate::worldgen::config::TerrainConfig;
    use crate::worldgen::noise::NoiseBank;
    use crate::worldgen::seed::WorldSeed;
    use crate::worldgen::terrain::chunk::ChunkFiller;
    use crate::worldgen::terrain::DensitySampler;
    use crate::worldgen::test_fixtures::test_blocks;
    use stagcrest_protocol::ChunkPos;

    #[test]
    fn terrace_rivers_have_elevated_water_without_carving() {
        let mut config = TerrainConfig::default();
        config.hydrology_mode = HydrologyMode::Terrace;
        let noise = NoiseBank::new(WorldSeed(42));
        let hydro = HydrologySampler::new(&config, &noise);
        let origin = 4096;
        let mut elevated = 0usize;

        for dz in 0..256 {
            for dx in 0..256 {
                let wx = origin + dx;
                let wz = origin + dz;
                if let Some(col) = hydro.river_column(wx, wz) {
                    if col.water_surface_y > config.sea_level {
                        elevated += 1;
                    }
                }
            }
        }
        assert!(elevated > 0, "expected some elevated river water");
    }

    #[test]
    fn drainage_water_monotonic_downstream() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(42));
        let hydro = HydrologySampler::new(&config, &noise);
        hydro.ensure_region(4096, 4096, 4096 + 512, 4096 + 512);

        let mut checked = 0usize;
        for cz in 64..72 {
            for cx in 64..72 {
                let cell = hydro.cache.borrow().cells.get(&(cx, cz)).copied();
                let Some(cell) = cell else { continue };
                if !cell.river_mask || cell.is_mouth {
                    continue;
                }
                if let Some((fcx, fcz)) = cell.flow_to {
                    let down = hydro.cache.borrow().cells.get(&(fcx, fcz)).copied();
                    if let Some(down) = down {
                        assert!(
                            cell.water_level >= down.water_level - 0.01,
                            "water must not rise downstream at ({cx},{cz})"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 0, "expected drainage cells to validate");
    }

    #[test]
    fn submerged_terrain_has_no_river_column() {
        let mut config = TerrainConfig::default();
        config.hydrology_mode = HydrologyMode::DrainageGrid;
        let noise = NoiseBank::new(WorldSeed(42));
        let hydro = HydrologySampler::new(&config, &noise);
        let mut submerged = 0usize;
        for wz in 0..512 {
            for wx in 0..512 {
                if hydro.shaper().surface_y(wx, wz) < config.sea_level as f64 {
                    submerged += 1;
                    assert!(
                        hydro.river_column(wx, wz).is_none(),
                        "river column should not exist below sea at ({wx},{wz})"
                    );
                }
            }
        }
        assert!(submerged > 0, "expected submerged terrain in sample region");
    }

    #[test]
    fn terrace_drops_are_pools_not_waterfalls() {
        let mut config = TerrainConfig::default();
        config.hydrology_mode = HydrologyMode::DrainageGrid;
        config.river_terrace_step = 6;
        config.waterfall_min_drop = 4;
        let noise = NoiseBank::new(WorldSeed(42));
        let hydro = HydrologySampler::new(&config, &noise);
        hydro.ensure_region(4096, 4096, 4096 + 256, 4096 + 256);
        let origin = 4096;
        let mut pool = 0usize;
        let mut waterfall = 0usize;
        for wz in 0..256 {
            for wx in 0..256 {
                if let Some(col) = hydro.river_column(origin + wx, origin + wz) {
                    match col.segment {
                        RiverSegment::Pool => pool += 1,
                        RiverSegment::Waterfall => waterfall += 1,
                    }
                }
            }
        }
        assert!(
            pool > waterfall,
            "terrace rivers should be mostly pools, got pool={pool} waterfall={waterfall}"
        );
    }

    #[test]
    fn coastal_river_merges_to_sea_level() {
        let mut config = TerrainConfig::default();
        config.river_channel_depth = 10;
        config.max_channel_carve = 12;
        config.mouth_sea_margin = 2;
        config.hydrology_mode = HydrologyMode::DrainageGrid;
        let noise = NoiseBank::new(WorldSeed(42));
        let hydro = HydrologySampler::new(&config, &noise);
        hydro.ensure_region(0, 0, 512, 512);

        let mut mouth_cols = 0usize;
        let mut coastal_cols = 0usize;
        for wz in 0..512 {
            for wx in 0..512 {
                let surface = hydro.shaper().surface_y(wx, wz);
                if surface < config.sea_level as f64 {
                    continue;
                }
                let Some(col) = hydro.river_column(wx, wz) else {
                    continue;
                };
                let cell_size = config.drainage_cell_size.max(1);
                let cx = floor_div(wx, cell_size);
                let cz = floor_div(wz, cell_size);
                let is_mouth = hydro
                    .cache
                    .borrow()
                    .cells
                    .get(&(cx, cz))
                    .is_some_and(|c| c.is_mouth);
                let borders_ocean = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .any(|(dx, dz)| hydro.shaper().surface_y(wx + dx, wz + dz) < config.sea_level as f64);
                if is_mouth && borders_ocean {
                    assert_eq!(
                        col.water_surface_y, config.sea_level,
                        "coastal mouth river at ({wx},{wz}) should merge to sea"
                    );
                    mouth_cols += 1;
                }
                if borders_ocean {
                    assert_eq!(
                        col.water_surface_y, config.sea_level,
                        "coastal river at ({wx},{wz}) should merge to sea"
                    );
                    coastal_cols += 1;
                }
            }
        }
        assert!(mouth_cols > 0 || coastal_cols > 0, "expected coastal/mouth river columns");
    }

    #[test]
    fn elevated_river_geometry_is_bounded() {
        let mut config = TerrainConfig::default();
        config.river_channel_depth = 10;
        config.max_channel_carve = 12;
        config.mouth_sea_margin = 2;
        let noise = NoiseBank::new(WorldSeed(42));
        let hydro = HydrologySampler::new(&config, &noise);
        hydro.ensure_region(4096, 4096, 4096 + 512, 4096 + 512);
        let origin = 4096;
        let channel = config.river_channel_depth;
        let mut elevated = 0usize;

        for dz in 0..256 {
            for dx in 0..256 {
                let wx = origin + dx;
                let wz = origin + dz;
                let Some(col) = hydro.river_column(wx, wz) else {
                    continue;
                };
                let surface = hydro.shaper().surface_y(wx, wz).floor() as i32;
                assert!(
                    surface - col.bed_y <= channel + 2,
                    "river at ({wx},{wz}) carved too deep: surface={surface} bed={} max={}",
                    col.bed_y,
                    channel + 2
                );
                if col.water_surface_y <= config.sea_level + config.mouth_sea_margin {
                    continue;
                }
                assert!(
                    surface - col.water_surface_y <= 2,
                    "water at ({wx},{wz}) should be near surface: surface={surface} water={}",
                    col.water_surface_y
                );
                assert_eq!(col.water_surface_y - col.bed_y, channel);
                elevated += 1;
            }
        }
        assert!(elevated > 0, "expected elevated river columns");
    }

    #[test]
    fn river_chunk_density_integrity() {
        let mut config = TerrainConfig::default();
        config.river_width_blocks = 8.0;
        config.river_channel_depth = 10;
        let seed = WorldSeed(42);
        let noise = NoiseBank::new(seed);
        let density = DensitySampler::new(&config, &noise, seed);
        let climate = ClimateSampler::new(&config, &noise);
        let biomes = BiomeRegistry::default();
        let blocks = test_blocks();
        let filler = ChunkFiller::new(&config, &density, climate, &biomes, blocks);

        let origin = 4096;
        let chunk_base = origin / stagcrest_protocol::CHUNK_SIZE;
        for dcy in 0..8 {
            for dcx in 0..4 {
                for dcz in 0..4 {
                    filler.fill_density(ChunkPos {
                        x: chunk_base + dcx,
                        y: dcy,
                        z: chunk_base + dcz,
                    });
                }
            }
        }

        let mut water_count = 0usize;
        for dcx in 0..4 {
            for dcz in 0..4 {
                let entries = filler.fill_density(ChunkPos {
                    x: chunk_base + dcx,
                    y: 0,
                    z: chunk_base + dcz,
                });
                water_count += entries
                    .iter()
                    .filter(|(_, id, _)| *id == blocks.water)
                    .count();
                for &(block_pos, id, _) in &entries {
                    let wx = block_pos.x;
                    let wz = block_pos.z;
                    let y = block_pos.y;
                    let Some(col) = density.river_column(wx, wz) else {
                        continue;
                    };
                    if y > col.water_surface_y {
                        let surface_y = density.surface_y(wx, wz);
                        assert!(
                            !(id == blocks.stone && (y as f64) < surface_y),
                            "stone must not be placed above river water at ({wx},{y},{wz})"
                        );
                    }
                }
            }
        }
        assert!(water_count > 0, "expected river water blocks in fill_density output");

        let mut river_cols = 0u32;
        let mut air_above = 0u32;
        for wz in (origin - 64)..=(origin + 64) {
            for wx in (origin - 64)..=(origin + 64) {
                let Some(col) = density.river_column(wx, wz) else {
                    continue;
                };
                river_cols += 1;
                let surface_y = density.surface_y(wx, wz);
                for y in (col.bed_y + 1)..=col.water_surface_y {
                    assert!(
                        !density.is_solid_at_y(wx, y, wz, surface_y),
                        "solid in water band at ({wx},{y},{wz})"
                    );
                }
                for y in (col.water_surface_y + 1)..(surface_y.ceil() as i32) {
                    if (y as f64) < surface_y {
                        assert!(
                            !density.is_solid_at_y(wx, y, wz, surface_y),
                            "solid block above river water at ({wx},{y},{wz})"
                        );
                        air_above += 1;
                    }
                }
            }
        }
        assert!(river_cols > 0, "expected river columns");
        assert!(air_above > 0, "river channel should be open above water");
    }
}
