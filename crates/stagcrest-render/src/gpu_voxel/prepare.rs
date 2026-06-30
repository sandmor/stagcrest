use bevy::prelude::*;
use bevy::render::render_resource::{CommandEncoderDescriptor, MapMode, PollType};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::MainWorld;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::gpu_voxel::chunk_cache::GpuChunkCache;
use crate::gpu_voxel::chunk_feedback::{GpuChunkRenderFeedback, GpuChunkSyncState};
use crate::gpu_voxel::chunk_gpu_pool::ChunkGpuPool;
use crate::gpu_voxel::chunk_scratch_arena::{ChunkScratchArena, ScratchFlushResult, ScratchGrowResult};
use crate::gpu_voxel::draw_node::VoxelDrawBindCache;
use crate::gpu_voxel::gpu_resources::{
    grow_chunk_table_buffer, grow_instance_buffers, init_render_gpu_buffers_with_config,
    GpuVoxelStats, GpuVoxelTables, InstanceBufferPool, RenderGpuVoxelBuffers,
};
use crate::gpu_voxel::extract_chunk::PendingChunkExtract;
use crate::gpu_voxel::memory_config::VoxelMemoryConfig;
use crate::gpu_voxel::rebuild_node::ChunkComputeBindCache;

#[derive(Resource)]
pub struct GpuVoxelRenderState {
    pub buffers: RenderGpuVoxelBuffers,
}

/// Tracks the render-distance capacity used for pool/scratch/table allocation.
#[derive(Resource)]
pub struct VoxelGpuAllocatedConfig {
    pub max_resident_chunks: u32,
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

pub fn prepare_gpu_voxel_buffers(
    tables: Option<Res<GpuVoxelTables>>,
    memory_config: Option<Res<VoxelMemoryConfig>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    existing: Option<Res<GpuVoxelRenderState>>,
    mut pending: Option<ResMut<PendingChunkExtract>>,
    mut commands: Commands,
) {
    if existing.is_some() {
        return;
    }
    let (Some(tables), Some(memory_config)) = (tables, memory_config) else {
        return;
    };
    let (buffers, pool) =
        init_render_gpu_buffers_with_config(&render_device, tables.as_ref(), &memory_config);
    let bucket_slot_count = buffers.bucket_slot_count;
    let mut chunk_pool = ChunkGpuPool::new(&memory_config, bucket_slot_count);
    let mut scratch = ChunkScratchArena::new(&render_device, &memory_config);
    if let Some(pending) = pending.as_mut() {
        let mut feedback = GpuChunkRenderFeedback::default();
        for batch in pending.batches.drain(..) {
            chunk_pool.apply_extract_batch(&batch, &mut feedback);
        }
        for &slot in chunk_pool.pos_to_slot.values() {
            scratch.init_slot(slot);
        }
        scratch.flush_layout(&render_device, &render_queue);
        if chunk_pool.resident_count() > 0 {
            chunk_pool.mark_all_dirty();
        }
    }
    commands.insert_resource(GpuVoxelRenderState { buffers });
    commands.insert_resource(chunk_pool);
    commands.insert_resource(scratch);
    commands.insert_resource(pool);
    commands.insert_resource(VoxelGpuAllocatedConfig {
        max_resident_chunks: memory_config.max_resident_chunks,
    });
    commands.init_resource::<ChunkComputeBindCache>();
}

/// Grows pool/scratch/table when render distance increases at runtime (no shrink).
pub fn resize_voxel_memory_if_needed(
    memory_config: Option<Res<VoxelMemoryConfig>>,
    allocated: Option<ResMut<VoxelGpuAllocatedConfig>>,
    state: Option<ResMut<GpuVoxelRenderState>>,
    pool: Option<ResMut<ChunkGpuPool>>,
    scratch: Option<ResMut<ChunkScratchArena>>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    mut compute_bind_cache: Option<ResMut<ChunkComputeBindCache>>,
    mut draw_bind_cache: Option<ResMut<VoxelDrawBindCache>>,
) {
    let (
        Some(memory_config),
        Some(mut allocated),
        Some(mut state),
        Some(mut pool),
        Some(mut scratch),
    ) = (
        memory_config,
        allocated,
        state,
        pool,
        scratch,
    ) else {
        return;
    };

    let target = memory_config.max_resident_chunks;
    if target <= allocated.max_resident_chunks {
        return;
    }

    tracing::info!(
        old = allocated.max_resident_chunks,
        new = target,
        "growing voxel GPU resident capacity"
    );

    pool.grow_capacity(target);
    state.buffers.chunk_table_buffer = grow_chunk_table_buffer(
        &render_device,
        &state.buffers.chunk_table_buffer,
        target,
    );
    render_queue.write_buffer(
        &state.buffers.chunk_table_buffer,
        0,
        bytemuck::cast_slice(&pool.chunk_table),
    );

    scratch.grow_capacity(&render_device, &render_queue, target);
    pool.mark_all_dirty();
    allocated.max_resident_chunks = target;

    if let Some(cache) = compute_bind_cache.as_mut() {
        cache.clear();
    }
    if let Some(cache) = draw_bind_cache.as_mut() {
        cache.invalidate();
    }
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
            &feedback.scratch_overflow,
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
    memory_config: Option<Res<VoxelMemoryConfig>>,
    mut state: Option<ResMut<GpuVoxelRenderState>>,
    pool: Option<ResMut<ChunkGpuPool>>,
    mut scratch: Option<ResMut<ChunkScratchArena>>,
    instance_pool: Option<ResMut<InstanceBufferPool>>,
    bind_cache: Option<ResMut<VoxelDrawBindCache>>,
    mut compute_bind_cache: Option<ResMut<ChunkComputeBindCache>>,
    stats: Option<ResMut<GpuVoxelStats>>,
    mut feedback: Option<ResMut<GpuChunkRenderFeedback>>,
    mut readback: Local<OverflowReadbackState>,
) {
    let _ = render_device.poll(PollType::Poll);

    let (
        Some(tables),
        Some(memory_config),
        Some(state),
        Some(mut pool),
        Some(scratch),
        Some(mut instance_pool),
        Some(mut bind_cache),
        Some(mut stats),
    ) = (
        tables,
        memory_config,
        state.as_mut(),
        pool,
        scratch.as_mut(),
        instance_pool,
        bind_cache,
        stats,
    )
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
            let poll_interval = if stats.scratch_overflow_chunks > 0 || stats.overflow_pending {
                memory_config.overflow_poll_interval_active
            } else {
                memory_config.overflow_poll_interval_idle
            };
            if readback.frames_since_poll % poll_interval != 0 {
                readback.stage = OverflowReadbackStage::Idle;
                return;
            }
            readback.frames_since_poll = 0;

            let byte_size = crate::gpu_voxel::gpu_resources::counters_buffer_size(
                state.buffers.bucket_slot_count,
            );
            let mut encoder = render_device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("voxel_overflow_readback_copy"),
            });
            encoder.copy_buffer_to_buffer(overflow_buf, 0, readback_buf, 0, byte_size);
            encoder.copy_buffer_to_buffer(
                &scratch.scratch_overflow,
                0,
                &scratch.scratch_overflow_readback,
                0,
                scratch.meta_byte_size,
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

                for (id, &count) in overflows.iter().enumerate() {
                    if id < instance_pool.peak.len() && count > 0 {
                        instance_pool.peak[id] =
                            instance_pool.peak[id].max(instance_pool.capacities[id]);
                    }
                }

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
                        &scratch.scratch_overflow_readback,
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
                stats_from_counters(&counters, &mut stats, Some(&mut instance_pool.peak));
                start_scratch_readback(
                    &scratch.scratch_overflow_readback,
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
                stats.scratch_arena_max_slot_capacity = scratch.arena_max_capacity;
                stats.scratch_arena_total_instances = scratch.total_instance_slots;

                let mut layout_changed = false;
                let mut budget_exceeded_chunks = 0u32;
                let mut budget_exceeded_instances = 0u32;
                if stats.scratch_overflow_chunks > 0 {
                    for (slot, &overflow_count) in scratch_overflows.iter().enumerate() {
                        if overflow_count == 0 {
                            continue;
                        }
                        let slot = slot as u32;
                        let current_cap = scratch.slot_capacities().get(slot as usize).copied().unwrap_or(0);
                        let needed = current_cap.saturating_add(overflow_count);
                        match scratch.plan_grow_slot(slot, needed) {
                            ScratchGrowResult::Grown => {
                                tracing::debug!(
                                    slot,
                                    chunk = ?pool.pos_for_slot(slot),
                                    needed,
                                    new_cap = scratch.slot_capacities()[slot as usize],
                                    "grew chunk scratch slot capacity"
                                );
                                if let Some(pos) = pool.pos_for_slot(slot) {
                                    pool.data_dirty.insert(pos);
                                }
                            }
                            ScratchGrowResult::AtCapacity => {
                                if current_cap >= scratch.scratch_max_per_chunk
                                    && needed > current_cap
                                {
                                    tracing::warn!(
                                        slot,
                                        chunk = ?pool.pos_for_slot(slot),
                                        needed,
                                        cap = scratch.scratch_max_per_chunk,
                                        "chunk scratch at per-chunk max; instances dropped"
                                    );
                                } else {
                                    tracing::debug!(
                                        slot,
                                        chunk = ?pool.pos_for_slot(slot),
                                        needed,
                                        current_cap,
                                        arena_allocated = scratch.allocated_instance_slots(),
                                        arena_max = scratch.arena_max_instance_slots(),
                                        "chunk scratch arena at instance cap; instances dropped"
                                    );
                                }
                                if let Some(pos) = pool.pos_for_slot(slot) {
                                    if let Some(feedback) = feedback.as_mut() {
                                        if !feedback.scratch_overflow.contains(&pos) {
                                            feedback.scratch_overflow.push(pos);
                                        }
                                    }
                                }
                            }
                            ScratchGrowResult::BudgetExceeded => {
                                budget_exceeded_chunks += 1;
                                budget_exceeded_instances += overflow_count;
                                if let Some(pos) = pool.pos_for_slot(slot) {
                                    if let Some(feedback) = feedback.as_mut() {
                                        if !feedback.scratch_overflow.contains(&pos) {
                                            feedback.scratch_overflow.push(pos);
                                        }
                                    }
                                }
                            }
                            ScratchGrowResult::Unchanged => {}
                        }
                    }
                    if budget_exceeded_chunks > 0 {
                        tracing::warn!(
                            chunks = budget_exceeded_chunks,
                            dropped_instances = budget_exceeded_instances,
                            budget_mb = scratch.scratch_arena_budget_bytes / (1024 * 1024),
                            arena_used = scratch.allocated_instance_slots(),
                            arena_max = scratch.arena_max_instance_slots(),
                            "chunk scratch arena at budget; some instances dropped"
                        );
                    }
                }

                match scratch.flush_layout(&render_device, &render_queue) {
                    ScratchFlushResult::BufferRecreated => {
                        layout_changed = true;
                        pool.mark_all_dirty();
                    }
                    ScratchFlushResult::Deferred => {
                        pool.mark_all_dirty();
                    }
                    ScratchFlushResult::LayoutOnly => {}
                    ScratchFlushResult::Unchanged => {}
                }

                if layout_changed {
                    stats.scratch_arena_max_slot_capacity = scratch.arena_max_capacity;
                    stats.scratch_arena_total_instances = scratch.total_instance_slots;
                    if let Some(compute_cache) = compute_bind_cache.as_mut() {
                        compute_cache.clear();
                    }
                }
                if scratch.take_compaction_occurred() {
                    pool.mark_all_dirty();
                    if let Some(compute_cache) = compute_bind_cache.as_mut() {
                        compute_cache.clear();
                    }
                }

                let total_overflow: u32 = overflow.iter().sum();
                if total_overflow > 0 {
                    let caps_before = instance_pool.capacities.clone();
                    let grown = grow_instance_buffers(
                        &render_device,
                        tables.as_ref(),
                        &mut instance_pool,
                        &overflow,
                        &state.buffers,
                    );
                    let draw_caps_grew = instance_pool
                        .capacities
                        .iter()
                        .zip(caps_before.iter())
                        .any(|(after, before)| after > before);
                    instance_pool.generation = instance_pool.generation.wrapping_add(1);
                    state.buffers = grown;
                    if draw_caps_grew {
                        pool.mark_all_dirty();
                        bind_cache.reset_for_growth(instance_pool.generation);
                        if let Some(compute_cache) = compute_bind_cache.as_mut() {
                            compute_cache.clear();
                        }
                    }
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

fn stats_from_counters(counters: &[u32], stats: &mut GpuVoxelStats, peak: Option<&mut [u32]>) {
    stats.total_instances = counters.iter().sum();
    stats.instance_counts = counters.to_vec();
    if let Some(peak) = peak {
        for (id, &count) in counters.iter().enumerate() {
            if id < peak.len() {
                peak[id] = peak[id].max(count);
            }
        }
        stats.peak_instances = peak.to_vec();
    }
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
