use crate::registry::BlockRegistry;
use stagcrest_protocol::manifest::{BiomeClientDef, BiomesSnapshot};
use stagcrest_protocol::BlockId;
use std::collections::HashMap;

pub use stagcrest_mod_sdk::{
    BiomeDimension, BiomeEnvironment, FeatureKind, FeaturePlacement, NoiseRange,
    RegisterBiomeFeatureRequest, RegisterBiomeRequest, RegisterCaveConfigRequest,
    RegisterFeatureRequest, RegisterRiverConfigRequest, TreeShape,
};

/// Sampled climate parameters at a world position.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClimateParams {
    pub temperature: f32,
    pub humidity: f32,
    pub continentalness: f32,
    pub erosion: f32,
    pub depth: f32,
    pub weirdness: f32,
}

#[derive(Debug, Clone)]
pub struct BiomeFeature {
    pub placement: FeaturePlacement,
    pub chance: f32,
}

#[derive(Debug, Clone)]
pub struct ResolvedBiome {
    pub index: u16,
    pub namespaced_id: String,
    pub dimension: BiomeDimension,
    pub temperature: NoiseRange,
    pub humidity: NoiseRange,
    pub continentalness: NoiseRange,
    pub erosion: NoiseRange,
    pub depth: NoiseRange,
    pub weirdness: NoiseRange,
    pub offset: f32,
    pub surface_top: BlockId,
    pub surface_under: BlockId,
    pub surface_depth: u8,
    pub underwater_top: Option<BlockId>,
    pub underwater_under: Option<BlockId>,
    pub environment: BiomeEnvironment,
    pub features: Vec<BiomeFeature>,
    pub climate_temp: f32,
    pub climate_downfall: f32,
}

#[derive(Debug, Clone)]
pub struct RiverConfig {
    pub width: f32,
    pub valley_depth: f32,
    pub bank_blocks: Vec<BlockId>,
    pub river_biome_index: u16,
    pub frozen_river_biome_index: u16,
    pub riverbank_biome_index: u16,
}

impl Default for RiverConfig {
    fn default() -> Self {
        Self {
            width: 4.0,
            valley_depth: 6.0,
            bank_blocks: Vec::new(),
            river_biome_index: 0,
            frozen_river_biome_index: 0,
            riverbank_biome_index: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CaveConfig {
    pub cheese_threshold: f64,
    pub spaghetti_threshold: f64,
    pub noodle_threshold: f64,
    pub lush_cave_biome_index: u16,
    pub dripstone_cave_biome_index: u16,
    pub deep_dark_biome_index: u16,
}

impl Default for CaveConfig {
    fn default() -> Self {
        Self {
            cheese_threshold: 0.55,
            spaghetti_threshold: 0.02,
            noodle_threshold: 0.015,
            lush_cave_biome_index: 0,
            dripstone_cave_biome_index: 0,
            deep_dark_biome_index: 0,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct BiomeRegistry {
    pending: Vec<RegisterBiomeRequest>,
    pending_features: Vec<RegisterFeatureRequest>,
    pending_legacy_features: Vec<RegisterBiomeFeatureRequest>,
    pending_river: Option<RegisterRiverConfigRequest>,
    pending_cave: Option<RegisterCaveConfigRequest>,
    biomes: Vec<ResolvedBiome>,
    river_config: RiverConfig,
    cave_config: CaveConfig,
    id_to_index: HashMap<String, u16>,
    finalized: bool,
}

impl BiomeRegistry {
    pub fn register_biome(&mut self, req: RegisterBiomeRequest) {
        self.finalized = false;
        self.pending.push(req);
    }

    pub fn register_feature(&mut self, req: RegisterFeatureRequest) {
        self.finalized = false;
        self.pending_features.push(req);
    }

    pub fn register_legacy_feature(&mut self, req: RegisterBiomeFeatureRequest) {
        self.finalized = false;
        self.pending_legacy_features.push(req);
    }

    pub fn register_river_config(&mut self, req: RegisterRiverConfigRequest) {
        self.finalized = false;
        self.pending_river = Some(req);
    }

    pub fn register_cave_config(&mut self, req: RegisterCaveConfigRequest) {
        self.finalized = false;
        self.pending_cave = Some(req);
    }

    pub fn finalize(&mut self, registry: &BlockRegistry) -> Result<(), String> {
        let mut feature_map: HashMap<String, Vec<BiomeFeature>> = HashMap::new();

        for feat in self.pending_features.drain(..) {
            feature_map
                .entry(feat.biome_id)
                .or_default()
                .push(BiomeFeature {
                    placement: feat.placement,
                    chance: feat.chance.clamp(0.0, 1.0),
                });
        }

        for feat in self.pending_legacy_features.drain(..) {
            if let Some(placement) = legacy_feature_to_placement(feat.feature_kind) {
                feature_map
                    .entry(feat.biome_id)
                    .or_default()
                    .push(BiomeFeature {
                        placement,
                        chance: feat.chance.clamp(0.0, 1.0),
                    });
            }
        }

        let mut resolved = Vec::new();
        let mut id_to_index = HashMap::new();

        for (idx, biome) in self.pending.drain(..).enumerate() {
            let surface_top = resolve_block(registry, &biome.surface_top)?;
            let surface_under = resolve_block(registry, &biome.surface_under)?;
            let underwater_top = biome
                .underwater_top
                .as_ref()
                .map(|n| resolve_block(registry, n))
                .transpose()?;
            let underwater_under = biome
                .underwater_under
                .as_ref()
                .map(|n| resolve_block(registry, n))
                .transpose()?;
            let climate_temp = (biome.temperature.min + biome.temperature.max) * 0.5;
            let climate_downfall = (biome.humidity.min + biome.humidity.max) * 0.5;
            let index = idx as u16;
            id_to_index.insert(biome.namespaced_id.clone(), index);
            let features = feature_map.remove(&biome.namespaced_id).unwrap_or_default();
            resolved.push(ResolvedBiome {
                index,
                namespaced_id: biome.namespaced_id,
                dimension: biome.dimension,
                temperature: biome.temperature,
                humidity: biome.humidity,
                continentalness: biome.continentalness,
                erosion: biome.erosion,
                depth: biome.depth,
                weirdness: biome.weirdness,
                offset: biome.offset,
                surface_top,
                surface_under,
                surface_depth: biome.surface_depth.max(1),
                underwater_top,
                underwater_under,
                environment: biome.environment,
                features,
                climate_temp,
                climate_downfall,
            });
        }

        if resolved.is_empty() {
            return Err("no biomes registered".into());
        }

        if !feature_map.is_empty() {
            let unknown: Vec<_> = feature_map.keys().cloned().collect();
            return Err(format!("unknown biome_id in features: {unknown:?}"));
        }

        if let Some(river_req) = self.pending_river.take() {
            self.river_config = RiverConfig {
                width: river_req.width,
                valley_depth: river_req.valley_depth,
                bank_blocks: river_req
                    .bank_blocks
                    .iter()
                    .map(|n| resolve_block(registry, n))
                    .collect::<Result<Vec<_>, _>>()?,
                river_biome_index: *id_to_index
                    .get(&river_req.river_biome_id)
                    .ok_or_else(|| format!("unknown river biome: {}", river_req.river_biome_id))?,
                frozen_river_biome_index: *id_to_index
                    .get(&river_req.frozen_river_biome_id)
                    .ok_or_else(|| {
                        format!(
                            "unknown frozen river biome: {}",
                            river_req.frozen_river_biome_id
                        )
                    })?,
                riverbank_biome_index: *id_to_index.get(&river_req.riverbank_biome_id).ok_or_else(
                    || format!("unknown riverbank biome: {}", river_req.riverbank_biome_id),
                )?,
            };
        }

        if let Some(cave_req) = self.pending_cave.take() {
            self.cave_config = CaveConfig {
                cheese_threshold: cave_req.cheese_threshold,
                spaghetti_threshold: cave_req.spaghetti_threshold,
                noodle_threshold: cave_req.noodle_threshold,
                lush_cave_biome_index: *id_to_index.get(&cave_req.lush_cave_biome_id).ok_or_else(
                    || format!("unknown lush cave biome: {}", cave_req.lush_cave_biome_id),
                )?,
                dripstone_cave_biome_index: *id_to_index
                    .get(&cave_req.dripstone_cave_biome_id)
                    .ok_or_else(|| {
                        format!(
                            "unknown dripstone cave biome: {}",
                            cave_req.dripstone_cave_biome_id
                        )
                    })?,
                deep_dark_biome_index: *id_to_index.get(&cave_req.deep_dark_biome_id).ok_or_else(
                    || format!("unknown deep dark biome: {}", cave_req.deep_dark_biome_id),
                )?,
            };
        }

        self.biomes = resolved;
        self.id_to_index = id_to_index;
        self.finalized = true;
        Ok(())
    }

    pub fn biomes(&self) -> &[ResolvedBiome] {
        &self.biomes
    }

    pub fn river_config(&self) -> &RiverConfig {
        &self.river_config
    }

    pub fn cave_config(&self) -> &CaveConfig {
        &self.cave_config
    }

    pub fn biome_by_index(&self, index: u16) -> Option<&ResolvedBiome> {
        self.biomes.get(index as usize)
    }

    pub fn index_of(&self, id: &str) -> Option<u16> {
        self.id_to_index.get(id).copied()
    }

    /// Pick the best-matching biome for sampled climate params in a dimension.
    pub fn biome_at(&self, params: ClimateParams, dimension: BiomeDimension) -> &ResolvedBiome {
        self.biomes
            .iter()
            .filter(|b| b.dimension == dimension)
            .min_by(|a, b| {
                let da = biome_fitness(a, params);
                let db = biome_fitness(b, params);
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .or_else(|| {
                self.biomes
                    .iter()
                    .find(|b| b.dimension == BiomeDimension::Surface)
            })
            .unwrap_or(&self.biomes[0])
    }

    /// Legacy 2D climate lookup (surface dimension).
    pub fn biome_at_climate(&self, temperature: f32, downfall: f32) -> &ResolvedBiome {
        self.biome_at(
            ClimateParams {
                temperature,
                humidity: downfall,
                ..Default::default()
            },
            BiomeDimension::Surface,
        )
    }

    pub fn default_plains(&self) -> &ResolvedBiome {
        self.biomes
            .iter()
            .find(|b| b.namespaced_id == "stagcrest:plains")
            .unwrap_or(&self.biomes[0])
    }

    pub fn to_client_snapshot(&self) -> BiomesSnapshot {
        BiomesSnapshot {
            biomes: self
                .biomes
                .iter()
                .map(|b| BiomeClientDef {
                    index: b.index,
                    namespaced_id: b.namespaced_id.clone(),
                    fog_color: b.environment.fog_color,
                    fog_density: b.environment.fog_density,
                    water_color: b.environment.water_color,
                    water_fog_color: b.environment.water_fog_color,
                    sky_color: b.environment.sky_color,
                    grass_color: b.environment.grass_color,
                    foliage_color: b.environment.foliage_color,
                    temperature: b.climate_temp,
                    downfall: b.climate_downfall,
                })
                .collect(),
        }
    }
}

fn resolve_block(registry: &BlockRegistry, name: &str) -> Result<BlockId, String> {
    registry
        .block_by_name(name)
        .ok_or_else(|| format!("unknown block: {name}"))
}

fn range_distance(value: f32, range: NoiseRange) -> f32 {
    if value < range.min {
        range.min - value
    } else if value > range.max {
        value - range.max
    } else {
        0.0
    }
}

fn biome_fitness(biome: &ResolvedBiome, params: ClimateParams) -> f32 {
    let mut cost = 0.0f32;
    cost += range_distance(params.temperature, biome.temperature).powi(2);
    cost += range_distance(params.humidity, biome.humidity).powi(2);
    cost += range_distance(params.continentalness, biome.continentalness).powi(2);
    cost += range_distance(params.erosion, biome.erosion).powi(2);
    cost += range_distance(params.depth, biome.depth).powi(2);
    cost += range_distance(params.weirdness, biome.weirdness).powi(2);
    cost - biome.offset
}

fn legacy_feature_to_placement(kind: FeatureKind) -> Option<FeaturePlacement> {
    Some(match kind {
        FeatureKind::ShortGrass => FeaturePlacement::Plant {
            block: "stagcrest:short_grass".into(),
            tall: false,
        },
        FeatureKind::TallGrass => FeaturePlacement::Plant {
            block: "stagcrest:tall_grass".into(),
            tall: true,
        },
        FeatureKind::Dandelion => FeaturePlacement::Plant {
            block: "stagcrest:dandelion".into(),
            tall: false,
        },
        FeatureKind::Poppy => FeaturePlacement::Plant {
            block: "stagcrest:poppy".into(),
            tall: false,
        },
        FeatureKind::Cactus => FeaturePlacement::Column {
            block: "stagcrest:cactus".into(),
            height: 3,
        },
        FeatureKind::DeadBush => FeaturePlacement::Plant {
            block: "stagcrest:dead_bush".into(),
            tall: false,
        },
        FeatureKind::OakTree => FeaturePlacement::Tree {
            trunk: "stagcrest:oak_log".into(),
            leaves: "stagcrest:oak_leaves".into(),
            shape: TreeShape::Oak,
            height: 7,
        },
    })
}

pub fn register_biome_host(registry: &mut BiomeRegistry, json: RegisterBiomeRequest) {
    registry.register_biome(json);
}

pub fn register_feature_host(registry: &mut BiomeRegistry, json: RegisterFeatureRequest) {
    registry.register_feature(json);
}

pub fn register_biome_feature_host(
    registry: &mut BiomeRegistry,
    json: RegisterBiomeFeatureRequest,
) {
    registry.register_legacy_feature(json);
}

pub fn register_river_config_host(registry: &mut BiomeRegistry, json: RegisterRiverConfigRequest) {
    registry.register_river_config(json);
}

pub fn register_cave_config_host(registry: &mut BiomeRegistry, json: RegisterCaveConfigRequest) {
    registry.register_cave_config(json);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::test_fixtures::test_registry;
    use stagcrest_mod_sdk::BiomeEnvironment;

    fn sample_biome(id: &str, temp: f32, humid: f32) -> RegisterBiomeRequest {
        RegisterBiomeRequest {
            namespaced_id: id.into(),
            dimension: BiomeDimension::Surface,
            temperature: NoiseRange::point(temp),
            humidity: NoiseRange::point(humid),
            continentalness: NoiseRange::new(-0.2, 0.2),
            erosion: NoiseRange::new(-0.5, 0.5),
            depth: NoiseRange::new(0.0, 0.5),
            weirdness: NoiseRange::new(-0.5, 0.5),
            offset: 0.0,
            surface_top: "stagcrest:grass_block".into(),
            surface_under: "stagcrest:dirt".into(),
            surface_depth: 3,
            underwater_top: Some("stagcrest:sand".into()),
            underwater_under: None,
            environment: BiomeEnvironment::plains_default(),
        }
    }

    #[test]
    fn biome_at_picks_nearest_climate() {
        let reg = test_registry();
        let mut biomes = BiomeRegistry::default();
        biomes.register_biome(sample_biome("stagcrest:plains", 0.8, 0.4));
        let mut desert = sample_biome("stagcrest:desert", 2.0, 0.0);
        desert.surface_top = "stagcrest:sand".into();
        desert.surface_under = "stagcrest:sand".into();
        biomes.register_biome(desert);
        biomes.finalize(&reg).unwrap();

        let desert = biomes.biome_at_climate(1.9, 0.05);
        assert_eq!(desert.namespaced_id, "stagcrest:desert");
        let plains = biomes.biome_at_climate(0.75, 0.45);
        assert_eq!(plains.namespaced_id, "stagcrest:plains");
    }
}
