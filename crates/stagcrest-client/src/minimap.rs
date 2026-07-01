use std::collections::HashSet;

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::Task;
use stagcrest_minimap::{
    ColumnBlockSource, ColumnCache, ColumnResolver, ColumnSlice, BlockDefTable,
    DEFAULT_MAX_CACHE_COLUMNS, DirtyRegion, MinimapBiomeClimate, MinimapColorCtxOwned,
    MinimapFramebuffer, UNLOADED_COLOR,
};
use stagcrest_protocol::{BlockDef, BlockId, BlockPos, ChunkPos, CHUNK_SIZE};
use stagcrest_world::World;

use crate::chunk_streaming::BiomeGridCache;
use crate::game::{AppState, ModContext};
use crate::mesh_scheduler::poll_future_now;
use crate::player::FlyCamera;
use crate::ui::UiTheme;
use crate::world_replica::WorldReplica;

pub const TEX_SIZE: u32 = 128;
const ZOOM_LEVELS: [u32; 5] = [1, 2, 4, 8, 16];
const Y_MIN: i32 = 0;
const Y_MAX: i32 = 256;
const MAX_COLUMNS_PER_BUILD: usize = 256;
const CACHE_EVICT_RADIUS: i32 = 256;

pub struct MinimapPlugin;

struct MinimapResolverBundle {
    table: BlockDefTable,
    color_ctx: MinimapColorCtxOwned,
    air: BlockId,
    biomes: std::collections::HashMap<u16, MinimapBiomeClimate>,
    y_min: i32,
    y_max: i32,
}

#[derive(Resource)]
pub struct MinimapColumnCache(pub ColumnCache);

#[derive(Resource)]
pub struct MinimapState {
    pub visible: bool,
    pub zoom_index: usize,
    pub texture: Handle<Image>,
    pub framebuffer: MinimapFramebuffer,
    resolver: MinimapResolverBundle,
    pending_columns: HashSet<(i32, i32)>,
    in_flight: Option<Task<Vec<(i32, i32, [u8; 3], bool)>>>,
    dirty: DirtyRegion,
    needs_upload: bool,
    last_center: BlockPos,
}

#[derive(Component)]
pub(crate) struct MinimapRoot;

#[derive(Component)]
struct MinimapImage;

#[derive(Component)]
struct MinimapMarker;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_minimap)
            .add_systems(
                Update,
                (
                    toggle_minimap,
                    zoom_minimap,
                    minimap_schedule,
                    minimap_apply,
                )
                    .run_if(in_state(AppState::InGame).or_else(in_state(AppState::Paused))),
            );
    }
}

impl MinimapState {
    /// Re-resolve horizontal columns for a chunk that overlaps the current view.
    pub fn refresh_chunk_in_view(
        &mut self,
        cache: &mut MinimapColumnCache,
        pos: ChunkPos,
    ) {
        if !self.visible {
            return;
        }
        let cols = chunk_columns_in_view(pos.x, pos.z, &self.framebuffer);
        if cols.is_empty() {
            return;
        }
        for (wx, wz) in &cols {
            cache.0.remove_column(*wx, *wz);
            self.pending_columns.insert((*wx, *wz));
        }
        let strip = DirtyRegion::strip(
            pos.x,
            pos.z,
            self.framebuffer.world_x0(),
            self.framebuffer.world_z0(),
            self.framebuffer.bpp,
            TEX_SIZE,
        );
        merge_dirty(&mut self.dirty, strip);
    }

    /// Drop cached columns for an unloaded chunk and repaint its strip as unloaded.
    pub fn unload_chunk(
        &mut self,
        cache: &mut MinimapColumnCache,
        pos: ChunkPos,
    ) {
        if !self.visible {
            return;
        }
        let cols = chunk_columns_in_view(pos.x, pos.z, &self.framebuffer);
        for (wx, wz) in &cols {
            cache.0.remove_column(*wx, *wz);
            self.pending_columns.remove(&(*wx, *wz));
        }
        if cols.is_empty() {
            return;
        }
        let strip = DirtyRegion::strip(
            pos.x,
            pos.z,
            self.framebuffer.world_x0(),
            self.framebuffer.world_z0(),
            self.framebuffer.bpp,
            TEX_SIZE,
        );
        self.framebuffer.composite_region(&cache.0, &strip);
        self.needs_upload = true;
    }

    /// Re-resolve a single column after a block change.
    pub fn refresh_column(
        &mut self,
        cache: &mut MinimapColumnCache,
        wx: i32,
        wz: i32,
    ) {
        if !self.visible {
            return;
        }
        if !column_in_view(wx, wz, &self.framebuffer) {
            return;
        }
        cache.0.remove_column(wx, wz);
        self.pending_columns.insert((wx, wz));
        let px = ((wx - self.framebuffer.world_x0()) / self.framebuffer.bpp as i32) as u32;
        let pz = ((wz - self.framebuffer.world_z0()) / self.framebuffer.bpp as i32) as u32;
        if px < TEX_SIZE && pz < TEX_SIZE {
            merge_dirty(
                &mut self.dirty,
                DirtyRegion {
                    full: false,
                    rects: vec![(px, pz, px, pz)],
                },
            );
        }
    }
}

pub(crate) fn cleanup_minimap(
    commands: &mut Commands,
    roots: &Query<Entity, With<MinimapRoot>>,
) {
    for entity in roots {
        commands.entity(entity).despawn();
    }
    commands.remove_resource::<MinimapState>();
    commands.remove_resource::<MinimapColumnCache>();
}

fn cpu_pool() -> &'static bevy::tasks::TaskPool {
    bevy::tasks::AsyncComputeTaskPool::get()
}

struct WorldColumnSource<'a> {
    world: &'a World,
}

impl ColumnBlockSource for WorldColumnSource<'_> {
    fn block_at(&self, wx: i32, wy: i32, wz: i32) -> (BlockId, stagcrest_protocol::BlockState) {
        self.world.get_block(BlockPos::new(wx, wy, wz))
    }
}

fn build_resolver_bundle(mod_ctx: &ModContext) -> MinimapResolverBundle {
    let block_defs: Vec<BlockDef> = mod_ctx
        .registry
        .block_ids()
        .into_iter()
        .filter_map(|id| mod_ctx.registry.block(id).cloned())
        .collect();
    let air = mod_ctx
        .registry
        .block_by_name("stagcrest:air")
        .unwrap_or(BlockId(0));
    let mut biomes = std::collections::HashMap::new();
    for b in &mod_ctx.biomes.biomes {
        biomes.insert(
            b.index,
            MinimapBiomeClimate {
                temperature: b.temperature,
                downfall: b.downfall,
                grass_color: b.grass_color,
                foliage_color: b.foliage_color,
                water_color: Some(b.water_color),
            },
        );
    }
    MinimapResolverBundle {
        table: BlockDefTable::new(block_defs),
        color_ctx: MinimapColorCtxOwned::new(
            mod_ctx.colormaps.grass.clone(),
            mod_ctx.colormaps.grass_w,
            mod_ctx.colormaps.grass_h,
            mod_ctx.colormaps.foliage.clone(),
            mod_ctx.colormaps.foliage_w,
            mod_ctx.colormaps.foliage_h,
            mod_ctx.colormaps.water.clone(),
            mod_ctx.colormaps.water_w,
            mod_ctx.colormaps.water_h,
        ),
        air,
        biomes,
        y_min: Y_MIN,
        y_max: Y_MAX,
    }
}

fn fill_unloaded_rgba(len: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; len];
    for px in rgba.chunks_mut(4) {
        px[0] = UNLOADED_COLOR[0];
        px[1] = UNLOADED_COLOR[1];
        px[2] = UNLOADED_COLOR[2];
        px[3] = 255;
    }
    rgba
}

fn spawn_minimap(
    mut commands: Commands,
    theme: Res<UiTheme>,
    mod_ctx: Res<ModContext>,
    mut images: ResMut<Assets<Image>>,
) {
    let resolver = build_resolver_bundle(&mod_ctx);
    let rgba = fill_unloaded_rgba((TEX_SIZE * TEX_SIZE * 4) as usize);
    let texture = images.add(Image::new(
        Extent3d {
            width: TEX_SIZE,
            height: TEX_SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        default(),
    ));

    commands.insert_resource(MinimapColumnCache(ColumnCache::new(
        DEFAULT_MAX_CACHE_COLUMNS,
    )));
    commands.insert_resource(MinimapState {
        visible: false,
        zoom_index: 0,
        texture: texture.clone(),
        framebuffer: MinimapFramebuffer::new(TEX_SIZE, 0, 0, ZOOM_LEVELS[0]),
        resolver,
        pending_columns: HashSet::new(),
        in_flight: None,
        dirty: DirtyRegion::full(),
        needs_upload: false,
        last_center: BlockPos::new(0, 0, 0),
    });

    commands
        .spawn((
            MinimapRoot,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(12.0),
                right: Val::Px(12.0),
                width: Val::Px(160.0),
                height: Val::Px(160.0),
                padding: UiRect::all(Val::Px(8.0)),
                border_radius: BorderRadius::all(Val::Px(4.0)),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(theme.panel_bg),
            Visibility::Hidden,
        ))
        .with_children(|panel| {
            panel.spawn((
                MinimapImage,
                ImageNode::new(texture).with_mode(NodeImageMode::Auto),
                Node {
                    width: Val::Px(TEX_SIZE as f32),
                    height: Val::Px(TEX_SIZE as f32),
                    ..default()
                },
            ));
            panel.spawn((
                MinimapMarker,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    left: Val::Px(76.0),
                    top: Val::Px(76.0),
                    border_radius: BorderRadius::all(Val::Px(4.0)),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.9, 0.2, 0.2)),
            ));
        });
}

fn toggle_minimap(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<MinimapState>,
    mut roots: Query<&mut Visibility, With<MinimapRoot>>,
    camera: Query<&Transform, With<FlyCamera>>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        state.visible = !state.visible;
        if state.visible {
            if let Ok(cam) = camera.single() {
                state.last_center = BlockPos::new(
                    cam.translation.x.floor() as i32,
                    cam.translation.y.floor() as i32,
                    cam.translation.z.floor() as i32,
                );
                state.framebuffer.center_x = state.last_center.x;
                state.framebuffer.center_z = state.last_center.z;
            }
            state.dirty = DirtyRegion::full();
            let (wx0, wz0, bpp, size) = (
                state.framebuffer.world_x0(),
                state.framebuffer.world_z0(),
                state.framebuffer.bpp,
                state.framebuffer.size,
            );
            queue_view_columns(&mut state.pending_columns, wx0, wz0, bpp, size);
        } else {
            state.in_flight = None;
            state.pending_columns.clear();
        }
        for mut vis in &mut roots {
            *vis = if state.visible {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        }
    }
}

fn zoom_minimap(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<MinimapState>,
    cache: Res<MinimapColumnCache>,
) {
    if !state.visible {
        return;
    }
    let old_bpp = ZOOM_LEVELS[state.zoom_index];
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        state.zoom_index = (state.zoom_index + 1).min(ZOOM_LEVELS.len() - 1);
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        state.zoom_index = state.zoom_index.saturating_sub(1);
    }
    let new_bpp = ZOOM_LEVELS[state.zoom_index];
    if new_bpp != old_bpp {
        state.framebuffer.set_bpp(new_bpp);
        state.framebuffer.composite_full_mode(&cache.0, false);
        state.needs_upload = true;
        state.dirty = DirtyRegion::default();
    }
}

fn minimap_schedule(
    mut state: ResMut<MinimapState>,
    mut cache: ResMut<MinimapColumnCache>,
    world: Res<WorldReplica>,
    mod_ctx: Res<ModContext>,
    biomes: Res<BiomeGridCache>,
    camera: Query<&Transform, With<FlyCamera>>,
) {
    if !state.visible || state.in_flight.is_some() {
        return;
    }

    let Ok(cam) = camera.single() else {
        return;
    };
    let center = BlockPos::new(
        cam.translation.x.floor() as i32,
        cam.translation.y.floor() as i32,
        cam.translation.z.floor() as i32,
    );

    let dx = center.x - state.last_center.x;
    let dz = center.z - state.last_center.z;
    if dx != 0 || dz != 0 {
        let scroll_dirty = state.framebuffer.scroll(dx, dz);
        if scroll_dirty.full {
            state.framebuffer.composite_full_mode(&cache.0, true);
        } else {
            state.framebuffer.composite_region_mode(&cache.0, &scroll_dirty, true);
        }
        state.needs_upload = true;
        let (wx0, wz0, bpp, size) = (
            state.framebuffer.world_x0(),
            state.framebuffer.world_z0(),
            state.framebuffer.bpp,
            state.framebuffer.size,
        );
        queue_edge_columns(&mut state.pending_columns, wx0, wz0, bpp, size, dx, dz);
        state.last_center = center;
        cache.0.evict_far_from(center.x, center.z, CACHE_EVICT_RADIUS);
    }

    if state.pending_columns.is_empty() {
        return;
    }

    // Refresh colormap/biome tables if mods could change (cheap clone of refs).
    state.resolver = build_resolver_bundle(&mod_ctx);

    let batch: Vec<(i32, i32)> = state
        .pending_columns
        .iter()
        .take(MAX_COLUMNS_PER_BUILD)
        .copied()
        .collect();
    for key in &batch {
        state.pending_columns.remove(key);
    }

    let source = WorldColumnSource {
        world: &world.world,
    };
    let slices: Vec<ColumnSlice> = batch
        .iter()
        .map(|(wx, wz)| {
            let loaded = column_has_loaded(&world.world, *wx, *wz, Y_MIN, Y_MAX);
            let biome = biome_at_block(
                *wx,
                Y_MAX,
                *wz,
                &biomes,
                &mod_ctx,
                &state.resolver.biomes,
            );
            ColumnSlice::capture(&source, *wx, *wz, Y_MIN, Y_MAX, biome, loaded)
        })
        .collect();

    let bundle = ResolverSnapshot {
        table: state.resolver.table.defs().to_vec(),
        color_ctx: state.resolver.color_ctx.clone(),
        air: state.resolver.air,
        y_min: state.resolver.y_min,
        y_max: state.resolver.y_max,
    };

    state.in_flight = Some(cpu_pool().spawn(async move {
        let table = BlockDefTable::new(bundle.table);
        let ctx = bundle.color_ctx.as_ctx();
        let resolver = ColumnResolver {
            table: &table,
            air: bundle.air,
            y_min: bundle.y_min,
            y_max: bundle.y_max,
            color_ctx: ctx,
        };
        resolver.resolve_batch(&slices)
    }));
}

fn minimap_apply(
    mut state: ResMut<MinimapState>,
    mut cache: ResMut<MinimapColumnCache>,
    mut images: ResMut<Assets<Image>>,
) {
    if let Some(task) = state.in_flight.as_mut() {
        if let Some(results) = poll_future_now(task) {
            state.in_flight = None;
            let batch_keys: Vec<(i32, i32)> =
                results.iter().map(|(wx, wz, _, _)| (*wx, *wz)).collect();
            for (wx, wz, rgb, loaded) in results {
                cache.0.insert(wx, wz, rgb, loaded);
            }
            let batch_dirty = dirty_for_columns(
                &batch_keys,
                state.framebuffer.world_x0(),
                state.framebuffer.world_z0(),
                state.framebuffer.bpp,
                state.framebuffer.size,
            );
            let was_dirty_full = state.dirty.full;
            if state.dirty.full {
                // Initial open: paint from cache (may show unloaded). Later regen: keep prior image.
                state
                    .framebuffer
                    .composite_full_mode(&cache.0, !was_dirty_full);
            } else {
                let mut dirty = state.dirty.clone();
                merge_dirty(&mut dirty, batch_dirty.clone());
                if dirty.full {
                    state.framebuffer.composite_full_mode(&cache.0, true);
                } else if !dirty.rects.is_empty() {
                    state.framebuffer.composite_region_mode(&cache.0, &dirty, true);
                } else if !batch_dirty.rects.is_empty() {
                    state.framebuffer.composite_region_mode(&cache.0, &batch_dirty, true);
                }
            }
            state.dirty = DirtyRegion::default();
            state.needs_upload = true;
        }
    }

    if state.visible && state.needs_upload {
        let len = state.framebuffer.data.len();
        if let Some(mut image) = images.get_mut(&state.texture) {
            match &mut image.data {
                Some(data) if data.len() == len => {
                    data.copy_from_slice(&state.framebuffer.data);
                }
                _ => image.data = Some(state.framebuffer.data.clone()),
            }
        }
        state.needs_upload = false;
    }
}

struct ResolverSnapshot {
    table: Vec<BlockDef>,
    color_ctx: MinimapColorCtxOwned,
    air: BlockId,
    y_min: i32,
    y_max: i32,
}

fn column_has_loaded(world: &World, wx: i32, wz: i32, y_min: i32, y_max: i32) -> bool {
    let cx = stagcrest_protocol::floor_div(wx, CHUNK_SIZE);
    let cz = stagcrest_protocol::floor_div(wz, CHUNK_SIZE);
    let cy_min = stagcrest_protocol::floor_div(y_min, CHUNK_SIZE);
    let cy_max = stagcrest_protocol::floor_div(y_max, CHUNK_SIZE);
    (cy_min..=cy_max).any(|cy| world.has_chunk(ChunkPos { x: cx, y: cy, z: cz }))
}

fn biome_at_block(
    wx: i32,
    wy: i32,
    wz: i32,
    biomes: &BiomeGridCache,
    mod_ctx: &ModContext,
    climate_table: &std::collections::HashMap<u16, MinimapBiomeClimate>,
) -> MinimapBiomeClimate {
    let pos = BlockPos::new(wx, wy, wz);
    let grid = biomes.biome_grid(pos.chunk_pos());
    let idx = grid.and_then(|g| {
        mod_ctx
            .biomes
            .sample_at(pos, Some(g))
            .map(|b| b.index)
    });
    idx.and_then(|i| climate_table.get(&i).copied())
        .unwrap_or_default()
}

fn queue_view_columns(
    pending: &mut HashSet<(i32, i32)>,
    world_x0: i32,
    world_z0: i32,
    bpp: u32,
    size: u32,
) {
    let span = size as i32 * bpp as i32;
    for dz in 0..span {
        for dx in 0..span {
            pending.insert((world_x0 + dx, world_z0 + dz));
        }
    }
}

fn queue_edge_columns(
    pending: &mut HashSet<(i32, i32)>,
    world_x0: i32,
    world_z0: i32,
    bpp: u32,
    size: u32,
    dx: i32,
    dz: i32,
) {
    let span = (size * bpp) as i32;
    let world_x1 = world_x0 + span - 1;
    let world_z1 = world_z0 + span - 1;

    if dx > 0 {
        let wx0 = (world_x1 - dx.abs() + 1).max(world_x0);
        for wx in wx0..=world_x1 {
            for wz in world_z0..=world_z1 {
                pending.insert((wx, wz));
            }
        }
    } else if dx < 0 {
        let wx1 = (world_x0 + dx.abs() - 1).min(world_x1);
        for wx in world_x0..=wx1 {
            for wz in world_z0..=world_z1 {
                pending.insert((wx, wz));
            }
        }
    }

    if dz > 0 {
        let wz0 = (world_z1 - dz.abs() + 1).max(world_z0);
        for wz in wz0..=world_z1 {
            for wx in world_x0..=world_x1 {
                pending.insert((wx, wz));
            }
        }
    } else if dz < 0 {
        let wz1 = (world_z0 + dz.abs() - 1).min(world_z1);
        for wz in world_z0..=wz1 {
            for wx in world_x0..=world_x1 {
                pending.insert((wx, wz));
            }
        }
    }
}

fn chunk_columns_in_view(cx: i32, cz: i32, fb: &MinimapFramebuffer) -> Vec<(i32, i32)> {
    let wx0 = fb.world_x0();
    let wz0 = fb.world_z0();
    let wx1 = wx0 + fb.size as i32 * fb.bpp as i32 - 1;
    let wz1 = wz0 + fb.size as i32 * fb.bpp as i32 - 1;
    let base_x = cx * CHUNK_SIZE;
    let base_z = cz * CHUNK_SIZE;
    let mut cols = Vec::new();
    for lz in 0..CHUNK_SIZE {
        for lx in 0..CHUNK_SIZE {
            let wx = base_x + lx;
            let wz = base_z + lz;
            if wx >= wx0 && wx <= wx1 && wz >= wz0 && wz <= wz1 {
                cols.push((wx, wz));
            }
        }
    }
    cols
}

fn column_in_view(wx: i32, wz: i32, fb: &MinimapFramebuffer) -> bool {
    let wx0 = fb.world_x0();
    let wz0 = fb.world_z0();
    let wx1 = wx0 + fb.size as i32 * fb.bpp as i32 - 1;
    let wz1 = wz0 + fb.size as i32 * fb.bpp as i32 - 1;
    wx >= wx0 && wx <= wx1 && wz >= wz0 && wz <= wz1
}

fn merge_dirty(target: &mut DirtyRegion, add: DirtyRegion) {
    if add.full {
        *target = DirtyRegion::full();
        return;
    }
    if target.full {
        return;
    }
    target.rects.extend(add.rects);
}

fn dirty_for_columns(
    columns: &[(i32, i32)],
    world_x0: i32,
    world_z0: i32,
    bpp: u32,
    size: u32,
) -> DirtyRegion {
    let mut rects = Vec::with_capacity(columns.len());
    for &(wx, wz) in columns {
        let px = (wx - world_x0) / bpp as i32;
        let pz = (wz - world_z0) / bpp as i32;
        if px < 0 || pz < 0 {
            continue;
        }
        let px = px as u32;
        let pz = pz as u32;
        if px < size && pz < size {
            rects.push((px, pz, px, pz));
        }
    }
    DirtyRegion {
        full: false,
        rects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_levels_cover_range() {
        assert_eq!(ZOOM_LEVELS[0], 1);
        assert_eq!(ZOOM_LEVELS[4], 16);
    }
}
