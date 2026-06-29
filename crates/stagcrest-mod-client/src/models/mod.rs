mod mount;
mod observer;
mod piston;
mod repeater;
mod torch;

use stagcrest_protocol::{
    mount_variant, observer_state, observer_variant, piston_head_variant, piston_variant,
    repeater_state, repeater_variant, torch_state, BlockModel, BlockState, Facing, ModelId,
    ModelVariant, TorchAttachment,
};

/// Number of redstone-torch attachment poses (floor + 4 walls).
pub const TORCH_ATTACHMENT_COUNT: u8 = 5;

pub fn model_variant_for_block(namespaced_id: &str, state: BlockState) -> ModelVariant {
    match namespaced_id {
        "stagcrest:redstone_torch" => torch::torch_variant(state),
        "stagcrest:lever" | "stagcrest:stone_button" => mount_variant(state),
        "stagcrest:repeater" => repeater_variant(state),
        "stagcrest:observer" => observer_variant(state),
        "stagcrest:piston" | "stagcrest:sticky_piston" => piston_variant(state),
        "stagcrest:piston_head" => piston_head_variant(state),
        _ => 0,
    }
}

/// Reconstruct a representative `BlockState` for a given model variant. Used at
/// bake time so each variant samples the correct (e.g. lit vs unlit) textures.
/// Only models whose textures vary by state need exact reconstruction; the rest
/// fall back to `BlockState(0)` since their textures are state-independent.
pub fn representative_state(id: ModelId, variant: ModelVariant) -> BlockState {
    match id {
        ModelId::RedstoneTorch => {
            let lit = variant >= TORCH_ATTACHMENT_COUNT;
            let attachment = TorchAttachment::from_bits((variant % TORCH_ATTACHMENT_COUNT) as u16);
            torch_state(lit, attachment)
        }
        ModelId::Repeater => {
            let delay = ((variant >> 3) & 0b11) + 1;
            let powered = (variant >> 2) & 1 != 0;
            let facing = Facing::from_bits((variant & 0b11) as u16);
            repeater_state(powered, facing, delay)
        }
        ModelId::Observer => {
            let powered = (variant >> 2) & 1 != 0;
            let facing = Facing::from_bits((variant & 0b11) as u16);
            observer_state(powered, facing)
        }
        _ => BlockState(0),
    }
}

pub fn resolve_block_model<'a>(
    registry: &'a ModelRegistry,
    id: ModelId,
    namespaced_id: &str,
    state: BlockState,
) -> &'a BlockModel {
    let variant = model_variant_for_block(namespaced_id, state);
    registry.get(id, variant)
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    redstone_torch: [BlockModel; 5],
    lever: Vec<BlockModel>,
    button: Vec<BlockModel>,
    repeater: Vec<BlockModel>,
    observer: Vec<BlockModel>,
    piston: Vec<BlockModel>,
    piston_head: Vec<BlockModel>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self {
            redstone_torch: torch::build_redstone_torch_models(),
            lever: mount::build_lever_models(),
            button: mount::build_button_models(),
            repeater: repeater::build_repeater_models(),
            observer: observer::build_observer_models(),
            piston: piston::build_piston_models(),
            piston_head: piston::build_piston_head_models(),
        }
    }

    pub fn get(&self, id: ModelId, variant: ModelVariant) -> &BlockModel {
        match id {
            ModelId::RedstoneTorch => {
                // Lit/unlit share geometry; variants 5..9 are the lit poses.
                let idx = (variant % TORCH_ATTACHMENT_COUNT).min(4) as usize;
                &self.redstone_torch[idx]
            }
            ModelId::Lever => {
                let idx = (variant as usize).min(self.lever.len().saturating_sub(1));
                &self.lever[idx]
            }
            ModelId::Button => {
                let idx = (variant as usize).min(self.button.len().saturating_sub(1));
                &self.button[idx]
            }
            ModelId::Repeater => {
                let idx = (variant as usize).min(self.repeater.len().saturating_sub(1));
                &self.repeater[idx]
            }
            ModelId::Observer => {
                let idx = (variant as usize).min(self.observer.len().saturating_sub(1));
                &self.observer[idx]
            }
            ModelId::Piston => {
                let idx = (variant as usize).min(self.piston.len().saturating_sub(1));
                &self.piston[idx]
            }
            ModelId::PistonHead => {
                let idx = (variant as usize).min(self.piston_head.len().saturating_sub(1));
                &self.piston_head[idx]
            }
        }
    }

    pub fn lever_count(&self) -> usize {
        self.lever.len()
    }

    pub fn button_count(&self) -> usize {
        self.button.len()
    }

    pub fn repeater_count(&self) -> usize {
        self.repeater.len()
    }

    pub fn observer_count(&self) -> usize {
        self.observer.len()
    }

    pub fn piston_count(&self) -> usize {
        self.piston.len()
    }

    pub fn piston_head_count(&self) -> usize {
        self.piston_head.len()
    }

    pub fn variant_count(&self, id: ModelId) -> usize {
        match id {
            ModelId::RedstoneTorch => (TORCH_ATTACHMENT_COUNT * 2) as usize,
            ModelId::Lever => self.lever.len(),
            ModelId::Button => self.button.len(),
            ModelId::Repeater => self.repeater.len(),
            ModelId::Observer => self.observer.len(),
            ModelId::Piston => self.piston.len(),
            ModelId::PistonHead => self.piston_head.len(),
        }
    }

    pub fn iter_all_variants(&self) -> impl Iterator<Item = (ModelId, ModelVariant, &BlockModel)> {
        let torch = (0..(TORCH_ATTACHMENT_COUNT * 2)).map(|i| {
            let m = &self.redstone_torch[(i % TORCH_ATTACHMENT_COUNT).min(4) as usize];
            (ModelId::RedstoneTorch, i as ModelVariant, m)
        });
        let lever = self.lever.iter().enumerate().map(|(i, m)| {
            (ModelId::Lever, i as ModelVariant, m)
        });
        let button = self.button.iter().enumerate().map(|(i, m)| {
            (ModelId::Button, i as ModelVariant, m)
        });
        let repeater = self.repeater.iter().enumerate().map(|(i, m)| {
            (ModelId::Repeater, i as ModelVariant, m)
        });
        let observer = self.observer.iter().enumerate().map(|(i, m)| {
            (ModelId::Observer, i as ModelVariant, m)
        });
        let piston = self.piston.iter().enumerate().map(|(i, m)| {
            (ModelId::Piston, i as ModelVariant, m)
        });
        let piston_head = self.piston_head.iter().enumerate().map(|(i, m)| {
            (ModelId::PistonHead, i as ModelVariant, m)
        });
        torch
            .chain(lever)
            .chain(button)
            .chain(repeater)
            .chain(observer)
            .chain(piston)
            .chain(piston_head)
    }
}
