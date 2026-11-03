use crate::assemble::pack::{PackDraw, PackKind};
use dpvs_iw4::GfxDrawSurf;
use dpvs_iw4::material_sort_key_row;
use render_frame::{RetainedDrawItem, RetainedDrawKind};

pub(crate) fn pack_draw(draw: &RetainedDrawItem) -> PackDraw {
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

pub(super) fn with_catalog(
    key: u64,
    material_rank: u32,
    kind: RetainedDrawKind,
    surface_samplers: crate::assemble::drawsurf::SurfaceSamplerInputs,
    catalog: &crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog,
) -> RetainedDrawItem {
    RetainedDrawItem {
        material_id: catalog
            .material_for_sorted_ordinal(material_rank)
            .map(|material| material.asset_id),
        key,
        material_rank,
        kind,
        surface_samplers,
        camera_region: crate::assemble::drawsurf::material_runtime::resolve_sorted_material(
            catalog,
            render_material::MaterialDrawKey::new(key, material_rank),
        )
        .ok()
        .map(|material| material.draw_rules.colour_camera_region),
    }
}

pub(super) fn item_uses_distortion_lane(
    mat_sort_key: u8,
    world_distortion_key: Option<u32>,
) -> bool {
    world_distortion_key == Some(u32::from(material_sort_key_row(mat_sort_key)))
}

pub(super) fn push_direct_lane_item(
    colour: &mut Vec<RetainedDrawItem>,
    emissive: &mut Vec<RetainedDrawItem>,
    distortion: Option<&mut Vec<RetainedDrawItem>>,
    item: RetainedDrawItem,
) {
    if let Some(distortion) = distortion {
        distortion.push(item);
    }
    match crate::assemble::drawsurf::frame_product_kind_for_camera_region(item.camera_region) {
        Some(crate::assemble::drawsurf::FrameProductKind::Emissive) => emissive.push(item),
        Some(crate::assemble::drawsurf::FrameProductKind::Colour) => colour.push(item),
        Some(_) | None => {}
    }
}

pub(crate) fn retained_draw_order_tie(kind: &RetainedDrawKind) -> u32 {
    match kind {
        RetainedDrawKind::World { surf, .. } => u32::from(*surf),
        RetainedDrawKind::Smodel { surface, .. } | RetainedDrawKind::XModel { surface, .. } => {
            *surface
        }
        RetainedDrawKind::CodeMesh { draw, .. }
        | RetainedDrawKind::ParticleCloud { draw, .. }
        | RetainedDrawKind::MarkMesh { draw, .. }
        | RetainedDrawKind::Glass { draw, .. } => *draw,
    }
}

pub(crate) fn mix_draw_membership(id: &mut u64, item: &RetainedDrawItem) {
    crate::assemble::drawsurf::list::mix_content_id(id, item.key);
    crate::assemble::drawsurf::list::mix_content_id(id, u64::from(item.material_rank));
    crate::assemble::drawsurf::list::mix_content_id(
        id,
        item.material_id.map_or(0, |id| u64::from(id.0) + 1),
    );
    let encode = |value: Option<u8>| value.map_or(0, |value| u64::from(value) + 1);
    crate::assemble::drawsurf::list::mix_content_id(
        id,
        encode(item.surface_samplers.reflection_probe.map(|value| value.0)),
    );
    crate::assemble::drawsurf::list::mix_content_id(
        id,
        encode(item.surface_samplers.primary_lightmap.map(|value| value.0)),
    );
    crate::assemble::drawsurf::list::mix_content_id(
        id,
        encode(
            item.surface_samplers
                .secondary_lightmap
                .map(|value| value.0),
        ),
    );
    let (kind_tag, a, b, c) = match item.kind {
        RetainedDrawKind::World {
            surf,
            run,
            run_off,
            brush_model,
            ..
        } => (
            0u64,
            u64::from(surf) | (u64::from(run) << 16),
            u64::from(run_off),
            brush_model.map_or(0, |model| u64::from(model) + 1),
        ),
        RetainedDrawKind::Smodel {
            placement,
            surface,
            lighting_handle,
            stream,
            pretess,
            ..
        } => {
            let stream_tag = match stream {
                Some(lighting_iw4::SmodelSurfPath::Rigid) => 1,
                Some(lighting_iw4::SmodelSurfPath::Skinned) => 2,
                Some(lighting_iw4::SmodelSurfPath::Cached) => 3,
                Some(lighting_iw4::SmodelSurfPath::Pretess) => 4,
                None => 0,
            };
            let dest = pretess.map_or(0, |range| {
                u64::from(range.start).wrapping_shl(32) ^ u64::from(range.count)
            });
            (
                1,
                u64::from(placement),
                u64::from(surface) ^ u64::from(lighting_handle).rotate_left(16),
                stream_tag ^ dest,
            )
        }
        RetainedDrawKind::XModel {
            surface, object_id, ..
        } => (2, u64::from(surface), u64::from(object_id), 0),
        RetainedDrawKind::CodeMesh {
            draw,
            arg_count,
            viewmodel,
            ..
        } => (
            3,
            u64::from(draw),
            u64::from(arg_count),
            u64::from(viewmodel),
        ),
        RetainedDrawKind::ParticleCloud { draw, .. } => (4, u64::from(draw), 0, 0),
        RetainedDrawKind::MarkMesh { draw, .. } => (5, u64::from(draw), 0, 0),
        RetainedDrawKind::Glass {
            draw,
            lighting_handle,
            ..
        } => (6, u64::from(draw), u64::from(lighting_handle), 0),
    };
    crate::assemble::drawsurf::list::mix_content_id(id, kind_tag);
    crate::assemble::drawsurf::list::mix_content_id(id, a);
    crate::assemble::drawsurf::list::mix_content_id(id, b);
    crate::assemble::drawsurf::list::mix_content_id(id, c);
    if let RetainedDrawKind::XModel {
        world_from_local,
        portal_slice,
        portal_aperture,
        ..
    } = item.kind
    {
        for value in world_from_local.to_cols_array() {
            crate::assemble::drawsurf::list::mix_content_id(id, u64::from(value.to_bits()));
        }
        crate::assemble::drawsurf::list::mix_content_id(id, u64::from(portal_slice.is_some()));
        crate::assemble::drawsurf::list::mix_content_id(id, u64::from(portal_aperture.is_some()));
        if let Some(aperture) = portal_aperture {
            crate::assemble::drawsurf::list::mix_content_id(id, u64::from(aperture.is_clone));
            for value in aperture.planes.into_iter().flatten() {
                crate::assemble::drawsurf::list::mix_content_id(id, value.to_bits());
            }
        }
        for value in portal_slice.into_iter().flatten() {
            crate::assemble::drawsurf::list::mix_content_id(id, value.to_bits());
        }
    }
}

pub(super) fn merge_presorted_retained(
    static_items: &[RetainedDrawItem],
    dynamic: &[RetainedDrawItem],
) -> Vec<RetainedDrawItem> {
    let mut out = Vec::with_capacity(static_items.len() + dynamic.len());
    let mut i = 0;
    let mut j = 0;
    while i < static_items.len() && j < dynamic.len() {
        let a = (
            static_items[i].host_sort_key(),
            retained_draw_order_tie(&static_items[i].kind),
        );
        let b = (
            dynamic[j].host_sort_key(),
            retained_draw_order_tie(&dynamic[j].kind),
        );
        if a <= b {
            out.push(static_items[i]);
            i += 1;
        } else {
            out.push(dynamic[j]);
            j += 1;
        }
    }
    out.extend_from_slice(&static_items[i..]);
    out.extend_from_slice(&dynamic[j..]);
    out
}

// The packed key holds only a 12-bit sort band; maps with more materials alias in it.
pub(super) fn world_surface_rank(
    cull: Option<&crate::prepare::scene::world::WorldCull>,
    surf: usize,
    packed: u64,
    catalog: &crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog,
) -> u32 {
    cull.and_then(|cull| cull.surface_materials.get(surf).copied().flatten())
        .and_then(|id| catalog.ordinal_for_asset_id(id))
        .map_or_else(
            || u32::from(GfxDrawSurf::from_packed(packed).material_sorted_index()),
            |ordinal| ordinal.get(),
        )
}
