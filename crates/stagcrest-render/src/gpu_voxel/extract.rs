use bevy::prelude::*;
use bevy::render::extract_resource::ExtractResource;

use crate::gpu_voxel::types::GpuCameraUniform;
use crate::plugin::VoxelCamera;

#[derive(Resource, Clone, ExtractResource)]
pub struct ExtractedVoxelCamera(pub VoxelCamera);

pub fn camera_to_gpu_uniform(camera: &VoxelCamera) -> GpuCameraUniform {
    GpuCameraUniform {
        view_proj: camera.view_proj.to_cols_array_2d(),
        position: [
            camera.position.x,
            camera.position.y,
            camera.position.z,
            1.0,
        ],
    }
}
