use crate::registry::BlockRegistry;
use stagcrest_protocol::BlockId;
use std::collections::HashMap;

pub use stagcrest_mod_sdk::{FeatureKind, RegisterBiomeFeatureRequest, RegisterBiomeRequest};

#[derive(Debug, Clone)]
struct PendingBiome {
    namespaced_id: String,
    temperature: f32,
    downfall: f32,
    surface_top: String,
    surface_under: String,
    surface_depth: u8,
    underwater_top: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BiomeFeature {
    pub kind: FeatureKind,
    pub chance: f32,
}

#[derive(Debug, Clone)]
pub struct ResolvedBiome {
    pub namespaced_id: String,
    pub temperature: f32,
    pub downfall: f32,
    pub surface_top: BlockId,
    pub surface_under: BlockId,
    pub surface_depth: u8,
    pub underwater_top: Option<BlockId>,
    pub features: Vec<BiomeFeature>,
}

#[derive(Debug, Default, Clone)]
pub struct BiomeRegistry {
    pending: Vec<PendingBiome>,
    pending_features: Vec<RegisterBiomeFeatureRequest>,
    biomes: Vec<ResolvedBiome>,
    finalized: bool,
}

impl BiomeRegistry {
    pub fn register_biome(&mut self, req: RegisterBiomeRequest) {
        self.finalized = false;
        self.pending.push(PendingBiome {
            namespaced_id: req.namespaced_id,
            temperature: req.temperature,
            downfall: req.downfall,
            surface_top: req.surface_top,
            surface_under: req.surface_under,
            surface_depth: req.surface_depth,
            underwater_top: req.underwater_top,
        });
    }

    pub fn register_feature(&mut self, req: RegisterBiomeFeatureRequest) {
        self.finalized = false;
        self.pending_features.push(req);
    }

    pub fn finalize(&mut self, registry: &BlockRegistry) -> Result<(), String> {
        let mut feature_map: HashMap<String, Vec<BiomeFeature>> = HashMap::new();
        for feat in self.pending_features.drain(..) {
            feature_map
                .entry(feat.biome_id)
                .or_default()
                .push(BiomeFeature {
                    kind: feat.feature_kind,
                    chance: feat.chance.clamp(0.0, 1.0),
                });
        }

        let mut resolved = Vec::new();
        for biome in self.pending.drain(..) {
            let surface_top = registry
                .block_by_name(&biome.surface_top)
                .ok_or_else(|| format!("unknown surface_top block: {}", biome.surface_top))?;
            let surface_under = registry
                .block_by_name(&biome.surface_under)
                .ok_or_else(|| format!("unknown surface_under block: {}", biome.surface_under))?;
            let underwater_top = biome
                .underwater_top
                .as_ref()
                .map(|name| {
                    registry
                        .block_by_name(name)
                        .ok_or_else(|| format!("unknown underwater_top block: {name}"))
                })
                .transpose()?;
            let features = feature_map.remove(&biome.namespaced_id).unwrap_or_default();
            resolved.push(ResolvedBiome {
                namespaced_id: biome.namespaced_id,
                temperature: biome.temperature,
                downfall: biome.downfall,
                surface_top,
                surface_under,
                surface_depth: biome.surface_depth.max(1),
                underwater_top,
                features,
            });
        }

        if resolved.is_empty() {
            return Err("no biomes registered".into());
        }

        if !feature_map.is_empty() {
            let unknown: Vec<_> = feature_map.keys().cloned().collect();
            return Err(format!("unknown biome_id in features: {unknown:?}"));
        }

        self.biomes = resolved;
        self.finalized = true;
        Ok(())
    }

    pub fn biomes(&self) -> &[ResolvedBiome] {
        &self.biomes
    }

    pub fn biome_at(&self, temperature: f32, downfall: f32) -> &ResolvedBiome {
        self.biomes
            .iter()
            .min_by(|a, b| {
                let da = biome_distance(a, temperature, downfall);
                let db = biome_distance(b, temperature, downfall);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("biome registry must be finalized with at least one biome")
    }

    pub fn default_plains(&self) -> &ResolvedBiome {
        self.biomes
            .iter()
            .find(|b| b.namespaced_id == "stagcrest:plains")
            .unwrap_or(&self.biomes[0])
    }
}

fn biome_distance(biome: &ResolvedBiome, temperature: f32, downfall: f32) -> f32 {
    let dt = biome.temperature - temperature;
    let dd = biome.downfall - downfall;
    dt * dt + dd * dd
}

pub fn register_biome_host(registry: &mut BiomeRegistry, json: RegisterBiomeRequest) {
    registry.register_biome(json);
}

pub fn register_biome_feature_host(registry: &mut BiomeRegistry, json: RegisterBiomeFeatureRequest) {
    registry.register_feature(json);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::test_fixtures::test_registry;

    #[test]
    fn biome_at_picks_nearest_climate() {
        let reg = test_registry();
        let mut biomes = BiomeRegistry::default();
        biomes.register_biome(RegisterBiomeRequest {
            namespaced_id: "stagcrest:plains".into(),
            temperature: 0.8,
            downfall: 0.4,
            surface_top: "stagcrest:grass_block".into(),
            surface_under: "stagcrest:dirt".into(),
            surface_depth: 3,
            underwater_top: Some("stagcrest:sand".into()),
        });
        biomes.register_biome(RegisterBiomeRequest {
            namespaced_id: "stagcrest:desert".into(),
            temperature: 2.0,
            downfall: 0.0,
            surface_top: "stagcrest:sand".into(),
            surface_under: "stagcrest:sand".into(),
            surface_depth: 4,
            underwater_top: Some("stagcrest:sand".into()),
        });
        biomes.finalize(&reg).unwrap();

        let desert = biomes.biome_at(1.9, 0.05);
        assert_eq!(desert.namespaced_id, "stagcrest:desert");
        let plains = biomes.biome_at(0.75, 0.45);
        assert_eq!(plains.namespaced_id, "stagcrest:plains");
    }

    #[test]
    fn finalize_errors_without_biomes() {
        let reg = test_registry();
        let mut biomes = BiomeRegistry::default();
        let err = biomes.finalize(&reg).unwrap_err();
        assert!(err.contains("no biomes registered"));
    }

    #[test]
    fn finalize_errors_on_orphan_features() {
        let reg = test_registry();
        let mut biomes = BiomeRegistry::default();
        biomes.register_biome(RegisterBiomeRequest {
            namespaced_id: "stagcrest:plains".into(),
            temperature: 0.8,
            downfall: 0.4,
            surface_top: "stagcrest:grass_block".into(),
            surface_under: "stagcrest:dirt".into(),
            surface_depth: 3,
            underwater_top: None,
        });
        biomes.register_feature(RegisterBiomeFeatureRequest {
            biome_id: "stagcrest:nonexistent".into(),
            feature_kind: FeatureKind::ShortGrass,
            chance: 0.1,
        });
        let err = biomes.finalize(&reg).unwrap_err();
        assert!(err.contains("unknown biome_id"));
    }
}
