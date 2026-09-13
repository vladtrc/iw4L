use render_backend::{PackDraw, PackKind, pack_sun_shadow_frontend as pack_lists};
use render_frame::PackedFrontendLists;
use render_frame::{RetainedDrawItem, RetainedDrawKind};

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
    scratch.extend(draws.into_iter().map(pack_draw));
    pack_lists(
        scratch,
        world_run_surfs,
        world_ranges,
        world_vertex_count,
        smodel_ranges,
        xmodel_ranges,
    )
}

fn pack_draw(draw: &RetainedDrawItem) -> PackDraw {
    let kind = match draw.kind {
        RetainedDrawKind::World {
            surf, run, run_off, ..
        } => PackKind::World { surf, run, run_off },
        RetainedDrawKind::Smodel {
            surface,
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Rigid),
            ..
        } => PackKind::SmodelRigid {
            surface,
            lighting_handle,
        },
        RetainedDrawKind::Smodel {
            surface,
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Skinned),
            ..
        } => PackKind::SmodelSkinned {
            surface,
            lighting_handle,
        },
        RetainedDrawKind::Smodel {
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Pretess),
            pretess: Some(dest),
            ..
        } => PackKind::SmodelPretess {
            lighting_handle,
            dest,
        },
        RetainedDrawKind::Smodel {
            lighting_handle,
            stream: Some(lighting_iw4::SmodelSurfPath::Cached),
            pretess: Some(dest),
            ..
        } => PackKind::SmodelCached {
            lighting_handle,
            dest,
        },
        RetainedDrawKind::XModel {
            surface,
            lighting_handle,
            ..
        } => PackKind::XModel {
            surface,
            lighting_handle,
        },
        _ => PackKind::Skip,
    };
    PackDraw {
        key: draw.key,
        material_rank: draw.material_rank,
        kind,
    }
}
