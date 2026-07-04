use stagcrest_protocol::{TextureAnimation, TextureDef};

/// Downscale a texture so `max(width,height) <= max_dimension`, preserving vertical
/// animation strip layout when metadata is present.
pub fn downscale_texture(tex: &TextureDef, max_dimension: u32) -> TextureDef {
    let max_dim = max_dimension.max(1);
    if tex.width <= max_dim && tex.height <= max_dim {
        return tex.clone();
    }

    if let Some(anim) = tex.animation.as_ref() {
        if anim.frame_height > 0
            && anim.frame_width > 0
            && tex.height >= anim.frame_height
            && tex.width == anim.frame_width
            && tex.height % anim.frame_height == 0
        {
            return downscale_vertical_strip(tex, anim, max_dim);
        }
    }

    downscale_uniform(tex, max_dim)
}

fn downscale_uniform(tex: &TextureDef, max_dim: u32) -> TextureDef {
    let scale = (tex.width.max(tex.height) as f32 / max_dim as f32).ceil();
    let scale = scale.max(1.0) as u32;
    let nw = (tex.width / scale).max(1);
    let nh = (tex.height / scale).max(1);
    resize_nearest(tex, nw, nh, tex.animation.clone())
}

fn downscale_vertical_strip(tex: &TextureDef, anim: &TextureAnimation, max_dim: u32) -> TextureDef {
    let fw = anim.frame_width.max(1);
    let fh = anim.frame_height.max(1);
    let frame_count = anim.frame_count.max(1);

    let mut target_fw = fw;
    let mut target_fh = fh;
    if fw > max_dim || fh > max_dim {
        let frame_scale = (fw.max(fh) as f32 / max_dim as f32).ceil().max(1.0) as u32;
        target_fw = (fw / frame_scale).max(1);
        target_fh = (fh / frame_scale).max(1);
    }

    let nh = target_fh.saturating_mul(frame_count).max(1);
    let animation = Some(TextureAnimation {
        frame_width: target_fw,
        frame_height: target_fh,
        frame_count,
        frametime_ticks: anim.frametime_ticks,
    });
    resize_nearest(tex, target_fw, nh, animation)
}

fn resize_nearest(
    tex: &TextureDef,
    new_w: u32,
    new_h: u32,
    animation: Option<TextureAnimation>,
) -> TextureDef {
    let mut out = vec![0u8; (new_w * new_h * 4) as usize];
    for y in 0..new_h {
        for x in 0..new_w {
            let sx = (x as u64 * tex.width as u64 / new_w.max(1) as u64) as u32;
            let sy = (y as u64 * tex.height as u64 / new_h.max(1) as u64) as u32;
            let si = ((sy * tex.width + sx) * 4) as usize;
            let di = ((y * new_w + x) * 4) as usize;
            if si + 3 < tex.rgba.len() && di + 3 < out.len() {
                out[di..di + 4].copy_from_slice(&tex.rgba[si..si + 4]);
            }
        }
    }
    TextureDef {
        id: tex.id,
        namespaced_id: tex.namespaced_id.clone(),
        width: new_w,
        height: new_h,
        rgba: out,
        animation,
    }
}
