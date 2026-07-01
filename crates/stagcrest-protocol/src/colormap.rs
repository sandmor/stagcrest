//! Biome colormap sampling (shared by mesh, minimap, environment).

/// Minecraft biome colormap sampling.
pub fn sample_colormap_rgb(
    rgba: &[u8],
    width: u32,
    height: u32,
    temperature: f32,
    downfall: f32,
) -> [f32; 3] {
    let w = width.max(1) as f32;
    let h = height.max(1) as f32;
    let adj_temp = temperature.clamp(0.0, 1.0);
    let adj_downfall = downfall.clamp(0.0, 1.0) * adj_temp;
    let x = ((1.0 - adj_temp) * (w - 1.0)).round() as u32;
    let y = ((1.0 - adj_downfall) * (h - 1.0)).round() as u32;
    let idx = ((y * width + x) * 4) as usize;
    if idx + 2 >= rgba.len() {
        return [0.57, 0.74, 0.35];
    }
    [
        rgba[idx] as f32 / 255.0,
        rgba[idx + 1] as f32 / 255.0,
        rgba[idx + 2] as f32 / 255.0,
    ]
}
