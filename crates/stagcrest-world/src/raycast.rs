use glam::Vec3;
use stagcrest_protocol::BlockPos;

#[derive(Debug, Clone, Copy)]
pub struct RaycastHit {
    pub block: BlockPos,
    pub face_normal: Vec3,
    pub distance: f32,
}

pub fn raycast_blocks(
    origin: Vec3,
    direction: Vec3,
    max_distance: f32,
    mut is_solid: impl FnMut(BlockPos) -> bool,
) -> Option<RaycastHit> {
    let dir = direction.normalize();
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
            return Some(RaycastHit {
                block: current,
                face_normal: last_normal,
                distance: t,
            });
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
