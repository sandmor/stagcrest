mod mount;
mod observer;
mod piston;
mod repeater;
mod torch;

use stagcrest_protocol::{
    mount_variant, observer_variant, piston_head_variant, piston_variant, repeater_variant,
    torch_attachment, BlockModel, BlockState, ModelId, ModelVariant,
};

pub fn model_variant_for_block(namespaced_id: &str, state: BlockState) -> ModelVariant {
    match namespaced_id {
        "stagcrest:redstone_torch" => torch::torch_variant_from_attachment(torch_attachment(state)),
        "stagcrest:lever" | "stagcrest:stone_button" => mount_variant(state),
        "stagcrest:repeater" => repeater_variant(state),
        "stagcrest:observer" => observer_variant(state),
        "stagcrest:piston" | "stagcrest:sticky_piston" => piston_variant(state),
        "stagcrest:piston_head" => piston_head_variant(state),
        _ => 0,
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
                let idx = variant.min(4) as usize;
                &self.redstone_torch[idx]
            }
            ModelId::Lever => {
                let idx = (variant as usize).min(self.lever.len() - 1);
                &self.lever[idx]
            }
            ModelId::Button => {
                let idx = (variant as usize).min(self.button.len() - 1);
                &self.button[idx]
            }
            ModelId::Repeater => {
                let idx = (variant as usize).min(self.repeater.len() - 1);
                &self.repeater[idx]
            }
            ModelId::Observer => {
                let idx = (variant as usize).min(self.observer.len() - 1);
                &self.observer[idx]
            }
            ModelId::Piston => {
                let idx = (variant as usize).min(self.piston.len() - 1);
                &self.piston[idx]
            }
            ModelId::PistonHead => {
                let idx = (variant as usize).min(self.piston_head.len() - 1);
                &self.piston_head[idx]
            }
        }
    }
}
