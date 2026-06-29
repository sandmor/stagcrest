use bevy::prelude::*;
use bevy::render::render_resource::{CommandEncoderDescriptor, MapMode, Maintain};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::MainWorld;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::gpu_voxel::chunk_cache::GpuChunkCache;
use crate::gpu_voxel::chunk_feedback::{GpuChunkRenderFeedback, GpuChunkSyncState};
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

#[derive(Default)]
enum OverflowReadbackStage {
    #[default]
    Idle,
    PendingOverflow {
        rx: Receiver<Vec<u32>>,
    },
    PendingCounters {
        overflow: Vec<u32>,
        rx: Receiver<Vec<u32>>,
    },
    PendingScratch {
        overflow: Vec<u32>,
        rx: Receiver<Vec<u32>>,
    },
}

#[derive(Default)]
pub struct OverflowReadbackState {
    frames_since_poll: u32,
    stage: OverflowReadbackStage,
}

const OVERFLOW_POLL_INTERVAL: u32 = 60;

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
        let mut feedback = GpuChunkRenderFeedback::default();
        for batch in pending.batches.drain(..) {
            store.apply_extract_batch(&batch, &mut feedback);
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

pub fn publish_gpu_chunk_feedback(
    feedback: Option<Res<GpuChunkRenderFeedback>>,
    main_world: Option<ResMut<MainWorld>>,
) {
    let (Some(feedback), Some(mut main_world)) = (feedback, main_world) else {
        return;
    };
    let main = main_world.as_mut();
    if let Some(mut cache) = main.get_resource_mut::<GpuChunkCache>() {
        cache.apply_render_feedback(
            &feedback.rendered,
            &feedback.failed_uploads,
            &feedback.evicted,
        );
    }
    if let Some(mut sync) = main.get_resource_mut::<GpuChunkSyncState>() {
        sync.render_confirmed = feedback.rendered.clone();
    }
    if let Some(mut stats) = main.get_resource_mut::<GpuVoxelStats>() {
        stats.upload_failures = feedback.failed_uploads.len();
        stats.evictions = feedback.evicted.len();
    }
}

/// Non-blocking overflow counter readback; grows instance buffers when needed.
pub fn poll_gpu_voxel_overflow(
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    tables: Option<Res<GpuVoxelTables>>,
    mut state: Option<ResMut<GpuVoxelRenderState>>,
    store: Option<ResMut<RenderChunkStore>>,
    pool: Option<ResMut<InstanceBufferPool>>,
    bind_cache: Option<ResMut<VoxelDrawBindCache>>,
    stats: Option<ResMut<GpuVoxelStats>>,
    mut feedback: Option<ResMut<GpuChunkRenderFeedback>>,
    mut readback: Local<OverflowReadbackState>,
) {
    render_device.poll(Maintain::Poll);

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

    match std::mem::take(&mut readback.stage) {
        OverflowReadbackStage::Idle => {
            readback.frames_since_poll = readback.frames_since_poll.wrapping_add(1);
            if readback.frames_since_poll % OVERFLOW_POLL_INTERVAL != 0 {
                readback.stage = OverflowReadbackStage::Idle;
                return;
            }
            readback.frames_since_poll = 0;

            let byte_size = crate::gpu_voxel::gpu_resources::counters_buffer_size(
                state.buffers.bucket_slot_count,
            );
            let scratch_byte_size =
                store.chunk_capacity as u64 * std::mem::size_of::<u32>() as u64;
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

            let (tx, rx) = std::sync::mpsc::channel();
            let overflow_clone = readback_buf.clone();
            readback_buf.slice(..).map_async(MapMode::Read, move |result| {
                if let Err(err) = result {
                    tracing::warn!("voxel overflow readback map failed: {err:?}");
                    return;
                }
                let data = overflow_clone.slice(..).get_mapped_range();
                let values: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
                drop(data);
                overflow_clone.unmap();
                let _ = tx.send(values);
            });
            readback.stage = OverflowReadbackStage::PendingOverflow { rx };
        }
        OverflowReadbackStage::PendingOverflow { rx } => match rx.try_recv() {
            Ok(overflows) => {
                stats.global_overflow_buckets =
                    overflows.iter().filter(|&&v| v > 0).count() as u32;
                stats.overflow_pending = overflows.iter().sum::<u32>() > 0;

                if let Some(counters_readback) = state.buffers.counters_readback_buffer.as_ref() {
                    let (tx, counters_rx) = std::sync::mpsc::channel();
                    let counters_clone = counters_readback.clone();
                    counters_readback.slice(..).map_async(MapMode::Read, move |result| {
                        if let Err(err) = result {
                            tracing::warn!("voxel counters readback map failed: {err:?}");
                            return;
                        }
                        let data = counters_clone.slice(..).get_mapped_range();
                        let values: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
                        drop(data);
                        counters_clone.unmap();
                        let _ = tx.send(values);
                    });
                    readback.stage = OverflowReadbackStage::PendingCounters {
                        overflow: overflows,
                        rx: counters_rx,
                    };
                } else {
                    start_scratch_readback(
                        &store.scratch_overflow_readback_buffer,
                        overflows,
                        &mut readback.stage,
                    );
                }
            }
            Err(TryRecvError::Empty) => {
                readback.stage = OverflowReadbackStage::PendingOverflow { rx };
            }
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("voxel overflow readback channel disconnected");
                readback.stage = OverflowReadbackStage::Idle;
            }
        },
        OverflowReadbackStage::PendingCounters { overflow, rx } => match rx.try_recv() {
            Ok(counters) => {
                stats_from_counters(&counters, &mut stats);
                start_scratch_readback(
                    &store.scratch_overflow_readback_buffer,
                    overflow,
                    &mut readback.stage,
                );
            }
            Err(TryRecvError::Empty) => {
                readback.stage = OverflowReadbackStage::PendingCounters { overflow, rx };
            }
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("voxel counters readback channel disconnected");
                readback.stage = OverflowReadbackStage::Idle;
            }
        },
        OverflowReadbackStage::PendingScratch { overflow, rx } => match rx.try_recv() {
            Ok(scratch_overflows) => {
                stats.scratch_overflow_chunks =
                    scratch_overflows.iter().filter(|&&v| v > 0).count() as u32;
                stats.scratch_overflow_instances = scratch_overflows.iter().sum();

                if stats.scratch_overflow_chunks > 0 {
                    tracing::warn!(
                        chunks = stats.scratch_overflow_chunks,
                        instances = stats.scratch_overflow_instances,
                        "chunk scratch instance overflow"
                    );
                    if let Some(feedback) = feedback.as_mut() {
                        for (slot, &count) in scratch_overflows.iter().enumerate() {
                            if count > 0 {
                                if let Some(pos) = store.pos_for_slot(slot as u32) {
                                    if !feedback.scratch_overflow.contains(&pos) {
                                        feedback.scratch_overflow.push(pos);
                                    }
                                    store.data_dirty.insert(pos);
                                }
                            }
                        }
                    }
                }

                let total_overflow: u32 = overflow.iter().sum();
                if total_overflow > 0 {
                    let grown =
                        grow_instance_buffers(&render_device, tables.as_ref(), &mut pool, &overflow);
                    pool.generation = pool.generation.wrapping_add(1);
                    state.buffers = grown;
                    store.mark_all_dirty();
                    bind_cache.reset_for_growth(pool.generation);
                    if let Some(overflow_buf) = state.buffers.overflow_counters_buffer.as_ref() {
                        let zeros = vec![0u32; bucket_slots];
                        render_queue.write_buffer(overflow_buf, 0, bytemuck::cast_slice(&zeros));
                    }
                }
                stats.overflow_pending = false;
                readback.stage = OverflowReadbackStage::Idle;
            }
            Err(TryRecvError::Empty) => {
                readback.stage = OverflowReadbackStage::PendingScratch { overflow, rx };
            }
            Err(TryRecvError::Disconnected) => {
                tracing::warn!("voxel scratch readback channel disconnected");
                readback.stage = OverflowReadbackStage::Idle;
            }
        },
    }
}

fn stats_from_counters(counters: &[u32], stats: &mut GpuVoxelStats) {
    stats.total_instances = counters.iter().sum();
    stats.instance_counts = counters.to_vec();
}

fn start_scratch_readback(
    scratch_readback: &bevy::render::render_resource::Buffer,
    overflow: Vec<u32>,
    stage: &mut OverflowReadbackStage,
) {
    let (tx, scratch_rx) = std::sync::mpsc::channel();
    let scratch_clone = scratch_readback.clone();
    scratch_readback.slice(..).map_async(MapMode::Read, move |result| {
        if let Err(err) = result {
            tracing::warn!("voxel scratch overflow readback map failed: {err:?}");
            return;
        }
        let data = scratch_clone.slice(..).get_mapped_range();
        let values: Vec<u32> = bytemuck::cast_slice(&data).to_vec();
        drop(data);
        scratch_clone.unmap();
        let _ = tx.send(values);
    });
    *stage = OverflowReadbackStage::PendingScratch {
        overflow,
        rx: scratch_rx,
    };
}
