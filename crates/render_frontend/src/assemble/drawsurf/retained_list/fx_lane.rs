use super::common::{
    item_uses_distortion_lane, push_direct_lane_item, retained_draw_order_tie, with_catalog,
};
use crate::assemble::drawsurf::tess::fx::FxCodeMeshPlan;
use crate::assemble::drawsurf::tess::glass::GfxGlassMeshPlan;
use crate::assemble::drawsurf::tess::mark::{GfxMarkMeshPlan, mark_mesh_surface_samplers};
use crate::assemble::drawsurf::tess::particle_cloud::FxParticleCloudPlan;
use crate::prepare::scene::world::WorldScene;
use bevy::prelude::*;
use dpvs_iw4::{
    pack_code_mesh_draw_surf, pack_glass_mesh_draw_surf, pack_mark_mesh_draw_surf,
    pack_particle_cloud_draw_surf,
};
use render_frame::{RetainedDrawItem, RetainedDrawKind};

#[derive(Resource, Clone, Debug, Default)]
pub struct FxDrawLane {
    pub(crate) colour: Vec<RetainedDrawItem>,
    pub(crate) emissive: Vec<RetainedDrawItem>,
    pub(crate) distortion: Vec<RetainedDrawItem>,

    pub(crate) membership_revision: u64,
    membership_hash: u64,
    membership_context: Option<FxMembershipContext>,

    pub(crate) payload_revision: u64,
    pub(crate) code_sorted: u32,
    pub(crate) particle_cloud_sorted: u32,
    pub(crate) mark_mesh_sorted: u32,
    pub(crate) glass_mesh_sorted: u32,
    pub(crate) code_skipped_no_ordinal: u32,
    pub(crate) particle_cloud_skipped_no_ordinal: u32,
    pub(crate) mark_mesh_skipped_no_ordinal: u32,
    pub(crate) mark_mesh_skipped_no_lighting: u32,
    pub(crate) glass_mesh_skipped_no_ordinal: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FxMembershipContext {
    generation: render_material::MaterialGenerationId,
    distortion_key: Option<u32>,
}

fn glass_depth_order(
    item: &RetainedDrawItem,
    eye: [f32; 3],
    glass: Option<&GfxGlassMeshPlan>,
) -> u32 {
    let RetainedDrawKind::Glass { draw, .. } = item.kind else {
        return 0;
    };
    let Some(origin) = glass
        .and_then(|plan| plan.draws.get(draw as usize))
        .map(|d| d.origin)
    else {
        return 0;
    };
    let dx = origin[0] - eye[0];
    let dy = origin[1] - eye[1];
    let dz = origin[2] - eye[2];
    !(dx * dx + dy * dy + dz * dz).to_bits()
}

fn sort_fx_draw_lane(lane: &mut FxDrawLane, eye: [f32; 3], glass: Option<&GfxGlassMeshPlan>) {
    let order = |item: &RetainedDrawItem| {
        (
            item.host_sort_key(),
            glass_depth_order(item, eye, glass),
            retained_draw_order_tie(&item.kind),
        )
    };
    lane.colour.sort_unstable_by_key(order);
    lane.emissive.sort_unstable_by_key(order);
    lane.distortion.sort_unstable_by_key(order);
}

fn lane_layout_hash(
    fx: Option<&FxCodeMeshPlan>,
    particle_cloud: Option<&FxParticleCloudPlan>,
    mark_mesh: Option<&GfxMarkMeshPlan>,
    glass_mesh: Option<&GfxGlassMeshPlan>,
) -> u64 {
    let mut id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    if let Some(plan) = fx {
        crate::assemble::drawsurf::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            crate::assemble::drawsurf::list::mix_content_id(&mut id, i as u64);
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.material));
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.viewmodel));
            let mat = plan.materials.get(draw.material as usize);
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                u64::from(mat.map(|m| m.sort_key).unwrap_or(0)),
            );
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                mat.and_then(|m| m.material_sorted_index)
                    .map(u64::from)
                    .unwrap_or(u64::MAX),
            );
        }
    }
    if let Some(plan) = particle_cloud {
        crate::assemble::drawsurf::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            crate::assemble::drawsurf::list::mix_content_id(&mut id, i as u64);
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.material));
        }
    }
    if let Some(plan) = mark_mesh {
        crate::assemble::drawsurf::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            crate::assemble::drawsurf::list::mix_content_id(&mut id, i as u64);
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.material));
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.sub_key.lmap));
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                u64::from(draw.sub_key.entity.unwrap_or(u16::MAX)),
            );
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                u64::from(draw.sub_key.smodel.unwrap_or(u16::MAX)),
            );
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                u64::from(draw.sub_key.glass.unwrap_or(u16::MAX)),
            );
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                u64::from(draw.sub_key.primary_light),
            );
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.sub_key.probe));
        }
    }
    if let Some(plan) = glass_mesh {
        crate::assemble::drawsurf::list::mix_content_id(&mut id, plan.draws.len() as u64);
        for (i, draw) in plan.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            crate::assemble::drawsurf::list::mix_content_id(&mut id, i as u64);
            crate::assemble::drawsurf::list::mix_content_id(&mut id, u64::from(draw.material));
            crate::assemble::drawsurf::list::mix_content_id(
                &mut id,
                u64::from(draw.reflection_probe_index),
            );
        }
    }
    id
}

fn overlay_fx_lane_payload(
    fx: Option<&FxCodeMeshPlan>,
    particle_cloud: Option<&FxParticleCloudPlan>,
    mark_mesh: Option<&GfxMarkMeshPlan>,
    glass_mesh: Option<&GfxGlassMeshPlan>,
    lane: &mut FxDrawLane,
) -> bool {
    for item in lane
        .colour
        .iter_mut()
        .chain(lane.emissive.iter_mut())
        .chain(lane.distortion.iter_mut())
    {
        match &mut item.kind {
            RetainedDrawKind::CodeMesh {
                draw,
                arg_count,
                args,
                ..
            } => {
                let Some(plan) = fx else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                let n = (src.arg_count as usize).min(2);
                *arg_count = n as u8;
                *args = [[0.0; 4]; 2];
                let start = src.arg_start as usize;
                for (ai, slot) in args.iter_mut().enumerate().take(n) {
                    if let Some(row) = plan.args.get(start + ai) {
                        *slot = *row;
                    }
                }
            }
            RetainedDrawKind::ParticleCloud { draw, clouds, .. } => {
                let Some(plan) = particle_cloud else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                *clouds = src.clouds;
            }
            RetainedDrawKind::MarkMesh {
                glass,
                draw,
                packed,
                lighting_handle,
                ..
            } => {
                let Some(plan) = mark_mesh else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                *packed = src.sub_key.packed;
                *glass = src.sub_key.glass.is_some();
                if let Some(piece) = src.sub_key.glass {
                    let Some(glass_draw) = glass_mesh
                        .and_then(|plan| plan.draws.iter().find(|draw| draw.piece == piece))
                    else {
                        return false;
                    };
                    *lighting_handle = glass_draw.lighting_handle;
                }
            }
            RetainedDrawKind::Glass {
                draw,
                lighting_handle,
                ..
            } => {
                let Some(plan) = glass_mesh else {
                    return false;
                };
                let Some(src) = plan.draws.get(*draw as usize) else {
                    return false;
                };
                *lighting_handle = src.lighting_handle;
            }
            _ => {}
        }
    }
    true
}

fn mix_fx_payload_revision(
    fx: Option<&FxCodeMeshPlan>,
    particle_cloud: Option<&FxParticleCloudPlan>,
    mark_mesh: Option<&GfxMarkMeshPlan>,
    glass_mesh: Option<&GfxGlassMeshPlan>,
) -> u64 {
    let mut id = crate::assemble::drawsurf::list::CONTENT_ID_SEED;
    crate::assemble::drawsurf::list::mix_content_id(
        &mut id,
        fx.map(|plan| plan.revision).unwrap_or(0),
    );
    crate::assemble::drawsurf::list::mix_content_id(
        &mut id,
        particle_cloud.map(|plan| plan.revision).unwrap_or(0),
    );
    crate::assemble::drawsurf::list::mix_content_id(
        &mut id,
        mark_mesh.map(|plan| plan.revision).unwrap_or(0),
    );
    crate::assemble::drawsurf::list::mix_content_id(
        &mut id,
        glass_mesh.map(|plan| plan.revision).unwrap_or(0),
    );
    id
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct MarkLightingSource<'w, 's> {
    mark_owners: Query<
        'w,
        's,
        (
            Entity,
            &'static crate::prepare::scene::world::WorldScriptModelInstance,
        ),
    >,
    lighting: Res<'w, crate::prepare::scene::model_lighting_cache::ResolvedModelLightingTable>,
    smodel_lighting: Option<Res<'w, crate::prepare::scene::smodel_lighting::WorldSmodelLighting>>,
}

impl MarkLightingSource<'_, '_> {
    fn resolve(
        &self,
        sub_key: &mut crate::assemble::drawsurf::tess::mark::GfxMarkSubKey,
    ) -> Option<u32> {
        let handle = if let Some(number) = sub_key.entity {
            use crate::prepare::scene::model_lighting_cache::{
                ModelLightingOwner, ResolvedModelLighting,
            };
            let result = self
                .mark_owners
                .iter()
                .find(|(_, owner)| owner.gentity_number == Some(number))
                .and_then(|(entity, _)| self.lighting.get(ModelLightingOwner::ScriptModel(entity)));
            let Some(ResolvedModelLighting::Seated {
                handle,
                scene_light_index,
                reflection_probe_index,
                ..
            }) = result
            else {
                return None;
            };
            sub_key.primary_light = scene_light_index;
            sub_key.probe = reflection_probe_index;
            handle
        } else if let Some(piece) = sub_key.glass {
            use crate::prepare::scene::model_lighting_cache::{
                ModelLightingOwner, ResolvedModelLighting,
            };
            let Some(ResolvedModelLighting::Seated {
                handle,
                scene_light_index,
                reflection_probe_index,
                ..
            }) = self.lighting.get(ModelLightingOwner::Glass(piece))
            else {
                return None;
            };
            sub_key.primary_light = scene_light_index;
            sub_key.probe = reflection_probe_index;
            handle
        } else if let Some(index) = sub_key.smodel {
            let handle = self
                .smodel_lighting
                .as_ref()
                .and_then(|l| l.handles.get(index as usize))
                .copied()?;
            u32::from(handle)
        } else {
            0
        };
        Some(handle)
    }
}

pub(crate) fn rebuild_fx_draw_lane(
    plans: (
        Option<Res<FxCodeMeshPlan>>,
        Option<Res<FxParticleCloudPlan>>,
        Option<Res<GfxMarkMeshPlan>>,
        Option<Res<GfxGlassMeshPlan>>,
    ),
    runtime: Res<crate::assemble::drawsurf::MaterialGeneration>,
    mut lane: ResMut<FxDrawLane>,
    mark_lighting: MarkLightingSource,
    camera_origin: Option<Res<render_fx::FxCameraOrigin>>,
    scene: Option<Res<WorldScene>>,
) {
    let (fx, particle_cloud, mark_mesh, glass_mesh) = plans;
    let eye = camera_origin.map(|c| c.0).unwrap_or([0.0; 3]);
    let glass_plan = glass_mesh.as_deref();
    let distortion_key = scene
        .as_deref()
        .and_then(|scene| scene.cull.as_ref())
        .and_then(|cull| cull.sort_key_distortion);
    let lane = &mut *lane;
    let layout = lane_layout_hash(
        fx.as_deref(),
        particle_cloud.as_deref(),
        mark_mesh.as_deref(),
        glass_mesh.as_deref(),
    );
    let payload = mix_fx_payload_revision(
        fx.as_deref(),
        particle_cloud.as_deref(),
        mark_mesh.as_deref(),
        glass_mesh.as_deref(),
    );
    let context = FxMembershipContext {
        generation: runtime.catalog.generation_id(),
        distortion_key,
    };
    if lane.can_overlay(layout, context)
        && overlay_fx_lane_payload(
            fx.as_deref(),
            particle_cloud.as_deref(),
            mark_mesh.as_deref(),
            glass_mesh.as_deref(),
            lane,
        )
    {
        sort_fx_draw_lane(lane, eye, glass_plan);
        lane.payload_revision = payload;
        perf::counter!("fx_layout_overlay", "iw4l.render", 1.0);
        return;
    }
    perf::counter!("fx_layout_overlay", "iw4l.render", 0.0);
    lane.colour.clear();
    lane.emissive.clear();
    lane.distortion.clear();
    lane.code_sorted = 0;
    lane.particle_cloud_sorted = 0;
    lane.mark_mesh_sorted = 0;
    lane.glass_mesh_sorted = 0;
    lane.code_skipped_no_ordinal = 0;
    lane.particle_cloud_skipped_no_ordinal = 0;
    lane.mark_mesh_skipped_no_ordinal = 0;
    lane.mark_mesh_skipped_no_lighting = 0;
    lane.glass_mesh_skipped_no_ordinal = 0;

    if let Some(fx) = fx.as_ref() {
        for (i, draw) in fx.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = fx.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.code_skipped_no_ordinal = lane.code_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let key = pack_code_mesh_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
            )
            .packed;
            let mut mesh_args = [[0.0f32; 4]; 2];
            let n = (draw.arg_count as usize).min(2);
            let start = draw.arg_start as usize;
            for (ai, slot) in mesh_args.iter_mut().enumerate().take(n) {
                if let Some(row) = fx.args.get(start + ai) {
                    *slot = *row;
                }
            }
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::CodeMesh {
                    viewmodel: draw.viewmodel,
                    draw: i as u32,
                    material: draw.material,
                    arg_count: n as u8,
                    args: mesh_args,
                },
                crate::assemble::drawsurf::SurfaceSamplerInputs::default(),
                &runtime.catalog,
            );
            push_direct_lane_item(
                &mut lane.colour,
                &mut lane.emissive,
                Some(&mut lane.distortion),
                item,
            );
            lane.code_sorted = lane.code_sorted.saturating_add(1);
        }
    }

    if let Some(clouds) = particle_cloud.as_ref() {
        for (i, draw) in clouds.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = clouds.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.particle_cloud_skipped_no_ordinal =
                    lane.particle_cloud_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let key = pack_particle_cloud_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
            )
            .packed;
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::ParticleCloud {
                    draw: i as u32,
                    material: draw.material,
                    clouds: draw.clouds,
                },
                crate::assemble::drawsurf::SurfaceSamplerInputs::default(),
                &runtime.catalog,
            );
            push_direct_lane_item(&mut lane.colour, &mut lane.emissive, None, item);
            lane.particle_cloud_sorted = lane.particle_cloud_sorted.saturating_add(1);
        }
    }

    if let Some(marks) = mark_mesh.as_ref() {
        for (i, draw) in marks.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = marks.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.mark_mesh_skipped_no_ordinal =
                    lane.mark_mesh_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let mut sub_key = draw.sub_key;
            let Some(lighting_handle) = mark_lighting.resolve(&mut sub_key) else {
                lane.mark_mesh_skipped_no_lighting += 1;
                continue;
            };
            let key = pack_mark_mesh_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
                sub_key.lmap,
                sub_key.primary_light,
                sub_key.probe,
            )
            .packed;
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::MarkMesh {
                    glass: sub_key.glass.is_some(),
                    draw: i as u32,
                    material: draw.material,
                    packed: sub_key.packed,
                    lighting_handle,
                },
                mark_mesh_surface_samplers(sub_key),
                &runtime.catalog,
            );
            push_direct_lane_item(&mut lane.colour, &mut lane.emissive, None, item);
            lane.mark_mesh_sorted = lane.mark_mesh_sorted.saturating_add(1);
        }
    }

    if let Some(glass) = glass_mesh.as_ref() {
        for (i, draw) in glass.draws.iter().enumerate() {
            if draw.index_count == 0 {
                continue;
            }
            let Some(mat) = glass.materials.get(draw.material as usize) else {
                continue;
            };
            let Some(material_sorted_index) = mat.material_sorted_index else {
                lane.glass_mesh_skipped_no_ordinal =
                    lane.glass_mesh_skipped_no_ordinal.saturating_add(1);
                continue;
            };
            let key = pack_glass_mesh_draw_surf(
                mat.sort_key,
                render_material::sort_band(material_sorted_index),
                i as u16,
                draw.reflection_probe_index,
            )
            .packed;
            let packed_key = dpvs_iw4::GfxDrawSurf::from_packed(key);
            let item = with_catalog(
                key,
                material_sorted_index,
                RetainedDrawKind::Glass {
                    draw: i as u32,
                    material: draw.material,
                    lighting_handle: draw.lighting_handle,
                },
                crate::assemble::drawsurf::SurfaceSamplerInputs {
                    reflection_probe: Some(crate::assemble::drawsurf::SurfaceReflectionProbeId(
                        packed_key.reflection_probe_index(),
                    )),
                    ..Default::default()
                },
                &runtime.catalog,
            );
            let distortion = if item_uses_distortion_lane(mat.sort_key, distortion_key) {
                Some(&mut lane.distortion)
            } else {
                None
            };
            push_direct_lane_item(&mut lane.colour, &mut lane.emissive, distortion, item);
            lane.glass_mesh_sorted = lane.glass_mesh_sorted.saturating_add(1);
        }
    }

    sort_fx_draw_lane(lane, eye, glass_plan);
    lane.membership_hash = layout;
    lane.membership_context = Some(context);
    lane.membership_revision = lane.membership_revision.wrapping_add(1);
    lane.payload_revision = payload;
}

impl FxDrawLane {
    fn can_overlay(&self, layout: u64, context: FxMembershipContext) -> bool {
        self.membership_hash == layout && self.membership_context == Some(context)
    }
}
