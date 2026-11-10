use crate::{ShadowDrawListWork, draw_surf_list_work_shadow};
use render_frame::{PackedFrontendLists, SunShadowAtlasProfile, SunShadowViewport};

pub const SUN_SHADOW_FORCED_PROFILE: SunShadowAtlasProfile = SunShadowAtlasProfile::Large;

pub const D3DRS_SCISSORTESTENABLE: u32 = 0xae;

pub const SHADOWMAP_CLEAR_FLAGS: u32 = 0x1 | 0x2;

pub const SHADOWMAP_CLEAR_Z: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct D3dScissorRect {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

pub fn set_viewport_and_scissor(viewport: SunShadowViewport) -> D3dScissorRect {
    D3dScissorRect {
        left: viewport.x,
        top: viewport.y,
        right: viewport.x.saturating_add(viewport.width),
        bottom: viewport.y.saturating_add(viewport.height),
    }
}

#[derive(Clone, Debug)]
pub struct SunShadowPartitionPass {
    pub render_target_id: u8,
    pub cleared: bool,
    pub scissor: D3dScissorRect,
    pub viewport: SunShadowViewport,
    pub work: Option<ShadowDrawListWork>,
}

pub fn draw_sun_shadow_map_partition(
    partition: u32,
    profile: SunShadowAtlasProfile,
    packed: Option<&PackedFrontendLists>,
) -> Option<SunShadowPartitionPass> {
    let viewport = profile.partition_viewport(partition)?;
    let scissor = set_viewport_and_scissor(viewport);
    Some(SunShadowPartitionPass {
        render_target_id: profile.render_target_id(),
        cleared: partition == 0,
        scissor,
        viewport,
        work: packed.map(draw_surf_list_work_shadow),
    })
}

pub fn draw_sun_shadow_map_forced(
    partition: u32,
    packed: Option<&PackedFrontendLists>,
) -> Option<SunShadowPartitionPass> {
    draw_sun_shadow_map_partition(partition, SUN_SHADOW_FORCED_PROFILE, packed)
}
