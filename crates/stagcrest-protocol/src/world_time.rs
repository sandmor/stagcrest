//! Server-authoritative day/night cycle.
//!
//! World axes and lighting conventions: see `stagcrest-render/docs/RENDERING.md` §5.5.

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
    ///
    /// Elevation follows a full daily arc: below the horizon at midnight (`y < 0`),
    /// eastern horizon at sunrise, zenith at noon, western horizon at sunset.
    pub fn sun_dir(&self) -> Vec3 {
        let phase = self.angle_rad();
        let elevation = (phase - std::f32::consts::FRAC_PI_2).sin();
        let horizontal = (1.0 - elevation * elevation).max(0.0).sqrt();
        let azimuth_sin = phase.sin();
        let azimuth_cos = phase.cos();
        Vec3::new(azimuth_sin * horizontal, elevation, azimuth_cos * horizontal).normalize_or_zero()
    }

    /// Unit vector **from the sun toward the scene** (incoming light / shadow cast direction).
    pub fn sun_light_dir(&self) -> Vec3 {
        -self.sun_dir()
    }

    /// Unit vector from the scene toward the moon (approximate; see conventions doc).
    ///
    /// At night the moon is near zenith. At sunrise/sunset it sits on the horizon
    /// opposite the sun. During the day it stays low on the far side of the sky.
    /// Direction blends smoothly across elevation bands (no piecewise jumps).
    pub fn moon_dir(&self) -> Vec3 {
        let sun = self.sun_dir();
        let y = sun.y;

        let moon_day = Vec3::new(-sun.x, 0.12, -sun.z).normalize_or_zero();
        let moon_twilight = moon_twilight_dir(sun, y);
        let moon_night = (-sun + Vec3::new(0.0, 0.5, 0.0)).normalize_or_zero();

        let twilight_to_night = 1.0 - smoothstep(-0.18, -0.06, y);
        let twilight_night = slerp_dir(moon_twilight, moon_night, twilight_to_night);

        let day_to_twilight = smoothstep(0.06, 0.18, y);
        slerp_dir(twilight_night, moon_day, day_to_twilight)
    }

    /// Unit vector **from the moon toward the scene** (incoming moonlight / shadow cast direction).
    pub fn moon_light_dir(&self) -> Vec3 {
        -self.moon_dir()
    }

    /// 0 deep night, ~0.35 at sunrise/sunset, 1 at noon.
    pub fn day_factor(&self) -> f32 {
        let y = self.sun_dir().y;
        ((y + 0.12) / 1.12).clamp(0.0, 1.0).powf(0.35)
    }

    /// Multiplier for rendering the sun disc (0 below horizon, 1 well above).
    pub fn sun_disc_factor(&self) -> f32 {
        let y = self.sun_dir().y;
        smoothstep(-0.12, 0.12, y)
    }

    /// Multiplier for rendering the moon disc.
    pub fn moon_disc_factor(&self) -> f32 {
        let y = self.sun_dir().y;
        let twilight_to_night = 1.0 - smoothstep(-0.18, -0.06, y);
        let twilight_night = 0.45 + (1.0 - 0.45) * twilight_to_night;
        let day_to_twilight = smoothstep(0.06, 0.18, y);
        0.15 + (twilight_night - 0.15) * (1.0 - day_to_twilight)
    }

    /// Normalized 0..1 position within the day cycle.
    pub fn cycle(&self) -> f32 {
        (self.0 / DAY_LENGTH_SECS) as f32
    }
}

/// Hermite smoothstep on `[edge0, edge1]`.
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 >= edge1 {
        return f32::from(x >= edge0);
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn moon_twilight_dir(sun: Vec3, y: f32) -> Vec3 {
    let azimuth = Vec3::new(-sun.x, 0.0, -sun.z);
    let horiz = if azimuth.length_squared() > 1e-6 {
        azimuth.normalize()
    } else {
        Vec3::NEG_Z
    };
    let elev = (y * 0.4 + 0.05).clamp(0.03, 0.12);
    let flat = (1.0 - elev * elev).max(0.0).sqrt();
    (horiz * flat + Vec3::Y * elev).normalize_or_zero()
}

/// Spherical interpolation between unit directions.
fn slerp_dir(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    let a = a.normalize_or_zero();
    let b = b.normalize_or_zero();
    if a.length_squared() < 1e-6 {
        return b;
    }
    if b.length_squared() < 1e-6 {
        return a;
    }
    let dot = a.dot(b).clamp(-1.0, 1.0);
    if dot > 0.9995 {
        return (a * (1.0 - t) + b * t).normalize_or_zero();
    }
    let omega = dot.acos();
    let sin_omega = omega.sin();
    if sin_omega.abs() < 1e-6 {
        return a;
    }
    (a * ((1.0 - t) * omega).sin() + b * (t * omega).sin()) / sin_omega
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
        assert!(sunrise.sun_dir().y >= 0.0);
    }

    #[test]
    fn sun_sets_in_west() {
        let sunset = TimeOfDay(DAY_LENGTH_SECS * 0.75);
        assert!(sunset.sun_dir().x < -0.5, "sunset x={}", sunset.sun_dir().x);
    }

    #[test]
    fn celestial_light_dir_is_negated_position_dir() {
        let morning = TimeOfDay(DAY_LENGTH_SECS * 0.3);
        assert!(morning.sun_dir().dot(morning.sun_light_dir()) < -0.9);

        let midnight = TimeOfDay(0.0);
        assert!(midnight.moon_dir().dot(midnight.moon_light_dir()) < -0.9);
    }

    #[test]
    fn sun_below_horizon_at_midnight() {
        let midnight = TimeOfDay(0.0);
        assert!(
            midnight.sun_dir().y < -0.9,
            "sun should be below horizon at midnight, y={}",
            midnight.sun_dir().y
        );
        assert!(midnight.day_factor() < 0.05);
    }

    #[test]
    fn moon_overhead_at_midnight() {
        let midnight = TimeOfDay(0.0);
        assert!(
            midnight.moon_dir().y > 0.8,
            "moon should be high at midnight, y={}",
            midnight.moon_dir().y
        );
    }

    #[test]
    fn twilight_moon_opposite_sun() {
        let sunrise = TimeOfDay(DAY_LENGTH_SECS * 0.25);
        let sunset = TimeOfDay(DAY_LENGTH_SECS * 0.75);
        assert!(sunrise.moon_dir().x < -0.9, "moon west at sunrise");
        assert!(sunset.moon_dir().x > 0.9, "moon east at sunset");
        assert!(sunrise.moon_dir().y < 0.15);
        assert!(sunset.moon_dir().y < 0.15);
    }

    #[test]
    fn twilight_has_nonzero_day_factor() {
        let sunrise = TimeOfDay(DAY_LENGTH_SECS * 0.25);
        let sunset = TimeOfDay(DAY_LENGTH_SECS * 0.75);
        assert!(sunrise.day_factor() > 0.2, "sunrise day_factor={}", sunrise.day_factor());
        assert!(sunset.day_factor() > 0.2, "sunset day_factor={}", sunset.day_factor());
        assert!(sunrise.sun_disc_factor() > 0.4);
        assert!(sunset.sun_disc_factor() > 0.4);
    }

    #[test]
    fn wraps_on_advance() {
        let mut t = TimeOfDay(DAY_LENGTH_SECS - 1.0);
        t.advance(2.0);
        assert!(t.seconds() < 2.0);
    }

    #[test]
    fn dusk_celestial_curves_are_continuous() {
        let mut prev_moon = TimeOfDay(880.0).moon_dir();
        let mut prev_sun_disc = TimeOfDay(880.0).sun_disc_factor();
        let mut prev_moon_disc = TimeOfDay(880.0).moon_disc_factor();

        for sec in 881..=960 {
            let t = TimeOfDay(sec as f64);
            let moon = t.moon_dir();
            let sun_disc = t.sun_disc_factor();
            let moon_disc = t.moon_disc_factor();

            assert!(
                prev_moon.dot(moon) > 0.98,
                "moon_dir jump at {sec}s: dot={}",
                prev_moon.dot(moon)
            );
            assert!(
                (sun_disc - prev_sun_disc).abs() < 0.05,
                "sun_disc jump at {sec}s: {prev_sun_disc} -> {sun_disc}"
            );
            assert!(
                (moon_disc - prev_moon_disc).abs() < 0.06,
                "moon_disc jump at {sec}s: {prev_moon_disc} -> {moon_disc}"
            );

            prev_moon = moon;
            prev_sun_disc = sun_disc;
            prev_moon_disc = moon_disc;
        }
    }
}
