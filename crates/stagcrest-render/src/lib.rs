mod reflection_camera;
mod entity_material;
mod graphics_settings;
mod outline;
mod prepass_depth;
mod plugin;
mod scene_lighting;
mod scene_reflection;
mod sky_palette;
mod skybox;
mod underwater;
mod volumetric_light;
mod voxel_material;
mod water_material;

pub use entity_material::{EntityMaterial, EntityMaterialPlugin, ENTITY_SHADER_HANDLE};
pub use graphics_settings::{
    GraphicsSettings, GraphicsSettingsPlugin, QualityTier, ReflectionTier,
};
pub use outline::{
    block_outline_mesh, spawn_block_outline, BlockOutlineMarker, OutlineMaterial,
    OutlineMaterialPlugin,
};
pub use plugin::{
    atlas_pixels_to_image, despawn_chunk_entities, next_atlas_revision, sync_chunk_meshes,
    BlockAtlasResource, ChunkEntityMarker, MeshCacheResource, PendingMeshRebuild,
    SyncChunkMeshesSet, VoxelRenderPlugin,
};
pub use stagcrest_mesh::ChunkLightGrid;
pub use scene_lighting::{Medium, SceneLighting, SceneLightingPlugin, SceneLightingUniform};
pub use reflection_camera::{
    main_camera_layers, main_camera_third_person_layers, player_visual_layers,
    water_chunk_layers, world_light_layers, MainViewCamera, PlanarReflectionCamera,
    ReflectionCameraPlugin, ReflectionPlaneHeight, PLAYER_LAYER, WATER_LAYER, WORLD_LAYER,
};
pub use scene_reflection::{
    ensure_scene_reflection_images, SceneReflectionImages, SceneReflectionParams,
    SceneReflectionPlugin,
};
pub use sky_palette::SkyPalette;
pub use skybox::{SkyboxPlugin, SkyMaterial};
pub use underwater::{UnderwaterEffect, UnderwaterPlugin};
pub use volumetric_light::{VolumetricLightPlugin, VolumetricLightSettings};
pub use voxel_material::{VoxelMaterial, VoxelMaterialPlugin};
pub use water_material::{sync_water_reflection_bindings, WaterMaterial, WaterMaterialPlugin};
