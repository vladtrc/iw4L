use lighting_iw4::dlight_hits_aabb;
use render_frame::{RetainedDrawItem, RetainedDrawKind};
use render_scene::{GfxScene, SCENE_VIEWMODEL_ENTNUM};

use super::FrameAssemblyInputs;
use crate::prepare::scene::world::WorldScene;

pub(crate) fn combined_light_types(map_types: &[u8], inputs: &FrameAssemblyInputs) -> Vec<u8> {
    let mut types = map_types.to_vec();
    if types.len() < inputs.map_light_n {
        types.resize(inputs.map_light_n, 0);
    }
    for light in inputs.primary_lights.iter().skip(inputs.map_light_n) {
        types.push(light.light_type);
    }
    types
}

fn posed_bounds(gfx: &GfxScene, entnum: u32) -> Option<dpvs_iw4::Bounds> {
    gfx.scene_dobj(entnum)
        .and_then(|dobj| dobj.posed_bounds)
        .or_else(|| gfx.scene_model(entnum).and_then(|model| model.posed_bounds))
}

pub(crate) fn dlight_receiver_keep(
    draw: &RetainedDrawItem,
    origin: [f32; 3],
    radius: f32,
    scene: Option<&WorldScene>,
    gfx: Option<&GfxScene>,
    world_run_surfs: &[u16],
) -> bool {
    match draw.kind {
        RetainedDrawKind::World {
            surf, run, run_off, ..
        } => {
            let Some(cull) = scene.and_then(|scene| scene.cull.as_ref()) else {
                return false;
            };
            (0..run.max(1)).any(|offset| {
                let member = if run <= 1 {
                    Some(surf)
                } else {
                    world_run_surfs
                        .get(run_off as usize + usize::from(offset))
                        .copied()
                };
                member
                    .and_then(|member| cull.dpvs.surface_bounds.get(usize::from(member)))
                    .is_some_and(|bounds| {
                        dlight_hits_aabb(origin, radius, bounds.mid(), bounds.half())
                    })
            })
        }
        RetainedDrawKind::Smodel {
            placement,
            stream: Some(_),
            ..
        } => {
            let Some(cull) = scene.and_then(|scene| scene.cull.as_ref()) else {
                return false;
            };
            cull.dpvs
                .smodel_bounds
                .get(placement as usize)
                .is_some_and(|bounds| dlight_hits_aabb(origin, radius, bounds.mid(), bounds.half()))
        }
        RetainedDrawKind::XModel { scene_entnum, .. } => {
            let Some(entnum) = scene_entnum else {
                return false;
            };
            if entnum == SCENE_VIEWMODEL_ENTNUM {
                return false;
            }
            let Some(gfx) = gfx else {
                return false;
            };
            posed_bounds(gfx, entnum)
                .is_some_and(|bounds| dlight_hits_aabb(origin, radius, bounds.mid(), bounds.half()))
        }
        _ => false,
    }
}
