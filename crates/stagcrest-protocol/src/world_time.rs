//! Server-authoritative day/night cycle.
//!
//! World axes and lighting conventions: see `stagcrest-render/docs/scene_lighting_conventions.md`.

use glam::Vec3;
use serde::{Deserialize, Serialize};

/// Real-time seconds for one full day/night cycle (20 minutes).
pub const DAY_LENGTH_SECS: f64 = 1200.0;

/// Seconds since world start, wrapping at [`DAY_LENGTH_SECS`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct TimeOfDay(pub f64);

impl TimeOfDay {
    pub fn new(t: f64) -> Self {
        Self(t.rem_euclid(DAY_LENGTH_SECS))
    }

    pub fn advance(&mut self, dt_secs: f64) {
        self.0 = (self.0 + dt_secs).rem_euclid(DAY_LENGTH_SECS);
    }

    pub fn seconds(&self) -> f64 {
        self.0
    }

    fn angle_rad(&self) -> f32 {
        let t = (self.0 / DAY_LENGTH_SECS) as f32;
        t * std::f32::consts::TAU
    }

    /// Unit vector from the scene **toward** the sun (not incoming light).
    /// Use [`Self::sun_light_dir`] for Bevy `DirectionalLight` orientation.
    pub fn sun_dir(&self) -> Vec3 {
        let phase = self.angle_rad();
        let elevation = (-phase.cos() + 1.0) * 0.5;
        let horizontal = (1.0 - elevation).sqrt();
        let azimuth_sin = phase.sin();
        let azimuth_cos = phase.cos();
        // +X at sunrise, sun travels east → west → east through south.
        Vec3::new(azimuth_sin * horizontal, elevation, azimuth_cos * horizontal).normalize_or_zero()
    }

    /// Unit vector **from the sun toward the scene** (incoming light / shadow cast direction).
    pub fn sun_light_dir(&self) -> Vec3 {
        -self.sun_dir()
    }

    /// Unit vector from the scene toward the moon (approximate; see conventions doc).
    pub fn moon_dir(&self) -> Vec3 {
        let sun = self.sun_dir();
        if sun.y > 0.05 {
            Vec3::new(-sun.x, 0.12, -sun.z).normalize_or_zero()
        } else {
            (-sun + Vec3::new(0.0, 0.35, 0.0)).normalize_or_zero()
        }
    }

    /// 0 at night, 1 at noon (from sun elevation).
    pub fn day_factor(&self) -> f32 {
        self.sun_dir().y.clamp(0.0, 1.0).powf(0.35)
    }

    /// Normalized 0..1 position within the day cycle.
    pub fn cycle(&self) -> f32 {
        (self.0 / DAY_LENGTH_SECS) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_overhead_at_noon() {
        let noon = TimeOfDay(DAY_LENGTH_SECS * 0.5);
        assert!(noon.sun_dir().y > 0.95, "sun y={}", noon.sun_dir().y);
        assert!(noon.day_factor() > 0.9);
    }

    #[test]
    fn sun_rises_in_east() {
        let sunrise = TimeOfDay(DAY_LENGTH_SECS * 0.25);
        assert!(
            sunrise.sun_dir().x > 0.5,
            "sunrise x={}",
            sunrise.sun_dir().x
        );
        assert!(sunrise.sun_dir().y > 0.0);
    }

    #[test]
    fn sun_sets_in_west() {
        let sunset = TimeOfDay(DAY_LENGTH_SECS * 0.75);
        assert!(sunset.sun_dir().x < -0.5, "sunset x={}", sunset.sun_dir().x);
    }

    #[test]
    fn sun_light_travels_opposite_position() {
        let morning = TimeOfDay(DAY_LENGTH_SECS * 0.3);
        let to_sun = morning.sun_dir();
        let light = morning.sun_light_dir();
        assert!(to_sun.dot(light) < -0.9);
    }

    #[test]
    fn sun_below_horizon_at_midnight() {
        let midnight = TimeOfDay(0.0);
        assert!(midnight.sun_dir().y < 0.05);
        assert!(midnight.day_factor() < 0.05);
    }

    #[test]
    fn wraps_on_advance() {
        let mut t = TimeOfDay(DAY_LENGTH_SECS - 1.0);
        t.advance(2.0);
        assert!(t.seconds() < 2.0);
    }
}
