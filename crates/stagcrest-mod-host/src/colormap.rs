use crate::assets::AssetReader;
use crate::resourcepack::ResourcePackLoader;

pub struct ColormapSet {
    pub grass: Vec<u8>,
    pub grass_w: u32,
    pub grass_h: u32,
    pub foliage: Vec<u8>,
    pub foliage_w: u32,
    pub foliage_h: u32,
    pub water: Vec<u8>,
    pub water_w: u32,
    pub water_h: u32,
}

impl ColormapSet {
    pub fn load(reader: &dyn AssetReader, packs: Option<&ResourcePackLoader>) -> Self {
        let grass = load_colormap(reader, packs, "grass").unwrap_or_else(default_grass_colormap);
        let foliage =
            load_colormap(reader, packs, "foliage").unwrap_or_else(default_foliage_colormap);
        let water = load_colormap(reader, packs, "water").unwrap_or_else(default_water_colormap);

        let (grass_w, grass_h, grass) = grass;
        let (foliage_w, foliage_h, foliage) = foliage;
        let (water_w, water_h, water) = water;

        Self {
            grass,
            grass_w,
            grass_h,
            foliage,
            foliage_w,
            foliage_h,
            water,
            water_w,
            water_h,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn load_async(
        reader: &crate::assets::HttpAssetReader,
        packs: Option<&ResourcePackLoader>,
    ) -> Self {
        let grass = load_colormap_async(reader, packs, "grass")
            .await
            .unwrap_or_else(default_grass_colormap);
        let foliage = load_colormap_async(reader, packs, "foliage")
            .await
            .unwrap_or_else(default_foliage_colormap);
        let water = load_colormap_async(reader, packs, "water")
            .await
            .unwrap_or_else(default_water_colormap);

        let (grass_w, grass_h, grass) = grass;
        let (foliage_w, foliage_h, foliage) = foliage;
        let (water_w, water_h, water) = water;

        Self {
            grass,
            grass_w,
            grass_h,
            foliage,
            foliage_w,
            foliage_h,
            water,
            water_w,
            water_h,
        }
    }

    /// Plains-like biome: temperature 0.8, downfall 0.4
    pub fn default_grass_tint(&self) -> [f32; 3] {
        sample_colormap_rgb(&self.grass, self.grass_w, self.grass_h, 0.8, 0.4)
    }

    pub fn default_foliage_tint(&self) -> [f32; 3] {
        sample_colormap_rgb(&self.foliage, self.foliage_w, self.foliage_h, 0.8, 0.4)
    }

    pub fn default_water_tint(&self) -> [f32; 3] {
        sample_colormap_rgb(&self.water, self.water_w, self.water_h, 0.8, 0.4)
    }
}

fn load_colormap(
    reader: &dyn AssetReader,
    packs: Option<&ResourcePackLoader>,
    name: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    if let Some(packs) = packs {
        if let Some(found) = packs.load_colormap(reader, name) {
            return Some(found);
        }
    }

    let bundled = format!("assets/minecraft/colormap/{name}.png");
    if reader.exists(&bundled) {
        return load_png_rgba(reader, &bundled);
    }
    None
}

#[cfg(target_arch = "wasm32")]
async fn load_colormap_async(
    reader: &crate::assets::HttpAssetReader,
    packs: Option<&ResourcePackLoader>,
    name: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    if let Some(packs) = packs {
        if let Some(found) = packs.load_colormap_async(reader, name).await {
            return Some(found);
        }
    }

    let bundled = format!("assets/minecraft/colormap/{name}.png");
    if reader.exists_async(&bundled).await {
        return load_png_rgba_async(reader, &bundled).await;
    }
    None
}

fn load_png_rgba(reader: &dyn AssetReader, path: &str) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = reader.read_bytes(path).ok()?;
    ResourcePackLoader::load_rgba_from_bytes(&bytes)
}

#[cfg(target_arch = "wasm32")]
async fn load_png_rgba_async(
    reader: &crate::assets::HttpAssetReader,
    path: &str,
) -> Option<(u32, u32, Vec<u8>)> {
    let bytes = reader.read_bytes_async(path).await.ok()?;
    ResourcePackLoader::load_rgba_from_bytes(&bytes)
}

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

fn default_grass_colormap() -> (u32, u32, Vec<u8>) {
    let w = 256u32;
    let h = 256u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let t = 1.0 - (x as f32 / (w - 1) as f32);
            let d = 1.0 - (y as f32 / (h - 1) as f32);
            let downfall = if t > 0.0 { d / t } else { 0.0 };
            let (r, g, b) = if downfall > 1.0 {
                (191, 183, 85)
            } else {
                let mix = downfall;
                let r = (128.0 + mix * 63.0) as u8;
                let g = (174.0 + mix * 45.0) as u8;
                let b = (57.0 + mix * 28.0) as u8;
                (r, g, b)
            };
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    (w, h, rgba)
}

fn default_foliage_colormap() -> (u32, u32, Vec<u8>) {
    let w = 256u32;
    let h = 256u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let t = 1.0 - (x as f32 / (w - 1) as f32);
            let d = 1.0 - (y as f32 / (h - 1) as f32);
            let downfall = if t > 0.0 { d / t } else { 0.0 };
            let (r, g, b) = if downfall > 1.0 {
                (174, 164, 42)
            } else {
                let mix = downfall;
                let r = (109.0 + mix * 65.0) as u8;
                let g = (161.0 + mix * 54.0) as u8;
                let b = (30.0 + mix * 12.0) as u8;
                (r, g, b)
            };
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    (w, h, rgba)
}

fn default_water_colormap() -> (u32, u32, Vec<u8>) {
    let w = 256u32;
    let h = 256u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let t = 1.0 - (x as f32 / (w - 1) as f32);
            let d = 1.0 - (y as f32 / (h - 1) as f32);
            let downfall = if t > 0.0 { d / t } else { 0.0 };
            let mix = downfall.min(1.0);
            let r = (32.0 + mix * 20.0) as u8;
            let g = (56.0 + mix * 40.0) as u8;
            let b = (140.0 + mix * 60.0) as u8;
            let i = ((y * w + x) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    (w, h, rgba)
}
