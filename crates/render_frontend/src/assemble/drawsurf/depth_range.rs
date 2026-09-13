use super::retained_list::RetainedDrawKind;
use super::tess::xmodel::XMODEL_OBJECT_ID_VIEWMODEL;

pub const GFX_DEPTH_RANGE_SCENE: i32 = 0;

pub const GFX_DEPTH_RANGE_VIEWMODEL: i32 = 2;

pub const GFX_DEPTH_RANGE_FULL: i32 = -1;

pub const DEPTH_RANGE_BAND: f32 = 0.015_625;

pub const RENDER_FX_DEPTH_HACK: u32 = 2;

pub fn d3d_depth_range(depth_range_type: i32) -> (f32, f32) {
    if depth_range_type == GFX_DEPTH_RANGE_SCENE {
        (DEPTH_RANGE_BAND, 1.0)
    } else {
        (0.0, DEPTH_RANGE_BAND)
    }
}

pub fn reverse_z_viewport_depth(depth_range_type: i32) -> (f32, f32) {
    let (d3d_near, d3d_far) = d3d_depth_range(depth_range_type);
    let a = 1.0 - d3d_far;
    let b = 1.0 - d3d_near;
    if a <= b { (a, b) } else { (b, a) }
}

pub fn depth_range_type_from_hack_flags(render_fx_flags: u32) -> i32 {
    if render_fx_flags & RENDER_FX_DEPTH_HACK != 0 {
        GFX_DEPTH_RANGE_VIEWMODEL
    } else {
        GFX_DEPTH_RANGE_SCENE
    }
}

pub fn depth_range_type_for_draw(kind: &RetainedDrawKind, key: u64) -> i32 {
    match kind {
        RetainedDrawKind::XModel { .. } => {
            let object_id = dpvs_iw4::GfxDrawSurf { packed: key }.object_id();
            depth_range_type_from_hack_flags(host_viewmodel_render_fx_flags(object_id))
        }
        _ => GFX_DEPTH_RANGE_SCENE,
    }
}

pub fn host_viewmodel_render_fx_flags(object_id: u16) -> u32 {
    if object_id == XMODEL_OBJECT_ID_VIEWMODEL {
        RENDER_FX_DEPTH_HACK
    } else {
        0
    }
}
