mod torch;

use stagcrest_protocol::{BlockModel, BlockState, ModelId, ModelVariant, torch_attachment};

pub use torch::ModelRegistry;

pub fn model_variant_for_block(namespaced_id: &str, state: BlockState) -> ModelVariant {
    if namespaced_id == "stagcrest:redstone_torch" {
        torch::torch_variant_from_attachment(torch_attachment(state))
    } else {
        0
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
