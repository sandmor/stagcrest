use glam::Vec3;
use stagcrest_protocol::BlockPos;

#[derive(Debug, Clone, Copy)]
pub struct RaycastHit {
    pub block: BlockPos,
    pub face_normal: Vec3,
    pub distance: f32,
}

/// Block-local AABB used by the raycast to refine hits against sub-block
/// geometry. Coordinates are relative to the block origin (0..1 range for a
/// full cube).
#[derive(Debug, Clone, Copy)]
pub struct RaycastBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl RaycastBounds {
    pub fn full_cube() -> Self {
        Self {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        }
    }
}

/// Slab test: ray (origin + t * dir) vs world-space AABB [aabb_min, aabb_max].
/// Returns `Some((t_enter, entering_axis))` or `None` on miss.
/// `entering_axis` is 0/1/2 for X/Y/Z, used to derive the face normal.
fn ray_aabb(origin: Vec3, inv_dir: Vec3, aabb_min: Vec3, aabb_max: Vec3) -> Option<(f32, usize)> {
    let t1 = (aabb_min - origin) * inv_dir;
    let t2 = (aabb_max - origin) * inv_dir;

    let t_near = Vec3::min(t1, t2);
    let t_far = Vec3::max(t1, t2);

    let mut enter = t_near.x;
    let mut axis = 0usize;
    if t_near.y > enter {
        enter = t_near.y;
        axis = 1;
    }
    if t_near.z > enter {
        enter = t_near.z;
        axis = 2;
    }

    let exit = t_far.x.min(t_far.y).min(t_far.z);

    if enter <= exit && exit >= 0.0 {
        Some((enter.max(0.0), axis))
    } else {
        None
    }
}

fn normal_from_axis(axis: usize, dir: Vec3) -> Vec3 {
    match axis {
        0 => Vec3::new(if dir.x > 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0),
        1 => Vec3::new(0.0, if dir.y > 0.0 { -1.0 } else { 1.0 }, 0.0),
        _ => Vec3::new(0.0, 0.0, if dir.z > 0.0 { -1.0 } else { 1.0 }),
    }
}

pub fn raycast_blocks(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    mut is_solid: impl FnMut(BlockPos) -> bool,
    mut bounds: impl FnMut(BlockPos) -> Option<RaycastBounds>,
) -> Option<RaycastHit> {
    let dir = direction.normalize();
    let inv_dir = Vec3::new(
        if dir.x != 0.0 { 1.0 / dir.x } else { f32::INFINITY },
        if dir.y != 0.0 { 1.0 / dir.y } else { f32::INFINITY },
        if dir.z != 0.0 { 1.0 / dir.z } else { f32::INFINITY },
    );
    let mut t = 0.0f32;
    let mut current = BlockPos::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let step = Vec3::new(
        if dir.x > 0.0 { 1.0 } else { -1.0 },
        if dir.y > 0.0 { 1.0 } else { -1.0 },
        if dir.z > 0.0 { 1.0 } else { -1.0 },
    );

    let mut t_max = Vec3::new(
        if dir.x != 0.0 {
            ((current.x as f32 + if step.x > 0.0 { 1.0 } else { 0.0 }) - origin.x) / dir.x
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            ((current.y as f32 + if step.y > 0.0 { 1.0 } else { 0.0 }) - origin.y) / dir.y
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            ((current.z as f32 + if step.z > 0.0 { 1.0 } else { 0.0 }) - origin.z) / dir.z
        } else {
            f32::INFINITY
        },
    );

    let t_delta = Vec3::new(
        if dir.x != 0.0 {
            (step.x / dir.x).abs()
        } else {
            f32::INFINITY
        },
        if dir.y != 0.0 {
            (step.y / dir.y).abs()
        } else {
            f32::INFINITY
        },
        if dir.z != 0.0 {
            (step.z / dir.z).abs()
        } else {
            f32::INFINITY
        },
    );

    let mut last_normal = Vec3::ZERO;

    while t <= max_distance {
        if is_solid(current) {
            if let Some(rb) = bounds(current) {
                let block_origin = Vec3::new(
                    current.x as f32,
                    current.y as f32,
                    current.z as f32,
                );
                let aabb_min = block_origin + Vec3::from_array(rb.min);
                let aabb_max = block_origin + Vec3::from_array(rb.max);
                if let Some((t_hit, axis)) = ray_aabb(origin, inv_dir, aabb_min, aabb_max) {
                    if t_hit <= max_distance {
                        return Some(RaycastHit {
                            block: current,
                            face_normal: normal_from_axis(axis, dir),
                            distance: t_hit,
                        });
                    }
                }
            } else {
                return Some(RaycastHit {
                    block: current,
                    face_normal: last_normal,
                    distance: t,
                });
            }
        }

        if t_max.x < t_max.y && t_max.x < t_max.z {
            t = t_max.x;
            t_max.x += t_delta.x;
            current.x += step.x as i32;
            last_normal = Vec3::new(-step.x, 0.0, 0.0);
        } else if t_max.y < t_max.z {
            t = t_max.y;
            t_max.y += t_delta.y;
            current.y += step.y as i32;
            last_normal = Vec3::new(0.0, -step.y, 0.0);
        } else {
            t = t_max.z;
            t_max.z += t_delta.z;
            current.z += step.z as i32;
            last_normal = Vec3::new(0.0, 0.0, -step.z);
        }
    }

    None
}
