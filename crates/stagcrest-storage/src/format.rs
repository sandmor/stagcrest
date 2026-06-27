use stagcrest_protocol::{BlockId, BlockState, LocalBlockPos, CHUNK_VOLUME};
use thiserror::Error;

pub const INACTIVE_CHUNK_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InactiveChunk {
    pub version: u8,
    pub palette_ids: Vec<BlockId>,
    pub palette_states: Vec<BlockState>,
    pub bits_per_index: u8,
    pub packed_indices: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum StorageFormatError {
    #[error("unsupported inactive chunk version: {0}")]
    UnsupportedVersion(u8),
    #[error("truncated wire data")]
    Truncated,
    #[error("invalid palette index {index} (palette len {palette_len})")]
    InvalidPaletteIndex { index: usize, palette_len: usize },
    #[error("palette ids/states length mismatch")]
    PaletteMismatch,
}

pub struct InactiveChunkReader<'a> {
    chunk: &'a InactiveChunk,
}

impl<'a> InactiveChunkReader<'a> {
    pub fn new(chunk: &'a InactiveChunk) -> Self {
        Self { chunk }
    }

    pub fn block_at(&self, local: LocalBlockPos) -> (BlockId, BlockState) {
        let idx = self.chunk.palette_index_at(local.index());
        let palette_idx = idx.min(self.chunk.palette_ids.len().saturating_sub(1));
        (
            self.chunk.palette_ids[palette_idx],
            self.chunk.palette_states[palette_idx],
        )
    }
}

impl InactiveChunk {
    pub fn from_indices(
        palette_ids: Vec<BlockId>,
        palette_states: Vec<BlockState>,
        indices: &[u16],
    ) -> Result<Self, StorageFormatError> {
        if palette_ids.len() != palette_states.len() {
            return Err(StorageFormatError::PaletteMismatch);
        }
        if indices.len() != CHUNK_VOLUME {
            return Err(StorageFormatError::InvalidPaletteIndex {
                index: indices.len(),
                palette_len: palette_ids.len(),
            });
        }
        let palette_len = palette_ids.len().max(1);
        let bits_per_index = bits_for_palette(palette_len);
        let mut packed_indices = vec![0u8; packed_byte_len(bits_per_index)];
        for (cell, &palette_idx) in indices.iter().enumerate() {
            let palette_idx = palette_idx as usize;
            if palette_idx >= palette_ids.len() {
                return Err(StorageFormatError::InvalidPaletteIndex {
                    index: palette_idx,
                    palette_len: palette_ids.len(),
                });
            }
            write_bits(
                &mut packed_indices,
                bits_per_index,
                cell,
                palette_idx as u32,
            );
        }
        Ok(Self {
            version: INACTIVE_CHUNK_VERSION,
            palette_ids,
            palette_states,
            bits_per_index,
            packed_indices,
        })
    }

    pub fn palette_index_at(&self, cell: usize) -> usize {
        read_bits(&self.packed_indices, self.bits_per_index, cell) as usize
    }

    pub fn to_indices(&self) -> Result<Vec<u16>, StorageFormatError> {
        let mut out = Vec::with_capacity(CHUNK_VOLUME);
        for cell in 0..CHUNK_VOLUME {
            let idx = self.palette_index_at(cell);
            if idx >= self.palette_ids.len() {
                return Err(StorageFormatError::InvalidPaletteIndex {
                    index: idx,
                    palette_len: self.palette_ids.len(),
                });
            }
            out.push(idx as u16);
        }
        Ok(out)
    }

    pub fn encode_wire(&self) -> Vec<u8> {
        let palette_count = self.palette_ids.len() as u16;
        let packed_len = self.packed_indices.len() as u32;
        let mut out = Vec::with_capacity(
            1 + 2 + self.palette_ids.len() * (4 + 2) + 1 + 4 + self.packed_indices.len(),
        );
        out.push(self.version);
        out.extend_from_slice(&palette_count.to_le_bytes());
        for (&id, &state) in self.palette_ids.iter().zip(self.palette_states.iter()) {
            out.extend_from_slice(&id.0.to_le_bytes());
            out.extend_from_slice(&state.0.to_le_bytes());
        }
        out.push(self.bits_per_index);
        out.extend_from_slice(&packed_len.to_le_bytes());
        out.extend_from_slice(&self.packed_indices);
        out
    }

    pub fn decode_wire(bytes: &[u8]) -> Result<Self, StorageFormatError> {
        let mut cursor = 0usize;
        let version = read_u8(bytes, &mut cursor)?;
        if version != INACTIVE_CHUNK_VERSION {
            return Err(StorageFormatError::UnsupportedVersion(version));
        }
        let palette_count = read_u16(bytes, &mut cursor)? as usize;
        let mut palette_ids = Vec::with_capacity(palette_count);
        let mut palette_states = Vec::with_capacity(palette_count);
        for _ in 0..palette_count {
            palette_ids.push(BlockId(read_u32(bytes, &mut cursor)?));
            palette_states.push(BlockState(read_u16(bytes, &mut cursor)?));
        }
        let bits_per_index = read_u8(bytes, &mut cursor)?;
        let packed_len = read_u32(bytes, &mut cursor)? as usize;
        if cursor + packed_len > bytes.len() {
            return Err(StorageFormatError::Truncated);
        }
        let packed_indices = bytes[cursor..cursor + packed_len].to_vec();
        Ok(Self {
            version,
            palette_ids,
            palette_states,
            bits_per_index,
            packed_indices,
        })
    }
}

fn bits_for_palette(palette_len: usize) -> u8 {
    if palette_len <= 1 {
        1
    } else {
        (usize::BITS - (palette_len - 1).leading_zeros()) as u8
    }
}

fn packed_byte_len(bits_per_index: u8) -> usize {
    (CHUNK_VOLUME * bits_per_index as usize).div_ceil(8)
}

fn write_bits(buf: &mut [u8], bits_per_index: u8, cell: usize, value: u32) {
    let bits = bits_per_index as usize;
    let bit_offset = cell * bits;
    for bit in 0..bits {
        let global_bit = bit_offset + bit;
        let byte_idx = global_bit / 8;
        let bit_in_byte = global_bit % 8;
        if value & (1 << bit) != 0 {
            buf[byte_idx] |= 1 << bit_in_byte;
        }
    }
}

fn read_bits(buf: &[u8], bits_per_index: u8, cell: usize) -> u32 {
    let bits = bits_per_index as usize;
    let bit_offset = cell * bits;
    let mut value = 0u32;
    for bit in 0..bits {
        let global_bit = bit_offset + bit;
        let byte_idx = global_bit / 8;
        let bit_in_byte = global_bit % 8;
        if buf[byte_idx] & (1 << bit_in_byte) != 0 {
            value |= 1 << bit;
        }
    }
    value
}

fn read_u8(bytes: &[u8], cursor: &mut usize) -> Result<u8, StorageFormatError> {
    let b = *bytes.get(*cursor).ok_or(StorageFormatError::Truncated)?;
    *cursor += 1;
    Ok(b)
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> Result<u16, StorageFormatError> {
    let end = cursor.saturating_add(2);
    let slice = bytes
        .get(*cursor..end)
        .ok_or(StorageFormatError::Truncated)?;
    *cursor = end;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, StorageFormatError> {
    let end = cursor.saturating_add(4);
    let slice = bytes
        .get(*cursor..end)
        .ok_or(StorageFormatError::Truncated)?;
    *cursor = end;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stagcrest_protocol::CHUNK_SIZE;

    #[test]
    fn pack_unpack_roundtrip() {
        let palette_ids = vec![BlockId(0), BlockId(1), BlockId(2)];
        let palette_states = vec![BlockState(0), BlockState(0), BlockState(1)];
        let mut indices = vec![0u16; CHUNK_VOLUME];
        indices[0] = 1;
        indices[1] = 2;
        indices[CHUNK_VOLUME - 1] = 1;

        let packed =
            InactiveChunk::from_indices(palette_ids.clone(), palette_states.clone(), &indices)
                .unwrap();
        let back = packed.to_indices().unwrap();
        assert_eq!(back, indices);

        let reader = InactiveChunkReader::new(&packed);
        let b0 = reader.block_at(LocalBlockPos { x: 0, y: 0, z: 0 });
        assert_eq!(b0, (BlockId(1), BlockState(0)));
    }

    #[test]
    fn wire_roundtrip() {
        let palette_ids = vec![BlockId(0), BlockId(5)];
        let palette_states = vec![BlockState(0), BlockState(3)];
        let mut indices = vec![0u16; CHUNK_VOLUME];
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let idx = (x + z * CHUNK_SIZE as i32) as usize;
                indices[idx] = (x % 2) as u16;
            }
        }
        let chunk = InactiveChunk::from_indices(palette_ids, palette_states, &indices).unwrap();
        let wire = chunk.encode_wire();
        let decoded = InactiveChunk::decode_wire(&wire).unwrap();
        assert_eq!(decoded, chunk);
    }
}
