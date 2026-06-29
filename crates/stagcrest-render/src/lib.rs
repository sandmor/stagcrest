mod outline;
mod plugin;
mod underwater;

pub mod gpu_voxel;

pub use gpu_voxel::{
    capture_gpu_chunk_upload, pack_gpu_chunk_upload, pack_halo_from_world, GpuChunkCache,
    GpuChunkSyncState, GpuVoxelPlugin, GpuVoxelStats, GpuVoxelTables,
};
pub use outline::{
    block_outline_mesh, spawn_block_outline, BlockOutlineMarker, OutlineMaterial,
    OutlineMaterialPlugin,
};
pub use plugin::{
    atlas_pixels_to_image, BlockAtlasResource, VoxelAtlasImage, VoxelMaterialSource, VoxelCamera,
    VoxelRenderConfig, VoxelRenderPlugin,
};
pub use underwater::{UnderwaterEffect, UnderwaterPlugin};
