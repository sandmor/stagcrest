use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use stagcrest_mod_client::TextureAtlas;

#[derive(Resource)]
pub struct BlockAtlasResource {
    pub atlas: TextureAtlas,
    pub grass_tint: Color,
    pub foliage_tint: Color,
    pub water_tint: Color,
    /// (frame_count, frame_uv_step, frametime_secs, elapsed_secs) — `.w` updated each frame
    pub fluid_anim: Vec4,
}

#[derive(Resource, Clone, ExtractResource)]
pub struct VoxelAtlasImage(pub Handle<Image>);

#[derive(Resource, Clone, ExtractResource, Default)]
pub struct VoxelMaterialSource {
    pub grass_tint: Color,
    pub foliage_tint: Color,
    pub water_tint: Color,
    pub fluid_anim: Vec4,
    pub atlas_width: f32,
    pub atlas_height: f32,
}

impl VoxelMaterialSource {
    pub fn from_atlas(atlas: &BlockAtlasResource) -> Self {
        Self {
            grass_tint: atlas.grass_tint,
            foliage_tint: atlas.foliage_tint,
            water_tint: atlas.water_tint,
            fluid_anim: atlas.fluid_anim,
            atlas_width: atlas.atlas.width as f32,
            atlas_height: atlas.atlas.height as f32,
        }
    }
}

fn sync_voxel_material_source(
    atlas: Option<Res<BlockAtlasResource>>,
    material: Option<ResMut<VoxelMaterialSource>>,
    mut commands: Commands,
) {
    let Some(atlas) = atlas else {
        return;
    };
    let next = VoxelMaterialSource::from_atlas(atlas.as_ref());
    if let Some(mut material) = material {
        *material = next;
    } else {
        commands.insert_resource(next);
    }
}

pub fn atlas_pixels_to_image(atlas: &TextureAtlas) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: atlas.width,
            height: atlas.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        atlas.pixels.clone(),
        TextureFormat::Rgba8UnormSrgb,
        default(),
    );
    image.sampler = ImageSampler::nearest();
    image
}

#[derive(Resource, Default, Clone, ExtractResource)]
pub struct VoxelCamera {
    pub view_proj: glam::Mat4,
    pub position: glam::Vec3,
}

pub struct VoxelRenderPlugin;

impl Plugin for VoxelRenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VoxelCamera>()
            .init_resource::<VoxelMaterialSource>()
            .add_systems(PostUpdate, sync_voxel_material_source);
    }
}
