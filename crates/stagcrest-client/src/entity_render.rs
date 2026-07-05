//! Client-side Bedrock entity rendering + animation.
//!
//! Parses geometry/animations shipped by the server (via `stagcrest-bedrock`),
//! bakes per-bone meshes lazily, spawns a transform hierarchy per entity, and
//! animates bone transforms from the Bedrock clip. Also spawns one client-local
//! follower that tracks the camera without blocking the view.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use stagcrest_bedrock::{bedrock_rotation_to_quat, AnimationSet, BakedModel, Geometry};
use stagcrest_net::{EntitySpawn, EntityUpdate};
use stagcrest_protocol::{EntityAssetTransfer, EntityId, EntityManifest, EntityTypeId, EntityWireDef};
use stagcrest_render::{EntityMaterial, SceneLighting};

use crate::game::AppState;
use crate::game_session::{in_active_gameplay, GameCamera, GameSessionEntity};

pub struct EntityRenderPlugin;

impl Plugin for EntityRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<EntityNetEvent>()
            .init_resource::<EntityGpuCache>()
            .init_resource::<EntityIndex>()
            .add_systems(
                Update,
                (
                    apply_entity_net_events,
                    spawn_follower,
                    update_follower,
                    animate_entities,
                    push_scene_lighting_to_entities,
                )
                    .chain()
                    .run_if(in_active_gameplay),
            )
            .add_systems(OnExit(AppState::InGame), reset_entity_index);
    }
}

// --- CPU catalog (built at load time in loading.rs) ---

/// Parsed, ready-to-bake assets for one entity type.
pub struct EntityTypeAssets {
    pub wire: EntityWireDef,
    pub model: BakedModel,
    pub animations: AnimationSet,
    pub texture_png: Vec<u8>,
}

/// All entity types known this session. Inserted during loading.
#[derive(Resource, Default)]
pub struct EntityCatalog {
    pub types: HashMap<EntityTypeId, EntityTypeAssets>,
}

/// Parse the manifest + raw asset bytes into a CPU catalog.
pub fn build_entity_catalog(manifest: &EntityManifest, assets: &[EntityAssetTransfer]) -> EntityCatalog {
    let mut by_type: HashMap<EntityTypeId, &EntityAssetTransfer> =
        assets.iter().map(|a| (a.type_id, a)).collect();
    let mut types = HashMap::new();
    for wire in &manifest.entities {
        let Some(asset) = by_type.remove(&wire.type_id) else {
            tracing::warn!("entity type {} has no asset bytes", wire.namespaced_id);
            continue;
        };
        let geo = match Geometry::from_json_bytes(&asset.geometry_json) {
            Ok(g) => g,
            Err(e) => {
                tracing::warn!("failed to parse geometry for {}: {e}", wire.namespaced_id);
                continue;
            }
        };
        let model = BakedModel::from_geometry(&geo);
        let animations = asset
            .animation_json
            .as_ref()
            .and_then(|bytes| AnimationSet::from_json_bytes(bytes).ok())
            .unwrap_or_default();
        types.insert(
            wire.type_id,
            EntityTypeAssets {
                wire: wire.clone(),
                model,
                animations,
                texture_png: asset.texture_png.clone(),
            },
        );
    }
    EntityCatalog { types }
}

// --- GPU cache (built lazily on first spawn of each type) ---

struct BoneGpu {
    name: String,
    parent: Option<usize>,
    /// Translation relative to the parent bone pivot (block units).
    bind_translation: Vec3,
    bind_rotation_deg: [f32; 3],
    mesh: Option<Handle<Mesh>>,
}

struct EntityTypeGpu {
    material: Handle<EntityMaterial>,
    scale: f32,
    bones: Vec<BoneGpu>,
}

#[derive(Resource, Default)]
pub struct EntityGpuCache {
    built: HashMap<EntityTypeId, EntityTypeGpu>,
}

/// Map from server entity id to its Bevy root entity.
#[derive(Resource, Default)]
pub struct EntityIndex {
    map: HashMap<EntityId, Entity>,
}

// --- Components ---

#[derive(Component)]
struct EntityRoot {
    type_id: EntityTypeId,
}

#[derive(Component)]
struct EntityAnimator {
    time: f32,
    life_time: f32,
    /// 0 = idle, 1 = walk.
    anim: u8,
}

#[derive(Component)]
struct EntityBone {
    name: String,
    bind_translation: Vec3,
    bind_rotation_deg: [f32; 3],
}

/// Points a bone entity back at its animated root.
#[derive(Component)]
struct BoneOwner(Entity);

/// The single client-local follower that tracks the camera.
#[derive(Component)]
struct EntityFollower;

// --- Net events ---

#[derive(Message)]
pub enum EntityNetEvent {
    Spawn(EntitySpawn),
    Update(EntityUpdate),
    Despawn(EntityId),
}

fn reset_entity_index(mut index: ResMut<EntityIndex>, mut cache: ResMut<EntityGpuCache>) {
    // Clear both the id map and the per-type GPU cache: a reconnect may load a
    // different catalog where the same `EntityTypeId` maps to different assets,
    // so cached meshes/textures/materials must not be reused across sessions.
    index.map.clear();
    cache.built.clear();
}

// --- GPU building ---

fn ensure_type_gpu<'a>(
    type_id: EntityTypeId,
    catalog: &EntityCatalog,
    cache: &'a mut EntityGpuCache,
    meshes: &mut Assets<Mesh>,
    images: &mut Assets<Image>,
    materials: &mut Assets<EntityMaterial>,
) -> Option<&'a EntityTypeGpu> {
    if !cache.built.contains_key(&type_id) {
        let assets = catalog.types.get(&type_id)?;
        let image = decode_entity_texture(&assets.texture_png);
        let tex_handle = images.add(image);
        let material = materials.add(EntityMaterial {
            texture: tex_handle,
            scene_lighting: default(),
            light: Vec4::new(1.0, 0.0, 0.5, 0.0),
        });
        let bones = assets
            .model
            .bones
            .iter()
            .map(|bone| {
                let parent_pivot = bone
                    .parent
                    .and_then(|p| assets.model.bones.get(p))
                    .map(|p| Vec3::from_array(p.pivot))
                    .unwrap_or(Vec3::ZERO);
                let pivot = Vec3::from_array(bone.pivot);
                let mesh = if bone.vertices.is_empty() {
                    None
                } else {
                    Some(meshes.add(bake_bone_mesh(bone)))
                };
                BoneGpu {
                    name: bone.name.clone(),
                    parent: bone.parent,
                    bind_translation: pivot - parent_pivot,
                    bind_rotation_deg: bone.bind_rotation,
                    mesh,
                }
            })
            .collect();
        cache.built.insert(
            type_id,
            EntityTypeGpu {
                material,
                scale: assets.wire.scale.max(0.01),
                bones,
            },
        );
    }
    cache.built.get(&type_id)
}

fn bake_bone_mesh(bone: &stagcrest_bedrock::BakedBone) -> Mesh {
    let positions: Vec<[f32; 3]> = bone.vertices.iter().map(|v| v.position).collect();
    let normals: Vec<[f32; 3]> = bone.vertices.iter().map(|v| v.normal).collect();
    let uvs: Vec<[f32; 2]> = bone.vertices.iter().map(|v| v.uv).collect();
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(bone.indices.clone()));
    mesh
}

fn decode_entity_texture(png: &[u8]) -> Image {
    let (width, height, rgba) = match image::load_from_memory(png) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            (w, h, rgba.into_raw())
        }
        Err(_) => (1, 1, vec![255, 0, 255, 255]),
    };
    let mut image = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        rgba,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::all(),
    );
    image.sampler = ImageSampler::nearest();
    image
}

// --- Spawn / update / despawn ---

fn spawn_entity_instance(
    commands: &mut Commands,
    gpu: &EntityTypeGpu,
    type_id: EntityTypeId,
    transform: Transform,
    anim: u8,
    follower: bool,
) -> Entity {
    let mut root_cmd = commands.spawn((
        GameSessionEntity,
        EntityRoot { type_id },
        EntityAnimator {
            time: 0.0,
            life_time: 0.0,
            anim,
        },
        transform.with_scale(Vec3::splat(gpu.scale)),
        Visibility::Visible,
    ));
    if follower {
        root_cmd.insert(EntityFollower);
    }
    let root = root_cmd.id();

    // Pass 1: spawn all bone entities.
    let mut bone_entities: Vec<Entity> = Vec::with_capacity(gpu.bones.len());
    for bone in &gpu.bones {
        let mut cmd = commands.spawn((
            EntityBone {
                name: bone.name.clone(),
                bind_translation: bone.bind_translation,
                bind_rotation_deg: bone.bind_rotation_deg,
            },
            BoneOwner(root),
            Transform::from_translation(bone.bind_translation)
                .with_rotation(bedrock_rotation_to_quat(bone.bind_rotation_deg)),
            Visibility::Inherited,
        ));
        if let Some(mesh) = &bone.mesh {
            cmd.insert((Mesh3d(mesh.clone()), MeshMaterial3d(gpu.material.clone())));
        }
        bone_entities.push(cmd.id());
    }

    // Pass 2: parent bones to their bone-parent (or the root).
    for (i, bone) in gpu.bones.iter().enumerate() {
        let parent = bone
            .parent
            .and_then(|p| bone_entities.get(p).copied())
            .unwrap_or(root);
        commands.entity(parent).add_child(bone_entities[i]);
    }

    root
}

#[allow(clippy::too_many_arguments)]
fn apply_entity_net_events(
    mut commands: Commands,
    mut events: MessageReader<EntityNetEvent>,
    catalog: Option<Res<EntityCatalog>>,
    mut cache: ResMut<EntityGpuCache>,
    mut index: ResMut<EntityIndex>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<EntityMaterial>>,
    mut roots: Query<(&mut Transform, &mut EntityAnimator), With<EntityRoot>>,
) {
    let Some(catalog) = catalog else {
        events.clear();
        return;
    };
    for event in events.read() {
        match event {
            EntityNetEvent::Spawn(spawn) => {
                if index.map.contains_key(&spawn.id) {
                    continue;
                }
                let Some(gpu) = ensure_type_gpu(
                    spawn.type_id,
                    &catalog,
                    &mut cache,
                    &mut meshes,
                    &mut images,
                    &mut materials,
                ) else {
                    continue;
                };
                let transform = Transform::from_xyz(spawn.x, spawn.y, spawn.z)
                    .with_rotation(Quat::from_rotation_y(spawn.yaw));
                let root =
                    spawn_entity_instance(&mut commands, gpu, spawn.type_id, transform, 0, false);
                index.map.insert(spawn.id, root);
            }
            EntityNetEvent::Update(update) => {
                if let Some(&root) = index.map.get(&update.id) {
                    if let Ok((mut tf, mut anim)) = roots.get_mut(root) {
                        tf.translation = Vec3::new(update.x, update.y, update.z);
                        tf.rotation = Quat::from_rotation_y(update.yaw);
                        anim.anim = update.anim;
                    }
                }
            }
            EntityNetEvent::Despawn(id) => {
                if let Some(root) = index.map.remove(id) {
                    if let Ok(mut e) = commands.get_entity(root) {
                        e.despawn();
                    }
                }
            }
        }
    }
}

// --- Animation ---

struct PoseState {
    poses: HashMap<String, stagcrest_bedrock::BonePose>,
}

fn animate_entities(
    time: Res<Time>,
    catalog: Option<Res<EntityCatalog>>,
    mut roots: Query<(Entity, &EntityRoot, &mut EntityAnimator)>,
    mut bones: Query<(&EntityBone, &BoneOwner, &mut Transform)>,
) {
    let Some(catalog) = catalog else {
        return;
    };
    let dt = time.delta_secs();

    let mut states: HashMap<Entity, PoseState> = HashMap::new();
    for (entity, root, mut anim) in &mut roots {
        anim.time += dt;
        anim.life_time += dt;
        let Some(ty) = catalog.types.get(&root.type_id) else {
            continue;
        };
        let clip_name = if anim.anim == 1 {
            ty.wire.walk_animation.as_deref()
        } else {
            Some(ty.wire.idle_animation.as_str())
        };
        let poses = clip_name
            .and_then(|n| ty.animations.get(n))
            .map(|clip| clip.sample(anim.time, anim.life_time))
            .unwrap_or_default();
        states.insert(entity, PoseState { poses });
    }

    for (bone, owner, mut tf) in &mut bones {
        let Some(state) = states.get(&owner.0) else {
            continue;
        };
        let pose = state.poses.get(&bone.name).copied().unwrap_or_default();
        let rot_deg = [
            bone.bind_rotation_deg[0] + pose.rotation[0],
            bone.bind_rotation_deg[1] + pose.rotation[1],
            bone.bind_rotation_deg[2] + pose.rotation[2],
        ];
        tf.rotation = bedrock_rotation_to_quat(rot_deg);
        tf.translation = bone.bind_translation
            + Vec3::new(pose.position[0], pose.position[1], pose.position[2])
                / stagcrest_bedrock::MODEL_PIXELS_PER_BLOCK;
        tf.scale = Vec3::new(pose.scale[0], pose.scale[1], pose.scale[2]);
    }
}

// --- Follower ---

fn spawn_follower(
    mut commands: Commands,
    catalog: Option<Res<EntityCatalog>>,
    mut cache: ResMut<EntityGpuCache>,
    existing: Query<(), With<EntityFollower>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<EntityMaterial>>,
) {
    if !existing.is_empty() {
        return;
    }
    let Some(catalog) = catalog else {
        return;
    };
    let Some(&type_id) = catalog.types.keys().next() else {
        return;
    };
    let Some(gpu) = ensure_type_gpu(
        type_id,
        &catalog,
        &mut cache,
        &mut meshes,
        &mut images,
        &mut materials,
    ) else {
        return;
    };
    spawn_entity_instance(
        &mut commands,
        gpu,
        type_id,
        Transform::from_xyz(0.0, -1000.0, 0.0),
        0,
        true,
    );
}

fn update_follower(
    camera: Query<&Transform, (With<GameCamera>, Without<EntityFollower>)>,
    mut follower: Query<&mut Transform, With<EntityFollower>>,
) {
    let Ok(cam) = camera.single() else {
        return;
    };
    let Ok(mut tf) = follower.single_mut() else {
        return;
    };
    // Place the follower behind the camera at ground level, facing the same way
    // the camera looks, so it never blocks the view (like a third-person self).
    let forward = cam.forward().as_vec3();
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    if flat_forward == Vec3::ZERO {
        return;
    }
    let feet = cam.translation - Vec3::Y * 1.62;
    tf.translation = feet - flat_forward * 2.0;
    // The model's front face looks toward -Z at yaw 0, so rotate to align it
    // with the camera's forward heading.
    let yaw = flat_forward.x.atan2(flat_forward.z) + std::f32::consts::PI;
    tf.rotation = Quat::from_rotation_y(yaw);
}

// --- Lighting sync ---

fn push_scene_lighting_to_entities(
    lighting: Res<SceneLighting>,
    mut materials: ResMut<Assets<EntityMaterial>>,
) {
    for (_, mat) in materials.iter_mut() {
        mat.scene_lighting = lighting.uniform;
    }
}
