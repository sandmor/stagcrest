//! Client-side world time with server sync and interpolation.

use bevy::prelude::*;
use stagcrest_protocol::TimeOfDay;

const LERP_SPEED: f32 = 8.0;
const SNAP_THRESHOLD: f64 = 30.0;

#[derive(Resource)]
pub struct WorldTime {
    pub server_time: f64,
    pub display: TimeOfDay,
    pub initialized: bool,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            server_time: 0.0,
            display: TimeOfDay::default(),
            initialized: false,
        }
    }
}

impl WorldTime {
    pub fn set_from_server(&mut self, time: f64) {
        let wrapped = TimeOfDay::new(time);
        self.server_time = wrapped.seconds();
        if !self.initialized {
            self.display = wrapped;
            self.initialized = true;
            return;
        }
        let diff = (wrapped.seconds() - self.display.seconds()).abs();
        let wrap_diff = stagcrest_protocol::DAY_LENGTH_SECS - diff;
        if diff.min(wrap_diff) > SNAP_THRESHOLD {
            self.display = wrapped;
        }
    }

    /// Toward the sun; maps to GPU `sun_position_dir`. Not incoming light.
    pub fn sun_dir(&self) -> Vec3 {
        let d = self.display.sun_dir();
        Vec3::new(d.x, d.y, d.z)
    }

    /// Incoming sunlight; orients Bevy `DirectionalLight`. Equals `-sun_dir()`.
    pub fn sun_light_dir(&self) -> Vec3 {
        let d = self.display.sun_light_dir();
        Vec3::new(d.x, d.y, d.z)
    }

    /// Toward the moon; maps to GPU `moon_position_dir`.
    pub fn moon_dir(&self) -> Vec3 {
        let d = self.display.moon_dir();
        Vec3::new(d.x, d.y, d.z)
    }

    pub fn day_factor(&self) -> f32 {
        self.display.day_factor()
    }

    pub fn cycle(&self) -> f32 {
        self.display.cycle()
    }

    pub fn sun_disc_factor(&self) -> f32 {
        self.display.sun_disc_factor()
    }

    pub fn moon_disc_factor(&self) -> f32 {
        self.display.moon_disc_factor()
    }
}

pub fn update_world_time(time: Res<Time>, mut world_time: ResMut<WorldTime>) {
    if !world_time.initialized {
        return;
    }

    let dt = time.delta_secs();
    world_time.server_time = (world_time.server_time + f64::from(dt))
        .rem_euclid(stagcrest_protocol::DAY_LENGTH_SECS);

    let mut display = world_time.display;
    display.advance(dt as f64);

    let target = TimeOfDay::new(world_time.server_time);
    let diff = (target.seconds() - display.seconds()).abs();
    if diff > SNAP_THRESHOLD {
        display = target;
    } else if diff > 0.05 {
        let t = (LERP_SPEED * dt).min(1.0) as f64;
        let a = display.seconds();
        let b = target.seconds();
        let mut lerped = if (b - a).abs() > stagcrest_protocol::DAY_LENGTH_SECS * 0.5 {
            if a > b {
                (a + (b + stagcrest_protocol::DAY_LENGTH_SECS - a) * t)
                    % stagcrest_protocol::DAY_LENGTH_SECS
            } else {
                (a + (b - stagcrest_protocol::DAY_LENGTH_SECS - a) * t)
                    .rem_euclid(stagcrest_protocol::DAY_LENGTH_SECS)
            }
        } else {
            a + (b - a) * t
        };
        lerped = lerped.rem_euclid(stagcrest_protocol::DAY_LENGTH_SECS);
        display = TimeOfDay::new(lerped);
    }

    world_time.display = display;
}

pub fn apply_world_time_message(world_time: &mut WorldTime, time: f64) {
    world_time.set_from_server(time);
}
