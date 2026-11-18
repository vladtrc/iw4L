use render_backend::{PackDraw, pack_sun_shadow_frontend as pack_lists};
use render_frame::PackedFrontendLists;
use render_frame::RetainedDrawItem;

pub fn pack_sun_shadow_frontend<'a>(
    draws: impl IntoIterator<Item = &'a RetainedDrawItem>,
    world_run_surfs: &[u16],
    world_ranges: &[(u32, u32)],
    world_vertex_count: u32,
    smodel_ranges: &[(u32, u32)],
    xmodel_ranges: &[(u32, u32)],
    scratch: &mut Vec<PackDraw>,
) -> PackedFrontendLists {
    scratch.clear();
    scratch.extend(draws.into_iter().map(PackDraw::from));
    pack_lists(
        scratch,
        world_run_surfs,
        world_ranges,
        world_vertex_count,
        smodel_ranges,
        xmodel_ranges,
    )
}
