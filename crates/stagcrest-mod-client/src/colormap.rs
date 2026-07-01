use stagcrest_protocol::TextureAnimation;

pub use stagcrest_protocol::sample_colormap_rgb;

/// Biome colormap data for client-side tinting.
#[derive(Clone)]
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

pub fn infer_vertical_strip_animation(
    texture_width: u32,
    texture_height: u32,
) -> Option<TextureAnimation> {
    if texture_width == 0 || texture_height <= texture_width {
        return None;
    }
    if texture_height % texture_width != 0 {
        return None;
    }
    Some(TextureAnimation {
        frame_width: texture_width,
        frame_height: texture_width,
        frame_count: texture_height / texture_width,
        frametime_ticks: 20,
    })
}
