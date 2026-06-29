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

    /// River strength 0..1 for path width and biome assignment.
    pub fn river_strength(&self, wx: i32, wz: i32, erosion: f32) -> f32 {
        let (sx, sz) = sample_space(self.noise, self.config, wx, wz);
        let n = self.noise.get(TerrainLayer::River).sample2d(sx, sz);
        let d = n.abs();
        let grad = river_gradient_magnitude(self.noise, sx, sz);
        // w = B * f * |∇n|: block width in sample space scaled by local contour slope.
        let width = (self.config.river_width_blocks as f64 * self.config.river_frequency * grad)
            .max(0.001);
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

fn river_gradient_magnitude(noise: &NoiseBank, sx: f64, sz: f64) -> f64 {
    const H: f64 = 0.05;
    let layer = TerrainLayer::River;
    let n0 = noise.get(layer).sample2d(sx, sz);
    let nx = noise.get(layer).sample2d(sx + H, sz);
    let nz = noise.get(layer).sample2d(sx, sz + H);
    let gx = (nx - n0) / H;
    let gz = (nz - n0) / H;
    (gx * gx + gz * gz).sqrt().max(0.5)
}

fn sample_space(noise: &NoiseBank, config: &TerrainConfig, wx: i32, wz: i32) -> (f64, f64) {
    let x = wx as f64;
    let z = wz as f64;
    let warp_x = noise.get(TerrainLayer::RiverB).sample2d(
        x * config.river_b_frequency,
        z * config.river_b_frequency,
    );
    let warp_z = noise.get(TerrainLayer::RiverB).sample2d(
        (x + 1000.0) * config.river_b_frequency,
        (z + 1000.0) * config.river_b_frequency,
    );
    let sx = x * config.river_frequency + warp_x * config.river_warp_strength;
    let sz = z * config.river_frequency + warp_z * config.river_warp_strength;
    (sx, sz)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::noise::NoiseBank;
    use crate::worldgen::seed::WorldSeed;
    use std::collections::VecDeque;

    fn erosion_at(noise: &NoiseBank, config: &TerrainConfig, wx: i32, wz: i32) -> f32 {
        noise
            .get(TerrainLayer::Erosion)
            .sample2d(
                wx as f64 * config.erosion_frequency,
                wz as f64 * config.erosion_frequency,
            ) as f32
    }

    fn full_river_span_axis(
        rivers: &RiverSampler<'_>,
        erosion: &impl Fn(i32, i32) -> f32,
        wx: i32,
        wz: i32,
        axis_x: bool,
    ) -> i32 {
        if rivers.river_strength(wx, wz, erosion(wx, wz)) <= 0.5 {
            return 0;
        }
        let mut span = 1i32;
        for d in 1..64 {
            let (x, z) = if axis_x { (wx + d, wz) } else { (wx, wz + d) };
            if rivers.river_strength(x, z, erosion(x, z)) <= 0.5 {
                break;
            }
            span += 1;
        }
        for d in 1..64 {
            let (x, z) = if axis_x { (wx - d, wz) } else { (wx, wz - d) };
            if rivers.river_strength(x, z, erosion(x, z)) <= 0.5 {
                break;
            }
            span += 1;
        }
        span
    }

    fn max_perpendicular_river_width_near(
        config: &TerrainConfig,
        noise: &NoiseBank,
        origin_x: i32,
        origin_z: i32,
        radius: i32,
    ) -> i32 {
        let rivers = RiverSampler::new(config, noise);
        let erosion = |x: i32, z: i32| erosion_at(noise, config, x, z);
        let mut best = 0i32;
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let wx = origin_x + dx;
                let wz = origin_z + dz;
                if rivers.river_strength(wx, wz, erosion(wx, wz)) <= 0.9 {
                    continue;
                }
                let sx = full_river_span_axis(&rivers, &erosion, wx, wz, true);
                let sz = full_river_span_axis(&rivers, &erosion, wx, wz, false);
                best = best.max(sx.min(sz));
            }
        }
        best
    }

    #[test]
    fn rivers_form_connected_channels() {
        let config = TerrainConfig::default();
        let noise = NoiseBank::new(WorldSeed(42));
        let rivers = RiverSampler::new(&config, &noise);
        let origin = 4096;
        let size = 512;
        let mut river_map = vec![false; size * size];

        for dz in 0..size {
            for dx in 0..size {
                let wx = origin + dx as i32;
                let wz = origin + dz as i32;
                river_map[dz * size + dx] =
                    rivers.is_river(wx, wz, erosion_at(&noise, &config, wx, wz));
            }
        }

        let total_rivers: usize = river_map.iter().filter(|&&r| r).count();
        assert!(total_rivers > 100, "expected meaningful river coverage, got {total_rivers}");

        let mut largest = 0usize;
        let mut visited = vec![false; size * size];
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

    #[test]
    fn river_width_blocks_affects_span() {
        let noise = NoiseBank::new(WorldSeed(42));
        let mut config = TerrainConfig::default();
        config.river_width_blocks = 8.0;
        let span8 = max_perpendicular_river_width_near(&config, &noise, 4096, 4096, 64);
        config.river_width_blocks = 16.0;
        let span16 = max_perpendicular_river_width_near(&config, &noise, 4096, 4096, 64);
        assert!(span8 > 0, "expected rivers in scan, perp_width8={span8}");
        assert!(
            span16 > span8,
            "doubling width should increase perpendicular span (8->{span8}, 16->{span16})"
        );
        assert!(
            (5..=12).contains(&span8),
            "width=8 should yield ~8 block perpendicular river, got {span8}"
        );
    }
}
