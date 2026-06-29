/// Per-direction link between wire cells: flat horizontal, climb up over a block, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WireLink {
    #[default]
    None,
    Side,
    Up,
}

/// Horizontal wire layout: index 0 = north (-Z), 1 = east (+X), 2 = south (+Z), 3 = west (-X).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WireConnections {
    pub sides: [WireLink; 4],
}

impl WireConnections {
    pub const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];

    pub fn count(self) -> u32 {
        self.sides.iter().filter(|s| **s != WireLink::None).count() as u32
    }

    pub fn side(&self, index: usize) -> WireLink {
        self.sides[index]
    }

    pub fn set_side(&mut self, index: usize, side: WireLink) {
        self.sides[index] = side;
    }

    pub fn icon_cross() -> Self {
        Self {
            sides: [WireLink::Side; 4],
        }
    }
}

pub fn compute_wire_connections(
    mut connectable_at: impl FnMut(i32, i32, i32) -> bool,
    mut full_cube_at: impl FnMut(i32, i32, i32) -> bool,
) -> WireConnections {
    let mut connections = WireConnections::default();
    for (i, &(dx, dz)) in WireConnections::DIRECTIONS.iter().enumerate() {
        connections.sides[i] =
            wire_link_for_direction(dx, dz, &mut connectable_at, &mut full_cube_at);
    }
    apply_single_connection_mirror(&mut connections);
    connections
}

pub fn wire_link_for_direction(
    dx: i32,
    dz: i32,
    connectable_at: &mut impl FnMut(i32, i32, i32) -> bool,
    full_cube_at: &mut impl FnMut(i32, i32, i32) -> bool,
) -> WireLink {
    if connectable_at(dx, 0, dz) {
        return WireLink::Side;
    }
    if full_cube_at(dx, 0, dz) {
        if connectable_at(dx, 1, dz) && !full_cube_at(0, 1, 0) {
            return WireLink::Up;
        }
    } else if connectable_at(dx, -1, dz) {
        return WireLink::Side;
    }
    WireLink::None
}

pub fn apply_single_connection_mirror(connections: &mut WireConnections) {
    let active: Vec<usize> = connections
        .sides
        .iter()
        .enumerate()
        .filter(|(_, s)| **s != WireLink::None)
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
    if connections.sides[opposite] == WireLink::None {
        connections.sides[opposite] = WireLink::Side;
    }
}
