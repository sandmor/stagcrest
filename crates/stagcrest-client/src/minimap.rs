use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::{AsyncComputeTaskPool, Task};
use stagcrest_minimap::{
    build_tile_mips, composite_tiles_into_framebuffer, dirty_region_for_map_tile,
    visible_map_tiles, DirtyRegion, MapChunkBlob, MapTileRgb, MinimapFramebuffer,
    MINIMAP_PREFETCH_MAP_CHUNKS, MINIMAP_TEX_SIZE, MINIMAP_ZOOM_LEVELS, UNLOADED_COLOR,
};
use stagcrest_net::MapViewSubscribe;
use stagcrest_protocol::BlockPos;

use crate::game::AppState;
use crate::mesh_scheduler::poll_future_now;
use crate::net_client::GameNetClient;
use crate::player::FlyCamera;
use crate::ui::UiTheme;

pub const TEX_SIZE: u32 = MINIMAP_TEX_SIZE;
pub const ZOOM_LEVELS: [u32; 5] = MINIMAP_ZOOM_LEVELS;
const MAX_DECODE_IN_FLIGHT: usize = 8;
const MAX_PENDING_DECODE: usize = 64;

pub struct MinimapPlugin;

#[derive(Resource, Default)]
pub struct MapTileCache(pub HashMap<(i32, i32), MapTileRgb>);

#[derive(Resource, Default)]
pub struct MinimapDecodeQueue {
    pending: VecDeque<(i32, i32, Vec<u8>)>,
    in_flight: Vec<Task<Option<(i32, i32, MapTileRgb)>>>,
}

impl MinimapDecodeQueue {
    pub fn enqueue(&mut self, mx: i32, mz: i32, compressed: Vec<u8>) {
        if let Some(pos) = self
            .pending
            .iter()
            .position(|&(x, z, _)| x == mx && z == mz)
        {
            self.pending[pos] = (mx, mz, compressed);
            return;
        }
        if self.pending.len() >= MAX_PENDING_DECODE {
            if let Some((x, z, _)) = self.pending.pop_front() {
                let _ = (x, z);
            }
        }
        self.pending.push_back((mx, mz, compressed));
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn pump(&mut self) {
        while self.in_flight.len() < MAX_DECODE_IN_FLIGHT {
            let Some((mx, mz, blob)) = self.pending.pop_front() else {
                break;
            };
            let task = AsyncComputeTaskPool::get().spawn(async move { decode_tile(mx, mz, &blob) });
            self.in_flight.push(task);
        }
    }

    pub fn drain_ready(&mut self) -> Vec<(i32, i32, MapTileRgb)> {
        let mut ready = Vec::new();
        self.in_flight.retain_mut(|task| {
            if let Some(result) = poll_future_now(task) {
                if let Some(tile) = result {
                    ready.push(tile);
                }
                false
            } else {
                true
            }
        });
        ready
    }
}

fn decode_tile(mx: i32, mz: i32, compressed: &[u8]) -> Option<(i32, i32, MapTileRgb)> {
    let layers = match MapChunkBlob::decode(compressed) {
        Ok(layers) => layers,
        Err(err) => {
            tracing::warn!("minimap decode failed for ({mx}, {mz}): {err}");
            return None;
        }
    };
    let layer = layers.first()?;
    Some((mx, mz, build_tile_mips(layer.rgb)))
}

#[derive(Resource)]
pub struct MinimapState {
    pub visible: bool,
    pub zoom_index: usize,
    pub texture: Handle<Image>,
    pub framebuffer: MinimapFramebuffer,
    last_subscribed_tiles: HashSet<(i32, i32)>,
    dirty: DirtyRegion,
    needs_upload: bool,
    needs_subscribe: bool,
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
                    minimap_decode_apply.after(crate::game::net_poll_system),
                    minimap_track_camera,
                    minimap_subscribe,
                    minimap_apply.after(minimap_decode_apply),
                )
                    .run_if(in_state(AppState::InGame).or_else(in_state(AppState::Paused))),
            );
    }
}

impl MinimapState {
    fn current_tile_set(&self) -> HashSet<(i32, i32)> {
        visible_map_tiles(
            self.framebuffer.center_x,
            self.framebuffer.center_z,
            self.framebuffer.bpp,
            TEX_SIZE,
            MINIMAP_PREFETCH_MAP_CHUNKS,
        )
        .into_iter()
        .collect()
    }

    fn mark_subscribe_if_needed(&mut self, tiles: &mut MapTileCache) {
        let set = self.current_tile_set();
        if !self.visible {
            if !self.last_subscribed_tiles.is_empty() {
                self.needs_subscribe = true;
            }
            return;
        }
        if set != self.last_subscribed_tiles {
            tiles.0.retain(|key, _| set.contains(key));
            self.last_subscribed_tiles = set;
            self.needs_subscribe = true;
        }
    }

    fn build_subscribe_message(&mut self) -> Option<MapViewSubscribe> {
        if !self.needs_subscribe {
            return None;
        }
        self.needs_subscribe = false;
        if !self.visible {
            self.last_subscribed_tiles.clear();
            return Some(MapViewSubscribe {
                active: false,
                center_x: self.framebuffer.center_x,
                center_z: self.framebuffer.center_z,
                bpp: self.framebuffer.bpp,
                tiles: Vec::new(),
            });
        }
        let tiles: Vec<(i32, i32)> = self.current_tile_set().into_iter().collect();
        Some(MapViewSubscribe {
            active: true,
            center_x: self.framebuffer.center_x,
            center_z: self.framebuffer.center_z,
            bpp: self.framebuffer.bpp,
            tiles,
        })
    }

    fn merge_tile_dirty(&mut self, mx: i32, mz: i32) {
        self.dirty.merge(dirty_region_for_map_tile(
            mx,
            mz,
            self.framebuffer.world_x0(),
            self.framebuffer.world_z0(),
            self.framebuffer.bpp,
            TEX_SIZE,
        ));
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
    commands.remove_resource::<MapTileCache>();
    commands.remove_resource::<MinimapDecodeQueue>();
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
    mut images: ResMut<Assets<Image>>,
) {
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

    commands.insert_resource(MapTileCache::default());
    commands.insert_resource(MinimapDecodeQueue::default());
    commands.insert_resource(MinimapState {
        visible: false,
        zoom_index: 0,
        texture: texture.clone(),
        framebuffer: MinimapFramebuffer::new(TEX_SIZE, 0, 0, ZOOM_LEVELS[0]),
        last_subscribed_tiles: HashSet::new(),
        dirty: DirtyRegion::full(),
        needs_upload: false,
        needs_subscribe: false,
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
    mut tiles: ResMut<MapTileCache>,
    mut queue: ResMut<MinimapDecodeQueue>,
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
            state.mark_subscribe_if_needed(&mut tiles);
        } else {
            state.mark_subscribe_if_needed(&mut tiles);
            queue.clear();
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
    mut tiles: ResMut<MapTileCache>,
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
        state.dirty = DirtyRegion::full();
        state.mark_subscribe_if_needed(&mut tiles);
    }
}

fn minimap_decode_apply(
    mut queue: ResMut<MinimapDecodeQueue>,
    mut state: ResMut<MinimapState>,
    mut tiles: ResMut<MapTileCache>,
) {
    if !state.visible {
        return;
    }
    queue.pump();
    for (mx, mz, tile) in queue.drain_ready() {
        tiles.0.insert((mx, mz), tile);
        if state.visible {
            state.merge_tile_dirty(mx, mz);
        }
    }
}

fn minimap_track_camera(
    mut state: ResMut<MinimapState>,
    mut tiles: ResMut<MapTileCache>,
    camera: Query<&Transform, With<FlyCamera>>,
) {
    if !state.visible {
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
    if dx == 0 && dz == 0 {
        return;
    }
    let scroll_dirty = state.framebuffer.scroll(dx, dz);
    if !scroll_dirty.is_empty() {
        composite_tiles_into_framebuffer(
            &mut state.framebuffer,
            &tiles.0,
            &scroll_dirty,
            !scroll_dirty.full,
        );
        state.needs_upload = true;
    }
    state.last_center = center;
    state.mark_subscribe_if_needed(&mut tiles);
}

fn minimap_subscribe(
    mut state: ResMut<MinimapState>,
    mut tiles: ResMut<MapTileCache>,
    mut queue: ResMut<MinimapDecodeQueue>,
    mut net: ResMut<GameNetClient>,
) {
    let Some(sub) = state.build_subscribe_message() else {
        return;
    };
    if !sub.active {
        tiles.0.clear();
        queue.clear();
    }
    net.send_map_view_subscribe(sub);
}

fn minimap_apply(
    mut state: ResMut<MinimapState>,
    tiles: Res<MapTileCache>,
    mut images: ResMut<Assets<Image>>,
) {
    if state.visible && !state.dirty.is_empty() {
        let region = std::mem::take(&mut state.dirty);
        composite_tiles_into_framebuffer(
            &mut state.framebuffer,
            &tiles.0,
            &region,
            false,
        );
        state.needs_upload = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_levels_cover_range() {
        assert_eq!(ZOOM_LEVELS[0], 1);
        assert_eq!(ZOOM_LEVELS[4], 16);
    }
}
