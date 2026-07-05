use stagcrest_protocol::{
    encode_power_tint, observer_facing, repeater_connects_toward, repeater_facing, BlockGeometry,
    BlockId, BlockState, CircuitKind, ModelId, TextureId,
};

use crate::registry::BlockRegistry;

pub use stagcrest_circuit::{compute_wire_connections, WireConnections, WireLink};

/// Texture IDs for wire line mesh composition (center dot + line segments + overlay).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WireLineTextures {
    pub dot: TextureId,
    pub line_ns: TextureId,
    pub line_ew: TextureId,
    pub overlay: Option<TextureId>,
}

pub trait PowerLookup: Sync {
    fn power_at(&self, pos: stagcrest_protocol::BlockPos) -> u8;
}

/// Whether `id` renders as a flat wire line (`stagcrest:redstone_dust` or `CircuitKind::Wire`).
pub fn is_wire_line_block(registry: &BlockRegistry, id: BlockId) -> bool {
    let Some(def) = registry.block(id) else {
        return false;
    };
    def.circuit_kind()
        .is_some_and(|kind| matches!(kind, CircuitKind::Wire { .. }))
}

/// Wire line neighbor check with directional rules for repeaters.
pub fn is_wire_line_neighbor(
    registry: &BlockRegistry,
    id: BlockId,
    state: BlockState,
    toward_wire_dx: i32,
    toward_wire_dz: i32,
) -> bool {
    let Some(def) = registry.block(id) else {
        return false;
    };
    if matches!(def.geometry, BlockGeometry::Model(ModelId::Repeater)) {
        return repeater_connects_toward(repeater_facing(state), toward_wire_dx, toward_wire_dz);
    }
    if matches!(def.geometry, BlockGeometry::Model(ModelId::Observer)) {
        return repeater_connects_toward(observer_facing(state), toward_wire_dx, toward_wire_dz);
    }
    is_wire_line_block(registry, id)
}

/// Resolve wire line layer textures from the registry.
pub fn resolve_wire_line_textures(registry: &BlockRegistry) -> WireLineTextures {
    let lookup = |name: &str, fallback: &str| {
        registry
            .texture_by_name(name)
            .or_else(|| registry.texture_by_name(fallback))
            .unwrap_or(TextureId(0))
    };
    WireLineTextures {
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

/// Vertex tint from wire signal level (0–15).
pub fn wire_power_vertex_tint(power: u8) -> f32 {
    encode_power_tint(power)
}

/// Whether the center junction dot should be drawn (hidden on pure straight horizontal lines).
pub fn wire_shows_center_junction(connections: WireConnections) -> bool {
    let mut side_count = 0u32;
    let mut up_count = 0u32;
    let mut ns = false;
    let mut ew = false;
    for (i, side) in connections.sides.iter().enumerate() {
        match side {
            WireLink::None => {}
            WireLink::Side => {
                side_count += 1;
                match i {
                    0 | 2 => ns = true,
                    _ => ew = true,
                }
            }
            WireLink::Up => up_count += 1,
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
