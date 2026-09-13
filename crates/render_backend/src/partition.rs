use crate::dip::{D3dDrawIndexedPrimitive, r_draw_indexed_from_shadow_work};
use crate::{ShadowDrawListWork, r_draw_surf_list_work_shadow};
use render_frame::{PackedFrontendLists, SunShadowAtlasProfile, SunShadowViewport};

pub const SUN_SHADOW_FORCED_PROFILE: SunShadowAtlasProfile = SunShadowAtlasProfile::Large;

pub const D3DRS_SCISSORTESTENABLE: u32 = 0xae;

pub const SHADOWMAP_CLEAR_FLAGS: u32 = 0x1 | 0x2;

pub const SHADOWMAP_CLEAR_Z: f32 = 1.0;

pub const SHADOWMAP_RESTORE_VIEWPORT: SunShadowViewport = SunShadowViewport {
    x: 0,
    y: 0,
    width: 0x1000,
    height: 0x1000,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dScissorRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

pub fn r_set_viewport_and_scissor(viewport: SunShadowViewport) -> D3dScissorRect {
    D3dScissorRect {
        left: viewport.x,
        top: viewport.y,
        right: viewport.x.saturating_add(viewport.width),
        bottom: viewport.y.saturating_add(viewport.height),
    }
}

#[derive(Clone, Debug)]
pub struct SunShadowPartitionPass {
    pub partition: u32,

    pub render_target_id: u8,
    pub cleared: bool,
    pub scissor: D3dScissorRect,
    pub viewport: SunShadowViewport,
    pub restore_viewport: SunShadowViewport,
    pub work: Option<ShadowDrawListWork>,
    pub dips: Vec<D3dDrawIndexedPrimitive>,
}

pub fn r_draw_sun_shadow_map_partition(
    partition: u32,
    profile: SunShadowAtlasProfile,
    packed: Option<&PackedFrontendLists>,
) -> Option<SunShadowPartitionPass> {
    let viewport = profile.partition_viewport(partition)?;
    let scissor = r_set_viewport_and_scissor(viewport);
    let (work, dips) = if let Some(packed) = packed {
        let work = r_draw_surf_list_work_shadow(packed);
        let dips = r_draw_indexed_from_shadow_work(&work);
        (Some(work), dips)
    } else {
        (None, Vec::new())
    };
    Some(SunShadowPartitionPass {
        partition,
        render_target_id: profile.render_target_id(),
        cleared: partition == 0,
        scissor,
        viewport,
        restore_viewport: SHADOWMAP_RESTORE_VIEWPORT,
        work,
        dips,
    })
}

pub fn r_draw_sun_shadow_map_forced(
    partition: u32,
    packed: Option<&PackedFrontendLists>,
) -> Option<SunShadowPartitionPass> {
    r_draw_sun_shadow_map_partition(partition, SUN_SHADOW_FORCED_PROFILE, packed)
}
