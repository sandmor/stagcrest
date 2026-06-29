use bytemuck::{Pod, Zeroable};
use stagcrest_protocol::manifest::BIOME_GRID_VOLUME;

use crate::biome::BiomeRegistryClient;
use crate::colormap::{sample_colormap_rgb, ColormapSet};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct ChunkTintCell {
    pub grass: [f32; 3],
    pub foliage: [f32; 3],
}

impl Default for ChunkTintCell {
    fn default() -> Self {
        Self {
            grass: [1.0, 1.0, 1.0],
            foliage: [1.0, 1.0, 1.0],
        }
    }
}

fn rgb_f32(c: [u8; 3]) -> [f32; 3] {
    [
        c[0] as f32 / 255.0,
        c[1] as f32 / 255.0,
        c[2] as f32 / 255.0,
    ]
}

fn default_grass(colormaps: &ColormapSet) -> [f32; 3] {
    sample_colormap_rgb(
        &colormaps.grass,
        colormaps.grass_w,
        colormaps.grass_h,
        0.8,
        0.4,
    )
}

fn default_foliage(colormaps: &ColormapSet) -> [f32; 3] {
    sample_colormap_rgb(
        &colormaps.foliage,
        colormaps.foliage_w,
        colormaps.foliage_h,
        0.8,
        0.4,
    )
}

/// One cell per biome-grid slot (4×4×4), indexed like `BiomeRegistryClient::sample_at`.
pub fn build_chunk_tint_cells(
    biome_grid: &[u8; BIOME_GRID_VOLUME],
    biomes: &BiomeRegistryClient,
    colormaps: &ColormapSet,
) -> [ChunkTintCell; BIOME_GRID_VOLUME] {
    let fallback_grass = default_grass(colormaps);
    let fallback_foliage = default_foliage(colormaps);
    let mut cells = [ChunkTintCell::default(); BIOME_GRID_VOLUME];
    for (idx, cell) in cells.iter_mut().enumerate() {
        let biome_idx = biome_grid[idx] as u16;
        let Some(biome) = biomes.biome_by_index(biome_idx) else {
            cell.grass = fallback_grass;
            cell.foliage = fallback_foliage;
            continue;
        };
        let temp = biome.temperature.clamp(0.0, 1.0);
        let downfall = biome.downfall.clamp(0.0, 1.0);
        cell.grass = biome.grass_color.map(rgb_f32).unwrap_or_else(|| {
            sample_colormap_rgb(
                &colormaps.grass,
                colormaps.grass_w,
                colormaps.grass_h,
                temp,
                downfall,
            )
        });
        cell.foliage = biome.foliage_color.map(rgb_f32).unwrap_or_else(|| {
            sample_colormap_rgb(
                &colormaps.foliage,
                colormaps.foliage_w,
                colormaps.foliage_h,
                temp,
                downfall,
            )
        });
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::manifest::{BiomeClientDef, BiomesSnapshot};

    fn test_colormaps() -> ColormapSet {
        ColormapSet::from_snapshot(&stagcrest_protocol::ColormapSnapshot {
            grass: vec![255, 0, 0, 255],
            grass_w: 1,
            grass_h: 1,
            foliage: vec![0, 255, 0, 255],
            foliage_w: 1,
            foliage_h: 1,
            water: vec![0, 0, 255, 255],
            water_w: 1,
            water_h: 1,
        })
    }

    #[test]
    fn different_biomes_produce_different_grass_tints() {
        let colormaps = test_colormaps();
        let biomes = BiomeRegistryClient::from_snapshot(BiomesSnapshot {
            biomes: vec![
                BiomeClientDef {
                    index: 0,
                    namespaced_id: "stagcrest:plains".into(),
                    temperature: 0.8,
                    downfall: 0.4,
                    fog_color: [0, 0, 0],
                    fog_density: 0.0,
                    water_color: [0, 0, 0],
                    water_fog_color: [0, 0, 0],
                    sky_color: [0, 0, 0],
                    grass_color: Some([80, 200, 40]),
                    foliage_color: None,
                },
                BiomeClientDef {
                    index: 1,
                    namespaced_id: "stagcrest:desert".into(),
                    temperature: 2.0,
                    downfall: 0.0,
                    fog_color: [0, 0, 0],
                    fog_density: 0.0,
                    water_color: [0, 0, 0],
                    water_fog_color: [0, 0, 0],
                    sky_color: [0, 0, 0],
                    grass_color: Some([180, 160, 60]),
                    foliage_color: None,
                },
            ],
        });
        let mut grid = [0u8; BIOME_GRID_VOLUME];
        grid[0] = 0;
        grid[1] = 1;
        let cells = build_chunk_tint_cells(&grid, &biomes, &colormaps);
        assert_ne!(cells[0].grass, cells[1].grass);
    }
}
