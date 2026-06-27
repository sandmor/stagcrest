use crate::worldgen::config::TerrainConfig;
use crate::worldgen::noise::NoiseBank;
use crate::worldgen::seed::TerrainLayer;

/// Organic river paths via a domain-warped noise zero-contour (winding lines).
pub struct RiverSampler<'a> {
    config: &'a TerrainConfig,
    noise: &'a NoiseBank,
}

impl<'a> RiverSampler<'a> {
    pub fn new(config: &'a TerrainConfig, noise: &'a NoiseBank) -> Self {
        Self { config, noise }
    }

    /// River strength 0..1 for valley carving and biome assignment.
    pub fn river_strength(&self, wx: i32, wz: i32, erosion: f32) -> f32 {
        let x = wx as f64;
        let z = wz as f64;

        let warp_x = self.noise.get(TerrainLayer::RiverB).sample2d(
            x * self.config.river_b_frequency,
            z * self.config.river_b_frequency,
        );
        let warp_z = self.noise.get(TerrainLayer::RiverB).sample2d(
            (x + 1000.0) * self.config.river_b_frequency,
            (z + 1000.0) * self.config.river_b_frequency,
        );

        let warp = self.config.river_warp_strength;
        let sx = x * self.config.river_frequency + warp_x * warp;
        let sz = z * self.config.river_frequency + warp_z * warp;

        let n = self.noise.get(TerrainLayer::River).sample2d(sx, sz);
        let d = n.abs();
        let width = self.config.river_width.max(0.001);
        let mut strength = (1.0 - d / width).clamp(0.0, 1.0) as f32;

        // Suppress rivers in steep/mountainous terrain (low erosion).
        let bias = self.config.river_erosion_bias as f32;
        strength *= smoothstep(-0.6 + bias, 0.2 + bias, erosion);
        strength.clamp(0.0, 1.0)
    }

    pub fn is_river(&self, wx: i32, wz: i32, erosion: f32) -> bool {
        self.river_strength(wx, wz, erosion) > 0.5
    }

    pub fn is_riverbank(&self, wx: i32, wz: i32, erosion: f32) -> bool {
        let s = self.river_strength(wx, wz, erosion);
        s > 0.0 && s <= 0.5
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::config::TerrainConfig;
    use crate::worldgen::noise::NoiseBank;
    use crate::worldgen::seed::{TerrainLayer, WorldSeed};
    use crate::worldgen::terrain::shaping::TerrainShaper;
    use std::collections::VecDeque;

    #[test]
    fn rivers_form_carved_connected_channels() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(42));
        let rivers = RiverSampler::new(&config, &noise);
        let shaper = TerrainShaper::new(&config, &noise);
        let origin = 4096;
        let size = 512;
        let mut river_map = vec![false; size * size];
        let mut carved = 0usize;

        for dz in 0..size {
            for dx in 0..size {
                let wx = origin + dx as i32;
                let wz = origin + dz as i32;
                let erosion = noise.get(TerrainLayer::Erosion).sample2d(
                    wx as f64 * config.erosion_frequency,
                    wz as f64 * config.erosion_frequency,
                ) as f32;
                let is_river = rivers.is_river(wx, wz, erosion);
                river_map[dz * size + dx] = is_river;
                if is_river {
                    let surface_y = shaper.surface_y(wx, wz);
                    assert!(
                        surface_y < config.sea_level as f64,
                        "river at ({wx},{wz}) surface_y={surface_y} should be below sea level"
                    );
                    carved += 1;
                }
            }
        }

        assert!(carved > 20, "expected river samples to validate carving");

        let total_rivers: usize = river_map.iter().filter(|&&r| r).count();
        assert!(
            total_rivers > 100,
            "expected meaningful river coverage, got {total_rivers}"
        );

        let mut isolated = 0usize;
        for dz in 0..size {
            for dx in 0..size {
                if !river_map[dz * size + dx] {
                    continue;
                }
                let neighbors = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                    .into_iter()
                    .filter(|&(ox, oz)| {
                        let nx = dx as i32 + ox;
                        let nz = dz as i32 + oz;
                        nx >= 0
                            && nz >= 0
                            && nx < size as i32
                            && nz < size as i32
                            && river_map[nz as usize * size + nx as usize]
                    })
                    .count();
                if neighbors == 0 {
                    isolated += 1;
                }
            }
        }

        assert!(
            isolated * 100 / total_rivers.max(1) < 20,
            "too many isolated river columns ({isolated}/{total_rivers}); field looks like noise"
        );

        let mut visited = vec![false; size * size];
        let mut largest = 0usize;
        for dz in 0..size {
            for dx in 0..size {
                let idx = dz * size + dx;
                if !river_map[idx] || visited[idx] {
                    continue;
                }
                let mut queue = VecDeque::from([(dx, dz)]);
                let mut component = 0usize;
                visited[idx] = true;
                while let Some((cx, cz)) = queue.pop_front() {
                    component += 1;
                    for (ox, oz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let nx = cx as i32 + ox;
                        let nz = cz as i32 + oz;
                        if nx < 0 || nz < 0 || nx >= size as i32 || nz >= size as i32 {
                            continue;
                        }
                        let nidx = nz as usize * size + nx as usize;
                        if river_map[nidx] && !visited[nidx] {
                            visited[nidx] = true;
                            queue.push_back((nx as usize, nz as usize));
                        }
                    }
                }
                largest = largest.max(component);
            }
        }

        assert!(
            largest >= 50,
            "largest river component too small ({largest}); expected serpentine channels"
        );
    }
}
