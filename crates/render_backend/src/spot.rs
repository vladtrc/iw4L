use crate::dip::{D3dDrawIndexedPrimitive, r_draw_indexed_from_shadow_work};
use crate::partition::{D3dScissorRect, r_set_viewport_and_scissor};
use crate::{ShadowDrawListWork, r_draw_surf_list_work_shadow};
use lighting_iw4::SpotShadowSlotPlan;
use render_frame::{PackedFrontendLists, SunShadowViewport};

#[derive(Clone, Debug)]
pub struct SpotShadowMapPass {
    pub slot_index: u32,
    pub render_target_id: u8,
    pub cleared: bool,
    pub scissor: D3dScissorRect,
    pub viewport: SunShadowViewport,
    pub work: Option<ShadowDrawListWork>,
    pub dips: Vec<D3dDrawIndexedPrimitive>,
}

fn plan_viewport(plan: SpotShadowSlotPlan) -> SunShadowViewport {
    SunShadowViewport {
        x: plan.viewport_x,
        y: plan.viewport_y,
        width: plan.viewport_size,
        height: plan.viewport_size,
    }
}

pub fn r_draw_spot_shadow_map(
    slot_index: u32,
    plan: SpotShadowSlotPlan,
    packed: Option<&PackedFrontendLists>,
) -> SpotShadowMapPass {
    let viewport = plan_viewport(plan);
    let scissor = r_set_viewport_and_scissor(viewport);
    let (work, dips) = if let Some(packed) = packed {
        let work = r_draw_surf_list_work_shadow(packed);
        let dips = r_draw_indexed_from_shadow_work(&work);
        (Some(work), dips)
    } else {
        (None, Vec::new())
    };
    SpotShadowMapPass {
        slot_index,
        render_target_id: plan.render_target_id,
        cleared: plan.clear,
        scissor,
        viewport,
        work,
        dips,
    }
}
