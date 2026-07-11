//! Helpers for sampling the depth prepass texture in custom render passes.

use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::render::render_resource::{TextureAspect, TextureView, TextureViewDescriptor};

/// Returns a depth-only view of the prepass depth texture for `texture_depth_2d` sampling.
///
/// The main [`bevy::render::view::ViewDepthTexture`] is a render attachment and is not
/// guaranteed to include [`bevy::render::render_resource::TextureUsages::TEXTURE_BINDING`].
pub fn prepass_depth_sample_view(prepass: &ViewPrepassTextures) -> Option<TextureView> {
    let depth = prepass.depth.as_ref()?;
    Some(depth.texture.texture.create_view(&TextureViewDescriptor {
        label: Some("prepass_depth_sample"),
        aspect: TextureAspect::DepthOnly,
        ..TextureViewDescriptor::default()
    }))
}
