//! Client graphics quality tiers and per-feature toggles.

use bevy::prelude::*;

/// Overall quality preset; individual toggles can still be changed afterward.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum QualityTier {
    Low,
    #[default]
    Medium,
    High,
    Ultra,
}

/// Water reflection technique tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReflectionTier {
    SkyOnly,
    Ssr,
    Planar,
}

impl Default for ReflectionTier {
    fn default() -> Self {
        Self::Ssr
    }
}

/// Persistent graphics configuration (saved in client settings).
#[derive(Resource, Clone, Debug)]
pub struct GraphicsSettings {
    pub tier: QualityTier,
    pub reflection_tier: ReflectionTier,
    pub shadow_cascades: u32,
    pub shadow_distance: f32,
    pub ssao: bool,
    pub bloom: bool,
    pub taa: bool,
    pub volumetric_light: bool,
    pub fog: bool,
    pub dof: bool,
    pub sharpen: bool,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        let mut s = Self {
            tier: QualityTier::Medium,
            reflection_tier: ReflectionTier::Ssr,
            shadow_cascades: 3,
            shadow_distance: 96.0,
            ssao: true,
            bloom: true,
            taa: true,
            volumetric_light: false,
            fog: true,
            dof: false,
            sharpen: true,
        };
        s.apply_tier_presets();
        s
    }
}

impl GraphicsSettings {
    pub fn from_tier(tier: QualityTier) -> Self {
        let mut s = Self::default();
        s.tier = tier;
        s.apply_tier_presets();
        s
    }

    /// Apply recommended per-feature defaults for the current [`QualityTier`].
    pub fn apply_tier_presets(&mut self) {
        match self.tier {
            QualityTier::Low => {
                self.reflection_tier = ReflectionTier::SkyOnly;
                self.shadow_cascades = 2;
                self.shadow_distance = 64.0;
                self.ssao = false;
                self.bloom = false;
                self.taa = false;
                self.volumetric_light = false;
                self.fog = true;
                self.dof = false;
                self.sharpen = false;
            }
            QualityTier::Medium => {
                self.reflection_tier = ReflectionTier::Ssr;
                self.shadow_cascades = 3;
                self.shadow_distance = 96.0;
                self.ssao = true;
                self.bloom = true;
                self.taa = true;
                self.volumetric_light = true;
                self.fog = true;
                self.dof = false;
                self.sharpen = true;
            }
            QualityTier::High => {
                self.reflection_tier = ReflectionTier::Planar;
                self.shadow_cascades = 4;
                self.shadow_distance = 128.0;
                self.ssao = true;
                self.bloom = true;
                self.taa = true;
                self.volumetric_light = true;
                self.fog = true;
                self.dof = false;
                self.sharpen = true;
            }
            QualityTier::Ultra => {
                self.reflection_tier = ReflectionTier::Planar;
                self.shadow_cascades = 4;
                self.shadow_distance = 160.0;
                self.ssao = true;
                self.bloom = true;
                self.taa = true;
                self.volumetric_light = true;
                self.fog = true;
                self.dof = true;
                self.sharpen = true;
            }
        }
        self.shadow_cascades = self.shadow_cascades.clamp(1, 4);
    }

    pub fn reflection_tier_index(&self) -> f32 {
        match self.reflection_tier {
            ReflectionTier::SkyOnly => 0.0,
            ReflectionTier::Ssr => 1.0,
            ReflectionTier::Planar => 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_presets_clamp_shadow_cascades() {
        let mut settings = GraphicsSettings::from_tier(QualityTier::Low);
        settings.shadow_cascades = 99;
        settings.apply_tier_presets();
        assert_eq!(settings.shadow_cascades, 2);

        settings.tier = QualityTier::Ultra;
        settings.apply_tier_presets();
        assert_eq!(settings.shadow_cascades, 4);
    }
}

pub struct GraphicsSettingsPlugin;

impl Plugin for GraphicsSettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GraphicsSettings>();
    }
}
