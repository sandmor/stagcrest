use bevy::prelude::*;
use bevy::render::render_resource::CommandEncoderDescriptor;
use bevy::render::renderer::{RenderDevice, RenderQueue};

use crate::gpu_voxel::chunk_cache::GpuChunkCache;
use crate::gpu_voxel::draw_node::VoxelDrawBindCache;
use crate::gpu_voxel::gpu_resources::{
    grow_instance_buffers, GpuVoxelStats, GpuVoxelTables, InstanceBufferPool,
};
use crate::gpu_voxel::gpu_resources::{init_render_gpu_buffers, RenderGpuVoxelBuffers};
use crate::gpu_voxel::extract_chunk::PendingChunkExtract;
use crate::gpu_voxel::render_chunk_store::RenderChunkStore;

#[derive(Resource)]
pub struct GpuVoxelRenderState {
    pub buffers: RenderGpuVoxelBuffers,
}

pub fn prepare_gpu_voxel_buffers(
    tables: Option<Res<GpuVoxelTables>>,
    render_device: Res<RenderDevice>,
    existing: Option<Res<GpuVoxelRenderState>>,
    mut pending: Option<ResMut<PendingChunkExtract>>,
    mut commands: Commands,
) {
    if existing.is_some() {
        return;
    }
    let Some(tables) = tables else {
        return;
    };
    let buffers = init_render_gpu_buffers(&render_device, tables.as_ref());
    let bucket_slot_count = buffers.bucket_slot_count;
    let mut store = RenderChunkStore::new(&render_device, bucket_slot_count);
    if let Some(pending) = pending.as_mut() {
        for batch in pending.batches.drain(..) {
            store.apply_extract_batch(&batch);
        }
    }
    commands.insert_resource(GpuVoxelRenderState { buffers });
    commands.insert_resource(store);
    commands.insert_resource(InstanceBufferPool::new(bucket_slot_count));
}

pub fn sync_gpu_voxel_stats(
    chunk_cache: Option<Res<GpuChunkCache>>,
    mut stats: ResMut<GpuVoxelStats>,
) {
    let Some(chunk_cache) = chunk_cache else {
        return;
    };
    stats.chunk_count = chunk_cache.chunk_count();
}

/// Read overflow counters periodically and grow instance buffers when needed.
pub fn poll_gpu_voxel_overflow(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    tables: Option<Res<GpuVoxelTables>>,
    mut state: Option<ResMut<GpuVoxelRenderState>>,
    store: Option<ResMut<RenderChunkStore>>,
    pool: Option<ResMut<InstanceBufferPool>>,
    bind_cache: Option<ResMut<VoxelDrawBindCache>>,
    stats: Option<ResMut<GpuVoxelStats>>,
    mut tick: Local<u32>,
) {
    *tick = tick.wrapping_add(1);
    if *tick % 60 != 0 {
        return;
    }
    let (Some(tables), Some(state), Some(mut store), Some(mut pool), Some(mut bind_cache), Some(mut stats)) =
        (tables, state.as_mut(), store, pool, bind_cache, stats)
    else {
        return;
    };

    let bucket_slots = state.buffers.bucket_slot_count as usize;
    let (Some(overflow_buf), Some(readback_buf)) = (
        state.buffers.overflow_counters_buffer.as_ref(),
        state.buffers.overflow_readback_buffer.as_ref(),
    ) else {
        return;
    };

    let byte_size = crate::gpu_voxel::gpu_resources::counters_buffer_size(
        state.buffers.bucket_slot_count,
    );
    let scratch_byte_size = store.chunk_capacity as u64 * std::mem::size_of::<u32>() as u64;
    let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("voxel_overflow_readback_copy"),
    });
    encoder.copy_buffer_to_buffer(overflow_buf, 0, readback_buf, 0, byte_size);
    encoder.copy_buffer_to_buffer(
        &store.scratch_overflow_buffer,
        0,
        &store.scratch_overflow_readback_buffer,
        0,
        scratch_byte_size,
    );
    if let Some(counters_readback) = state.buffers.counters_readback_buffer.as_ref() {
        encoder.copy_buffer_to_buffer(
            &state.buffers.counters_buffer,
            0,
            counters_readback,
            0,
            byte_size,
        );
    }
    render_queue.submit(std::iter::once(encoder.finish()));
    render_device.poll(bevy::render::render_resource::Maintain::wait());

    let slice = readback_buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(bevy::render::render_resource::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    render_device.poll(bevy::render::render_resource::Maintain::wait());
    let Ok(Ok(())) = rx.recv() else {
        return;
    };
    let data = slice.get_mapped_range();
    let overflows: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
    drop(data);
    readback_buf.unmap();

    if let Some(counters_readback) = state.buffers.counters_readback_buffer.as_ref() {
        let slice = counters_readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(bevy::render::render_resource::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        render_device.poll(bevy::render::render_resource::Maintain::wait());
        if let Ok(Ok(())) = rx.recv() {
            let data = slice.get_mapped_range();
            let counters: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
            drop(data);
            counters_readback.unmap();
            stats.total_instances = counters.iter().sum();
            stats.instance_counts = counters;
        }
    }

    let total_overflow: u32 = overflows.iter().sum();
    let scratch_slice = store.scratch_overflow_readback_buffer.slice(..);
    let (scratch_tx, scratch_rx) = std::sync::mpsc::channel();
    scratch_slice.map_async(bevy::render::render_resource::MapMode::Read, move |result| {
        let _ = scratch_tx.send(result);
    });
    render_device.poll(bevy::render::render_resource::Maintain::wait());
    if let Ok(Ok(())) = scratch_rx.recv() {
        let data = scratch_slice.get_mapped_range();
        let scratch_overflows: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        store.scratch_overflow_readback_buffer.unmap();
        stats.scratch_overflow_chunks = scratch_overflows.iter().filter(|&&v| v > 0).count() as u32;
    }
    stats.global_overflow_buckets = overflows.iter().filter(|&&v| v > 0).count() as u32;
    stats.overflow_pending = total_overflow > 0;

    if total_overflow == 0 {
        return;
    }

    let grown = grow_instance_buffers(
        &render_device,
        tables.as_ref(),
        &mut pool,
        &overflows,
    );
    pool.generation = pool.generation.wrapping_add(1);
    state.buffers = grown;
    store.mark_all_dirty();
    bind_cache.reset_for_growth(pool.generation);
    stats.overflow_pending = false;

    if let Some(overflow_buf) = state.buffers.overflow_counters_buffer.as_ref() {
        let zeros = vec![0u32; bucket_slots];
        render_queue.write_buffer(overflow_buf, 0, bytemuck::cast_slice(&zeros));
    }
}
