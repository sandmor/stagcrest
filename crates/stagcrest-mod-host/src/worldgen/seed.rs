#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldSeed(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerrainLayer {
    ElevationLow,
    ElevationMid,
    ElevationHigh,
    Roughness,
    SkyIslandPlacement,
    SkyIslandShape,
    Temperature,
    Moisture,
    CaveCheese,
    CaveSpaghetti,
    CaveNoodle,
    OreIron,
}

impl TerrainLayer {
    pub const ALL: [TerrainLayer; 12] = [
        TerrainLayer::ElevationLow,
        TerrainLayer::ElevationMid,
        TerrainLayer::ElevationHigh,
        TerrainLayer::Roughness,
        TerrainLayer::SkyIslandPlacement,
        TerrainLayer::SkyIslandShape,
        TerrainLayer::Temperature,
        TerrainLayer::Moisture,
        TerrainLayer::CaveCheese,
        TerrainLayer::CaveSpaghetti,
        TerrainLayer::CaveNoodle,
        TerrainLayer::OreIron,
    ];
}

impl WorldSeed {
    pub fn layer_seed(self, layer: TerrainLayer) -> u32 {
        let tag = layer as u64;
        let mixed = self.0 ^ tag.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        (mixed ^ (mixed >> 33)).wrapping_mul(0xBF58_476D_1CE4_E5B9) as u32
    }
}
