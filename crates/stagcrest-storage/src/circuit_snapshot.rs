use stagcrest_protocol::{LocalBlockPos, CHUNK_SIZE};

pub const CIRCUIT_SNAPSHOT_WIRE_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDelaySnapshot {
    pub local: LocalBlockPos,
    /// Ticks until the scheduled output fires, relative to the circuit tick at export.
    pub remaining_ticks: u8,
    pub output: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChunkCircuitSnapshot {
    pub power: Vec<(LocalBlockPos, u8)>,
    pub delay_input: Vec<(LocalBlockPos, u8)>,
    pub pending_delays: Vec<PendingDelaySnapshot>,
    pub button_release: Vec<(LocalBlockPos, u8)>,
    /// Extended pistons and whether they were still powered when they finished extending.
    pub piston_extend_sustained: Vec<(LocalBlockPos, bool)>,
}

impl ChunkCircuitSnapshot {
    pub fn encode_wire(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.push(CIRCUIT_SNAPSHOT_WIRE_VERSION);
        write_u16(&mut out, self.power.len() as u16);
        for (local, level) in &self.power {
            write_u16(&mut out, pack_nibble_entry(*local, *level));
        }
        write_u16(&mut out, self.delay_input.len() as u16);
        for (local, input) in &self.delay_input {
            write_u16(&mut out, pack_nibble_entry(*local, *input));
        }
        out.push(self.pending_delays.len() as u8);
        for delay in &self.pending_delays {
            write_u16(&mut out, pack_nibble_entry(delay.local, delay.output));
            out.push(delay.remaining_ticks);
        }
        out.push(self.button_release.len() as u8);
        for (local, remaining) in &self.button_release {
            write_u16(&mut out, local.index() as u16);
            out.push(*remaining);
        }
        write_u16(&mut out, self.piston_extend_sustained.len() as u16);
        for (local, sustained) in &self.piston_extend_sustained {
            write_u16(&mut out, local.index() as u16);
            out.push(u8::from(*sustained));
        }
        out
    }

    pub fn decode_wire(bytes: &[u8]) -> Result<Self, crate::format::StorageFormatError> {
        let mut cursor = 0usize;
        let version = read_u8(bytes, &mut cursor)?;
        if version != CIRCUIT_SNAPSHOT_WIRE_VERSION {
            return Err(crate::format::StorageFormatError::UnsupportedCircuitSnapshot(version));
        }

        let power_count = read_u16(bytes, &mut cursor)? as usize;
        let mut power = Vec::with_capacity(power_count);
        for _ in 0..power_count {
            let packed = read_u16(bytes, &mut cursor)?;
            power.push(unpack_nibble_entry(packed)?);
        }

        let delay_input_count = read_u16(bytes, &mut cursor)? as usize;
        let mut delay_input = Vec::with_capacity(delay_input_count);
        for _ in 0..delay_input_count {
            let packed = read_u16(bytes, &mut cursor)?;
            delay_input.push(unpack_nibble_entry(packed)?);
        }

        let pending_count = read_u8(bytes, &mut cursor)? as usize;
        let mut pending_delays = Vec::with_capacity(pending_count);
        for _ in 0..pending_count {
            let packed = read_u16(bytes, &mut cursor)?;
            let (local, output) = unpack_nibble_entry(packed)?;
            let remaining_ticks = read_u8(bytes, &mut cursor)?;
            pending_delays.push(PendingDelaySnapshot {
                local,
                remaining_ticks,
                output,
            });
        }

        let button_count = read_u8(bytes, &mut cursor)? as usize;
        let mut button_release = Vec::with_capacity(button_count);
        for _ in 0..button_count {
            let index = read_u16(bytes, &mut cursor)? as usize;
            let local = local_from_index(index)?;
            let remaining = read_u8(bytes, &mut cursor)?;
            button_release.push((local, remaining));
        }

        let piston_extend_sustained = if version >= CIRCUIT_SNAPSHOT_WIRE_VERSION {
            let sustained_count = read_u16(bytes, &mut cursor)? as usize;
            let mut entries = Vec::with_capacity(sustained_count);
            for _ in 0..sustained_count {
                let index = read_u16(bytes, &mut cursor)? as usize;
                let local = local_from_index(index)?;
                let sustained = read_u8(bytes, &mut cursor)? != 0;
                entries.push((local, sustained));
            }
            entries
        } else {
            Vec::new()
        };

        Ok(Self {
            power,
            delay_input,
            pending_delays,
            button_release,
            piston_extend_sustained,
        })
    }
}

/// 12-bit chunk cell index + 4-bit value (0–15).
fn pack_nibble_entry(local: LocalBlockPos, value: u8) -> u16 {
    let index = local.index() as u16;
    debug_assert!(index <= 0x0FFF);
    (index & 0x0FFF) | ((u16::from(value.min(15)) & 0x0F) << 12)
}

fn unpack_nibble_entry(
    packed: u16,
) -> Result<(LocalBlockPos, u8), crate::format::StorageFormatError> {
    let index = (packed & 0x0FFF) as usize;
    let value = ((packed >> 12) & 0x0F) as u8;
    Ok((local_from_index(index)?, value))
}

fn local_from_index(index: usize) -> Result<LocalBlockPos, crate::format::StorageFormatError> {
    let volume = (CHUNK_SIZE as usize).pow(3);
    if index >= volume {
        return Err(crate::format::StorageFormatError::InvalidCircuitIndex(
            index,
        ));
    }
    let layer = CHUNK_SIZE as usize;
    let y = index / (layer * layer);
    let rem = index % (layer * layer);
    let z = rem / layer;
    let x = rem % layer;
    Ok(LocalBlockPos {
        x: x as u8,
        y: y as u8,
        z: z as u8,
    })
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, crate::format::StorageFormatError> {
    let b = *bytes
        .get(*cursor)
        .ok_or(crate::format::StorageFormatError::Truncated)?;
    *cursor += 1;
    Ok(b)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, crate::format::StorageFormatError> {
    let end = cursor.saturating_add(2);
    let slice = bytes
        .get(*cursor..end)
        .ok_or(crate::format::StorageFormatError::Truncated)?;
    *cursor = end;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_wire_roundtrip() {
        let snapshot = ChunkCircuitSnapshot {
            power: vec![
                (LocalBlockPos { x: 1, y: 2, z: 3 }, 14),
                (LocalBlockPos { x: 15, y: 0, z: 0 }, 7),
            ],
            delay_input: vec![(LocalBlockPos { x: 4, y: 5, z: 6 }, 12)],
            pending_delays: vec![PendingDelaySnapshot {
                local: LocalBlockPos { x: 2, y: 2, z: 2 },
                remaining_ticks: 3,
                output: 15,
            }],
            button_release: vec![(LocalBlockPos { x: 1, y: 0, z: 0 }, 20)],
            piston_extend_sustained: vec![
                (LocalBlockPos { x: 3, y: 0, z: 0 }, true),
                (LocalBlockPos { x: 4, y: 1, z: 2 }, false),
            ],
        };
        let wire = snapshot.encode_wire();
        let decoded = ChunkCircuitSnapshot::decode_wire(&wire).unwrap();
        assert_eq!(decoded, snapshot);
    }
}
