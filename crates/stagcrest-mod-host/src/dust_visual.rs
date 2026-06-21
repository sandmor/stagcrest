use stagcrest_protocol::{encode_power_tint, BlockId, CircuitKind, FaceTexture, TintKind};

use crate::registry::BlockRegistry;

/// Horizontal connection bitmask: N=1, E=2, S=4, W=8.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DustConnections(pub u8);

impl DustConnections {
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }
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

/// Resolve dust texture from neighbor layout. Tint is always power-based (0–15 levels).
pub fn resolve_dust_face(
    registry: &BlockRegistry,
    _connections: DustConnections,
    _power: u8,
) -> FaceTexture {
    let texture_name = dust_texture_name(_connections);
    let texture = registry
        .texture_by_name(texture_name)
        .or_else(|| registry.texture_by_name("stagcrest:redstone_dust_dot"))
        .unwrap_or(stagcrest_protocol::TextureId(0));

    FaceTexture {
        texture,
        overlay: None,
        tint: TintKind::PowerLevel,
        overlay_tint: TintKind::None,
    }
}

/// Vertex tint value for dust at the given power level.
pub fn dust_vertex_tint(power: u8) -> f32 {
    encode_power_tint(power)
}

fn dust_texture_name(connections: DustConnections) -> &'static str {
    match connections.count() {
        0 => "stagcrest:redstone_dust_dot",
        1 => "stagcrest:redstone_dust_line",
        2 if connections.0 == 0b0101 || connections.0 == 0b1010 => {
            "stagcrest:redstone_dust_line"
        }
        2 => "stagcrest:redstone_dust_corner",
        3 => "stagcrest:redstone_dust_t",
        _ => "stagcrest:redstone_dust_cross",
    }
}

/// Build connection mask from horizontal neighbors.
pub fn dust_connections_from_neighbors<F>(mut is_connectable: F) -> DustConnections
where
    F: FnMut(i32, i32, i32) -> bool,
{
    let mut mask = 0u8;
    if is_connectable(0, 0, -1) {
        mask |= 1;
    }
    if is_connectable(1, 0, 0) {
        mask |= 2;
    }
    if is_connectable(0, 0, 1) {
        mask |= 4;
    }
    if is_connectable(-1, 0, 0) {
        mask |= 8;
    }
    DustConnections(mask)
}
