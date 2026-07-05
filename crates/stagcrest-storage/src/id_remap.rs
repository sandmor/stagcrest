use std::collections::HashMap;

use stagcrest_protocol::{BlockId, UNKNOWN_BLOCK_ID};

use crate::meta::BlockRegistryEntry;

/// Remap table built from a saved block registry snapshot and the live registry.
#[derive(Debug, Clone, Default)]
pub struct BlockIdRemap {
    map: HashMap<u32, BlockId>,
    unknown_id: BlockId,
}

impl BlockIdRemap {
    pub fn from_saved_registry(
        saved: &[BlockRegistryEntry],
        live_by_name: &HashMap<String, BlockId>,
        unknown_id: BlockId,
    ) -> Self {
        let mut map = HashMap::new();
        for entry in saved {
            let current = live_by_name
                .get(&entry.namespaced_id)
                .copied()
                .unwrap_or(unknown_id);
            map.insert(entry.id, current);
        }
        Self { map, unknown_id }
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (u32, String)>) -> Vec<BlockRegistryEntry> {
        let mut out: Vec<_> = entries
            .into_iter()
            .map(|(id, namespaced_id)| BlockRegistryEntry {
                id,
                namespaced_id,
            })
            .collect();
        out.sort_by_key(|e| e.id);
        out
    }

    pub fn remap(&self, id: BlockId) -> BlockId {
        self.map.get(&id.0).copied().unwrap_or(self.unknown_id)
    }

    pub fn remap_palette(&self, palette_ids: &mut [BlockId]) {
        for id in palette_ids {
            *id = self.remap(*id);
        }
    }
}

pub const UNKNOWN_BLOCK_NAMESPACED: &str = UNKNOWN_BLOCK_ID;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_by_name_survives_insertion() {
        let saved = vec![
            BlockRegistryEntry {
                id: 0,
                namespaced_id: "stagcrest:air".into(),
            },
            BlockRegistryEntry {
                id: 1,
                namespaced_id: "stagcrest:stone".into(),
            },
        ];
        let mut live = HashMap::new();
        live.insert("stagcrest:air".into(), BlockId(0));
        live.insert("stagcrest:stone".into(), BlockId(2));
        live.insert("stagcrest:basalt".into(), BlockId(1));
        let remap = BlockIdRemap::from_saved_registry(&saved, &live, BlockId(99));
        assert_eq!(remap.remap(BlockId(1)), BlockId(2));
    }
}
