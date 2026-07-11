//! Per-frame sun/moon shadow weights for celestial directional lights.

use stagcrest_render::QualityTier;

/// Continuous celestial shadow strengths and GPU enable flags for one frame.
#[derive(Debug, Clone, Copy)]
pub struct CelestialWeights {
    pub sun_disc: f32,
    pub moon_disc: f32,
    /// Shader shadow strength 0..1 for the sun CSM.
    pub sun_shadow: f32,
    /// Shader shadow strength 0..1 for the moon CSM.
    pub moon_shadow: f32,
    /// Bevy `DirectionalLight.shadow_maps_enabled` for the sun.
    pub sun_shadow_gpu: bool,
    /// Bevy `DirectionalLight.shadow_maps_enabled` for the moon.
    pub moon_shadow_gpu: bool,
}

/// Minimum shadow strength before allocating a cascaded shadow map.
const GPU_SHADOW_EPSILON: f32 = 0.02;

impl CelestialWeights {
    pub fn compute(
        tier: QualityTier,
        day_factor: f32,
        sun_disc: f32,
        moon_disc: f32,
    ) -> Self {
        let mut sun_shadow = sun_disc * smoothstep(0.0, 0.12, sun_disc);
        let mut moon_shadow =
            moon_disc * (1.0 - day_factor * 0.5) * smoothstep(0.0, 0.12, moon_disc);

        if tier == QualityTier::Low {
            let sun_dominance = smoothstep(0.15, 0.35, day_factor);
            sun_shadow *= sun_dominance;
            moon_shadow *= 1.0 - sun_dominance;
        }

        Self {
            sun_disc,
            moon_disc,
            sun_shadow,
            moon_shadow,
            sun_shadow_gpu: sun_shadow > GPU_SHADOW_EPSILON,
            moon_shadow_gpu: moon_shadow > GPU_SHADOW_EPSILON,
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 >= edge1 {
        return f32::from(x >= edge0);
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_controls_twilight_shadow_balance() {
        let (day_factor, sun_disc, moon_disc) = (0.25_f32, 0.4_f32, 0.7_f32);

        let low_dusk =
            CelestialWeights::compute(QualityTier::Low, day_factor, sun_disc, moon_disc);
        let low_night = CelestialWeights::compute(QualityTier::Low, 0.1, 0.0, 0.9);
        let ultra =
            CelestialWeights::compute(QualityTier::Ultra, day_factor, sun_disc, moon_disc);

        assert!(low_dusk.sun_shadow > 0.0 && low_dusk.moon_shadow > 0.0);
        assert!(low_night.sun_shadow < low_dusk.sun_shadow);
        assert!(low_night.moon_shadow > low_dusk.moon_shadow);
        assert!(ultra.sun_shadow > 0.0 && ultra.moon_shadow > 0.0);
        assert!(ultra.moon_shadow >= low_dusk.moon_shadow);
    }

    #[test]
    fn gpu_enable_follows_epsilon() {
        let faint = CelestialWeights::compute(QualityTier::Ultra, 0.9, 0.01, 0.01);
        assert!(!faint.sun_shadow_gpu);
        assert!(!faint.moon_shadow_gpu);

        let strong = CelestialWeights::compute(QualityTier::Ultra, 0.5, 0.5, 0.5);
        assert!(strong.sun_shadow_gpu);
        assert!(strong.moon_shadow_gpu);
    }
}
