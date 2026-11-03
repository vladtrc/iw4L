use super::common::{retained_draw_order_tie, with_catalog};
use crate::assemble::drawsurf::tess::xmodel::{
    XMODEL_OBJECT_ID_VIEWMODEL, XModelDrawPlan, XModelProducers,
};
use crate::prepare::scene::view_parms::PreparedSceneView;
use bevy::prelude::*;
use dpvs_iw4::{
    GfxDrawSurf, SF_XMODEL_RIGID_SKINNED, pack_xmodel_rigid_skinned_draw_surf,
    with_reflection_probe_index,
};
use render_frame::{RetainedDrawItem, RetainedDrawKind};

#[derive(Resource, Clone, Debug, Default)]
pub struct XModelDrawLane {
    pub(crate) colour: Vec<RetainedDrawItem>,
    pub(crate) emissive: Vec<RetainedDrawItem>,
    pub(crate) distortion: Vec<RetainedDrawItem>,

    colour_source: Vec<u32>,
    emissive_source: Vec<u32>,
    distortion_source: Vec<u32>,

    pub(crate) membership_revision: u64,
    membership_hash: u64,

    pub(crate) payload_revision: u64,
    pub(crate) merge_packed_n: Option<u32>,
    pub(crate) sorted: u32,
    pub(crate) object_id_standin: u32,
    pub(crate) fx_object_id_exhausted: u32,
    pub(crate) skipped_no_ordinal: u32,
    pub(crate) skipped_no_baked_key: u32,
    pub(crate) skipped_camera_frustum: u32,
    pub(crate) skipped_no_lighting: u32,
}

fn xmodel_camera_material(
    xmodel: &XModelDrawPlan,
    draw: &crate::assemble::drawsurf::tess::xmodel::XModelSurfaceDraw,
    catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
    thermal: &crate::assemble::drawsurf::thermal_body::ThermalBodySelection,
) -> Option<(u32, bool)> {
    if let Some(ordinal) = thermal.for_draw(draw) {
        let ordinal = *ordinal.as_ref().ok()?;
        let material = catalog.material_for_sorted_ordinal(ordinal)?;
        Some((ordinal, material.takes_model_lighting))
    } else {
        let material = xmodel.materials.get(draw.material as usize)?;
        Some((
            material.material_sorted_index?,
            material.model_lighting_required,
        ))
    }
}

fn xmodel_lane_layout_hash(
    xmodel: &XModelDrawPlan,
    catalog: &crate::assemble::drawsurf::material_runtime::RuntimeMaterialCatalog,
    thermal: &crate::assemble::drawsurf::thermal_body::ThermalBodySelection,
) -> u64 {
    let mut id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_id(&mut id, catalog.generation_id().get());
    crate::assemble::drawsurf::list::mix_content_id(&mut id, xmodel.topology_revision);
    crate::assemble::drawsurf::list::mix_content_id(&mut id, xmodel.draws.len() as u64);
    for draw in &xmodel.draws {
        crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.surface));
        crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.material));
        crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.object_id));
        crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.scene_light_index));
        crate::assemble::drawsurf::list::mix_content_id(
            &mut id,
            u64::from(draw.reflection_probe_index),
        );
        let refusal = match draw.colour_refusal {
            Some(crate::assemble::drawsurf::tess::xmodel::XModelColourRefusal::CameraFrustum) => 1,
            None => 0,
        };
        crate::assemble::drawsurf::list::mix_content_id(&mut id, refusal);
        let material = xmodel_camera_material(xmodel, draw, catalog, thermal);
        let lighting_skip =
            material.is_some_and(|(_, required)| required && draw.lighting_handle == 0);
        crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(lighting_skip));
        let ordinal = material
            .map(|(ordinal, _)| u64::from(ordinal))
            .unwrap_or(u64::MAX);
        crate::assemble::drawsurf::list::mix_content_id(&mut id, ordinal);
    }
    id
}

fn overlay_xmodel_lane_payload(xmodel: &XModelDrawPlan, lane: &mut XModelDrawLane) -> bool {
    let XModelDrawLane {
        colour,
        emissive,
        distortion,
        colour_source,
        emissive_source,
        distortion_source,
        ..
    } = lane;
    if colour.len() != colour_source.len()
        || emissive.len() != emissive_source.len()
        || distortion.len() != distortion_source.len()
    {
        return false;
    }
    let items = colour
        .iter_mut()
        .chain(emissive.iter_mut())
        .chain(distortion.iter_mut());
    let sources = colour_source
        .iter()
        .chain(emissive_source.iter())
        .chain(distortion_source.iter());
    for (item, &source) in items.zip(sources) {
        let RetainedDrawKind::XModel {
            world_from_local,
            lighting_handle,
            packed_lighting,
            is_scope,
            scene_entnum,
            ..
        } = &mut item.kind
        else {
            continue;
        };
        let Some(draw) = xmodel.draws.get(source as usize) else {
            return false;
        };
        *world_from_local = draw.world_from_local;
        *lighting_handle = draw.lighting_handle;
        *packed_lighting = draw.packed_lighting;
        *is_scope = draw.is_scope;
        *scene_entnum = draw.scene_entnum;
    }
    true
}

fn sort_xmodel_lane(items: &mut [RetainedDrawItem], sources: &mut [u32], order: &mut Vec<u32>) {
    debug_assert_eq!(items.len(), sources.len());
    order.clear();
    order.extend(0..u32::try_from(items.len()).unwrap_or(u32::MAX));
    order.sort_unstable_by_key(|&i| {
        let item = &items[i as usize];
        (item.host_sort_key(), retained_draw_order_tie(&item.kind))
    });
    for start in 0..order.len() {
        let mut at = start;
        loop {
            let from = order[at] as usize;
            order[at] = u32::try_from(at).unwrap_or(u32::MAX);
            if from == start {
                break;
            }
            items.swap(at, from);
            sources.swap(at, from);
            at = from;
        }
    }
}

pub(crate) fn rebuild_xmodel_draw_lane(
    plans: (
        Option<Res<crate::assemble::drawsurf::tess::xmodel::FpvDrawPlan>>,
        Option<Res<crate::assemble::drawsurf::tess::xmodel::RemoteBodyDrawPlan>>,
        Option<Res<crate::assemble::drawsurf::tess::xmodel::ScriptModelDrawPlan>>,
        Option<Res<crate::assemble::drawsurf::tess::xmodel::MissileDrawPlan>>,
        Option<Res<crate::assemble::drawsurf::tess::xmodel::ItemDrawPlan>>,
        Option<Res<crate::assemble::drawsurf::tess::xmodel::FxModelDrawPlan>>,
        Option<Res<crate::assemble::drawsurf::tess::xmodel::DynEntDrawPlan>>,
        Option<Res<crate::prepare::scene::gfx_scene::HostGfxScene>>,
    ),
    mut xmodel: ResMut<XModelDrawPlan>,
    sky: Option<Res<crate::assemble::drawsurf::tess::sky::SkyModelDrawPlan>>,
    world: Option<Res<crate::prepare::scene::world::WorldScene>>,
    prepared: Option<Res<PreparedSceneView>>,
    runtime: Res<crate::assemble::drawsurf::MaterialGeneration>,
    mut lane: ResMut<XModelDrawLane>,
    presentation: (Res<net::PresentedSnapshot>, Res<frame::ScreenEffectsView>),
) {
    let (fpv, bodies, scripts, missiles, items, fx_models, dynents, gfx_scene) = plans;
    let empty_fpv = crate::assemble::drawsurf::tess::xmodel::FpvDrawPlan::default();
    let empty_body = crate::assemble::drawsurf::tess::xmodel::RemoteBodyDrawPlan::default();
    let empty_scripts = crate::assemble::drawsurf::tess::xmodel::ScriptModelDrawPlan::default();
    let empty_missiles = crate::assemble::drawsurf::tess::xmodel::MissileDrawPlan::default();
    let empty_items = crate::assemble::drawsurf::tess::xmodel::ItemDrawPlan::default();
    let empty_fx_models = crate::assemble::drawsurf::tess::xmodel::FxModelDrawPlan::default();
    let empty_dynents = crate::assemble::drawsurf::tess::xmodel::DynEntDrawPlan::default();
    XModelProducers {
        fpv: fpv.as_deref().unwrap_or(&empty_fpv),
        bodies: bodies.as_deref().unwrap_or(&empty_body),
        scripts: scripts.as_deref().unwrap_or(&empty_scripts),
        missiles: missiles.as_deref().unwrap_or(&empty_missiles),
        items: items.as_deref().unwrap_or(&empty_items),
        fx_models: fx_models.as_deref().unwrap_or(&empty_fx_models),
        dynents: dynents.as_deref().unwrap_or(&empty_dynents),
    }
    .merge_into(
        &mut xmodel,
        gfx_scene.as_ref().map(|g| &g.scene),
        sky.as_deref()
            .zip(prepared.as_ref().filter(|v| v.ready).map(|v| v.eye))
            .map(|(sky, eye)| {
                (
                    sky,
                    eye,
                    world
                        .as_deref()
                        .and_then(|world| world.portal_view.as_ref()),
                )
            }),
    );

    let (presented, effects) = presentation;
    let thermal = crate::assemble::drawsurf::thermal_body::ThermalBodySelection::new(
        &presented,
        &effects,
        &runtime.catalog,
    );
    if let Some(name) = xmodel
        .draws
        .iter()
        .find_map(|draw| match thermal.for_draw(draw) {
            Some(Err(name)) => Some(name),
            _ => None,
        })
    {
        diag::warn_once!(
            World,
            "thermal body material missing: `{name}` not in global catalog",
            name = name.clone()
        );
    }
    let layout = xmodel_lane_layout_hash(&xmodel, &runtime.catalog, &thermal);
    lane.merge_packed_n = xmodel.packed_rows().map(|rows| rows.len() as u32);
    lane.fx_object_id_exhausted = xmodel.fx_object_id_exhausted;
    if layout == lane.membership_hash && overlay_xmodel_lane_payload(&xmodel, &mut lane) {
        lane.payload_revision = xmodel.revision;
        perf::counter!("xmodel_layout_overlay", "iw4l.render", 1.0);
        return;
    }
    perf::counter!("xmodel_layout_overlay", "iw4l.render", 0.0);

    lane.colour.clear();
    lane.emissive.clear();
    lane.distortion.clear();
    lane.colour_source.clear();
    lane.emissive_source.clear();
    lane.distortion_source.clear();
    lane.merge_packed_n = xmodel.packed_rows().map(|rows| rows.len() as u32);
    lane.sorted = 0;
    lane.object_id_standin = 0;
    lane.fx_object_id_exhausted = xmodel.fx_object_id_exhausted;
    lane.skipped_no_ordinal = 0;
    lane.skipped_no_baked_key = 0;
    lane.skipped_camera_frustum = 0;
    lane.skipped_no_lighting = 0;

    for (source, draw) in xmodel.draws.iter().enumerate() {
        let source = u32::try_from(source).unwrap_or(u32::MAX);
        if matches!(
            draw.colour_refusal,
            Some(crate::assemble::drawsurf::tess::xmodel::XModelColourRefusal::CameraFrustum)
        ) {
            lane.skipped_camera_frustum = lane.skipped_camera_frustum.saturating_add(1);
            continue;
        }
        let Some((material_sorted_index, model_lighting_required)) =
            xmodel_camera_material(&xmodel, draw, &runtime.catalog, &thermal)
        else {
            lane.skipped_no_ordinal = lane.skipped_no_ordinal.saturating_add(1);
            continue;
        };
        if model_lighting_required && draw.lighting_handle == 0 {
            lane.skipped_no_lighting = lane.skipped_no_lighting.saturating_add(1);
            continue;
        }
        let Some(baked) = runtime
            .catalog
            .material_for_sorted_ordinal(material_sorted_index)
            .and_then(|material| material.baked_draw_surf)
        else {
            lane.skipped_no_baked_key = lane.skipped_no_baked_key.saturating_add(1);
            continue;
        };
        let key = with_reflection_probe_index(
            pack_xmodel_rigid_skinned_draw_surf(GfxDrawSurf::from_packed(baked), draw.object_id),
            draw.reflection_probe_index,
        )
        .packed;
        let key = crate::assemble::drawsurf::with_scene_light_index(key, draw.scene_light_index);
        let packed_key = dpvs_iw4::GfxDrawSurf::from_packed(key);
        debug_assert_eq!(packed_key.surf_type(), SF_XMODEL_RIGID_SKINNED);
        let item = with_catalog(
            key,
            material_sorted_index,
            RetainedDrawKind::XModel {
                portal_slice: None,
                portal_aperture: None,
                surface: draw.surface,
                material: draw.material,
                object_id: draw.object_id,
                world_from_local: draw.world_from_local,
                lighting_handle: draw.lighting_handle,
                packed_lighting: draw.packed_lighting,
                is_scope: draw.is_scope,
                scene_entnum: draw.scene_entnum,
            },
            crate::assemble::drawsurf::SurfaceSamplerInputs {
                reflection_probe: Some(crate::assemble::drawsurf::SurfaceReflectionProbeId(
                    packed_key.reflection_probe_index(),
                )),
                ..Default::default()
            },
            &runtime.catalog,
        );
        lane.distortion.push(item);
        lane.distortion_source.push(source);
        match crate::assemble::drawsurf::frame_product_kind_for_camera_region(item.camera_region) {
            Some(crate::assemble::drawsurf::FrameProductKind::Emissive) => {
                lane.emissive.push(item);
                lane.emissive_source.push(source);
            }
            Some(crate::assemble::drawsurf::FrameProductKind::Colour) => {
                lane.colour.push(item);
                lane.colour_source.push(source);
            }
            Some(_) | None => {}
        }
        lane.sorted = lane.sorted.saturating_add(1);
        if draw.object_id >= XMODEL_OBJECT_ID_VIEWMODEL {
            lane.object_id_standin = lane.object_id_standin.saturating_add(1);
        }
    }
    let mut order = Vec::new();
    {
        let XModelDrawLane {
            colour,
            emissive,
            distortion,
            colour_source,
            emissive_source,
            distortion_source,
            ..
        } = &mut *lane;
        sort_xmodel_lane(colour, colour_source, &mut order);
        sort_xmodel_lane(emissive, emissive_source, &mut order);
        sort_xmodel_lane(distortion, distortion_source, &mut order);
    }
    lane.membership_hash = layout;
    lane.membership_revision = lane.membership_revision.wrapping_add(1);
    lane.payload_revision = xmodel.revision;
    perf::counter!(
        "xmodel_colour_camera_frustum",
        "iw4l.render",
        f64::from(lane.skipped_camera_frustum)
    );
    perf::counter!(
        "xmodel_colour_no_lighting",
        "iw4l.render",
        f64::from(lane.skipped_no_lighting)
    );
}
