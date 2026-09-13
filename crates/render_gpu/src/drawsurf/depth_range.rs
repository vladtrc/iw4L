use render_frame::{RENDER_FX_DEPTH_HACK, RetainedDrawKind, host_viewmodel_render_fx_flags};

pub const GFX_DEPTH_RANGE_SCENE: i32 = 0;

pub const GFX_DEPTH_RANGE_VIEWMODEL: i32 = 2;

pub fn reverse_z_viewport_depth(depth_range_type: i32) -> (f32, f32) {
    let (d3d_near, d3d_far) = if depth_range_type == GFX_DEPTH_RANGE_SCENE {
        (render_backend::DEPTH_RANGE_BAND, 1.0)
    } else {
        (0.0, render_backend::DEPTH_RANGE_BAND)
    };
    let a = 1.0 - d3d_far;
    let b = 1.0 - d3d_near;
    if a <= b { (a, b) } else { (b, a) }
}

pub fn depth_range_type_for_draw(kind: &RetainedDrawKind, key: u64) -> i32 {
    match kind {
        RetainedDrawKind::XModel { .. } => {
            let object_id = dpvs_iw4::GfxDrawSurf { packed: key }.object_id();
            if host_viewmodel_render_fx_flags(object_id) & RENDER_FX_DEPTH_HACK != 0 {
                GFX_DEPTH_RANGE_VIEWMODEL
            } else {
                GFX_DEPTH_RANGE_SCENE
            }
        }
        _ => GFX_DEPTH_RANGE_SCENE,
    }
}
