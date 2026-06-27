use stagcrest_protocol::{
    encode_power_tint, repeater_connects_toward, repeater_facing, BlockGeometry, BlockId,
    BlockState, CircuitKind, ModelId, TextureId,
};

use crate::registry::BlockRegistry;

/// Per-direction redstone wire connection (matches vanilla `none` / `side` / `up`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DustSide {
    #[default]
    None,
    Side,
    Up,
}

/// Horizontal wire layout: index 0 = north (-Z), 1 = east (+X), 2 = south (+Z), 3 = west (-X).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DustConnections {
    pub sides: [DustSide; 4],
}

impl DustConnections {
    pub const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

    pub fn count(self) -> u32 {
        self.sides.iter().filter(|s| **s != DustSide::None).count() as u32
    }

    pub fn side(&self, index: usize) -> DustSide {
        self.sides[index]
    }

    pub fn set_side(&mut self, index: usize, side: DustSide) {
        self.sides[index] = side;
    }

    /// Inventory / icon preview: four-way cross.
    pub fn icon_cross() -> Self {
        Self {
            sides: [DustSide::Side; 4],
        }
    }
}

/// Texture IDs for vanilla-style dust composition (dot + line0/line1 + overlay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DustTextures {
    pub dot: TextureId,
    pub line_ns: TextureId,
    pub line_ew: TextureId,
    pub overlay: Option<TextureId>,
}

pub trait PowerLookup: Sync {
    fn power_at(&self, pos: stagcrest_protocol::BlockPos) -> u8;
}

/// Whether `id` is redstone dust or another block that extends dust visually.
pub fn is_dust_connectable(registry: &BlockRegistry, id: BlockId) -> bool {
    let Some(def) = registry.block(id) else {
        return false;
    };
    if def.namespaced_id == "stagcrest:redstone_dust" {
        return true;
    }
    def.circuit
        .is_some_and(|c| matches!(c.kind, CircuitKind::Wire { .. }))
}

/// Dust connection check with directional rules for repeaters.
pub fn is_dust_connectable_neighbor(
    registry: &BlockRegistry,
    id: BlockId,
    state: BlockState,
    toward_dust_dx: i32,
    toward_dust_dz: i32,
) -> bool {
    let Some(def) = registry.block(id) else {
        return false;
    };
    if matches!(def.geometry, BlockGeometry::Model(ModelId::Repeater)) {
        return repeater_connects_toward(repeater_facing(state), toward_dust_dx, toward_dust_dz);
    }
    is_dust_connectable(registry, id)
}

/// Resolve the four dust layer textures from the registry.
pub fn resolve_dust_textures(registry: &BlockRegistry) -> DustTextures {
    let lookup = |name: &str, fallback: &str| {
        registry
            .texture_by_name(name)
            .or_else(|| registry.texture_by_name(fallback))
            .unwrap_or(TextureId(0))
    };
    DustTextures {
        dot: lookup("stagcrest:redstone_dust_dot", "stagcrest:redstone_dust_dot"),
        line_ns: lookup(
            "stagcrest:redstone_dust_line",
            "stagcrest:redstone_dust_dot",
        ),
        line_ew: lookup(
            "stagcrest:redstone_dust_line1",
            "stagcrest:redstone_dust_line",
        ),
        overlay: registry.texture_by_name("stagcrest:redstone_dust_overlay"),
    }
}

/// Vertex tint value for dust at the given power level.
pub fn dust_vertex_tint(power: u8) -> f32 {
    encode_power_tint(power)
}

/// Whether the center dot should be drawn (hidden on pure straight horizontal lines).
pub fn dust_shows_dot(connections: DustConnections) -> bool {
    let mut side_count = 0u32;
    let mut up_count = 0u32;
    let mut ns = false;
    let mut ew = false;
    for (i, side) in connections.sides.iter().enumerate() {
        match side {
            DustSide::None => {}
            DustSide::Side => {
                side_count += 1;
                match i {
                    0 | 2 => ns = true,
                    _ => ew = true,
                }
            }
            DustSide::Up => up_count += 1,
        }
    }
    if up_count > 0 {
        return true;
    }
    if side_count == 2 && ns && !ew {
        return false;
    }
    if side_count == 2 && ew && !ns {
        return false;
    }
    true
}

/// Build per-direction connection states from local neighbor queries.
///
/// `connectable_at(dx, dy, dz)` — dust-connectable block at offset from dust.
/// `full_cube_at(dx, dy, dz)` — opaque solid cube at offset (blocks vertical climb when capped).
pub fn compute_dust_connections(
    mut connectable_at: impl FnMut(i32, i32, i32) -> bool,
    mut full_cube_at: impl FnMut(i32, i32, i32) -> bool,
) -> DustConnections {
    let mut connections = DustConnections::default();
    for (i, &(dx, dz)) in DustConnections::DIRECTIONS.iter().enumerate() {
        connections.sides[i] =
            dust_side_for_direction(dx, dz, &mut connectable_at, &mut full_cube_at);
    }
    apply_single_connection_mirror(&mut connections);
    connections
}

fn dust_side_for_direction(
    dx: i32,
    dz: i32,
    connectable_at: &mut impl FnMut(i32, i32, i32) -> bool,
    full_cube_at: &mut impl FnMut(i32, i32, i32) -> bool,
) -> DustSide {
    if connectable_at(dx, 0, dz) {
        return DustSide::Side;
    }
    if full_cube_at(dx, 0, dz) {
        if connectable_at(dx, 1, dz) && !full_cube_at(0, 1, 0) {
            return DustSide::Up;
        }
    } else if connectable_at(dx, -1, dz) {
        return DustSide::Side;
    }
    DustSide::None
}

/// When exactly one direction connects, vanilla draws a line through the block.
fn apply_single_connection_mirror(connections: &mut DustConnections) {
    let active: Vec<usize> = connections
        .sides
        .iter()
        .enumerate()
        .filter(|(_, s)| **s != DustSide::None)
        .map(|(i, _)| i)
        .collect();
    if active.len() != 1 {
        return;
    }
    let i = active[0];
    let opposite = match i {
        0 => 2,
        1 => 3,
        2 => 0,
        3 => 1,
        _ => return,
    };
    if connections.sides[opposite] == DustSide::None {
        connections.sides[opposite] = DustSide::Side;
    }
}
