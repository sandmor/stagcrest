use bevy::prelude::*;
use stagcrest_mod_host::BlockRegistry;
use stagcrest_protocol::BlockId;

pub const MAIN_SLOTS: usize = 27;
pub const HOTBAR_SLOTS: usize = 9;

#[derive(Resource)]
pub struct CreativeInventory {
    pub main: [Option<BlockId>; MAIN_SLOTS],
    pub hotbar: [Option<BlockId>; HOTBAR_SLOTS],
    pub selected_index: usize,
}

impl CreativeInventory {
    pub fn from_placeable(placeable: &[BlockId]) -> Self {
        let mut hotbar = [None; HOTBAR_SLOTS];
        for (slot, &id) in hotbar.iter_mut().zip(placeable.iter().take(HOTBAR_SLOTS)) {
            *slot = Some(id);
        }
        Self {
            main: [None; MAIN_SLOTS],
            hotbar,
            selected_index: 0,
        }
    }

    pub fn selected_block(&self) -> Option<BlockId> {
        self.hotbar[self.selected_index]
    }

    pub fn get_slot(&self, kind: SlotKind) -> Option<BlockId> {
        match kind {
            SlotKind::Main(i) => self.main[i],
            SlotKind::Hotbar(i) => self.hotbar[i],
        }
    }

    pub fn set_slot(&mut self, kind: SlotKind, block: Option<BlockId>) {
        match kind {
            SlotKind::Main(i) => self.main[i] = block,
            SlotKind::Hotbar(i) => self.hotbar[i] = block,
        }
    }

    pub fn pick_up(&mut self, kind: SlotKind) -> Option<BlockId> {
        let block = self.get_slot(kind);
        if block.is_some() {
            self.set_slot(kind, None);
        }
        block
    }

    pub fn place(&mut self, kind: SlotKind, block: BlockId) -> Option<BlockId> {
        let previous = self.get_slot(kind);
        self.set_slot(kind, Some(block));
        previous
    }

    pub fn swap(&mut self, a: SlotKind, b: SlotKind) {
        let va = self.get_slot(a);
        let vb = self.get_slot(b);
        self.set_slot(a, vb);
        self.set_slot(b, va);
    }

    pub fn first_empty_hotbar(&self) -> Option<usize> {
        self.hotbar.iter().position(|s| s.is_none())
    }

    pub fn first_empty_main(&self) -> Option<usize> {
        self.main.iter().position(|s| s.is_none())
    }

    pub fn quick_assign_hotbar(&mut self, block: BlockId) -> SlotKind {
        if let Some(i) = self.first_empty_hotbar() {
            self.hotbar[i] = Some(block);
            return SlotKind::Hotbar(i);
        }
        let i = self.selected_index;
        self.hotbar[i] = Some(block);
        SlotKind::Hotbar(i)
    }

    pub fn quick_assign_any(&mut self, block: BlockId) -> SlotKind {
        if let Some(i) = self.first_empty_hotbar() {
            self.hotbar[i] = Some(block);
            return SlotKind::Hotbar(i);
        }
        if let Some(i) = self.first_empty_main() {
            self.main[i] = Some(block);
            return SlotKind::Main(i);
        }
        let i = self.selected_index;
        self.hotbar[i] = Some(block);
        SlotKind::Hotbar(i)
    }

    pub fn shift_move_slot(&mut self, kind: SlotKind) {
        match kind {
            SlotKind::Hotbar(i) => {
                if let Some(block) = self.hotbar[i] {
                    if let Some(j) = self.first_empty_main() {
                        self.main[j] = Some(block);
                        self.hotbar[i] = None;
                    } else {
                        self.swap(SlotKind::Hotbar(i), SlotKind::Hotbar(self.selected_index));
                    }
                }
            }
            SlotKind::Main(i) => {
                if let Some(block) = self.main[i] {
                    if let Some(j) = self.first_empty_hotbar() {
                        self.hotbar[j] = Some(block);
                        self.main[i] = None;
                    } else {
                        self.swap(SlotKind::Main(i), SlotKind::Hotbar(self.selected_index));
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotKind {
    Main(usize),
    Hotbar(usize),
}

#[derive(Resource, Default)]
pub struct InventoryUiState {
    pub open: bool,
    pub search: String,
    /// Block currently "held" on the mouse cursor (click-to-carry model).
    pub cursor: Option<BlockId>,
}

pub fn filtered_placeable(registry: &BlockRegistry, search: &str) -> Vec<BlockId> {
    let query = search.trim().to_lowercase();
    registry
        .placeable_blocks()
        .iter()
        .copied()
        .filter(|&id| {
            if query.is_empty() {
                return true;
            }
            let Some(def) = registry.block(id) else {
                return false;
            };
            def.display_name.to_lowercase().contains(&query)
                || def.namespaced_id.to_lowercase().contains(&query)
        })
        .collect()
}

pub fn inventory_open(state: Option<Res<InventoryUiState>>) -> bool {
    state.is_some_and(|s| s.open)
}
