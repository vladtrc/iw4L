use std::collections::HashMap;

use bevy::prelude::*;

use crate::anim::scene_submission::{AnimDObjSceneSkels, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::xmodel_pose::PosedModelSurface;
use crate::occupancy::script_model::pose_script_dobj_with_materials;
use crate::{
    MissileDrawPlan, MissileOwnerDraw, XMODEL_OBJECT_ID_MISSILE_BASE, append_missile_surfaces,
    authored_lit_xmodel_pass_material, dobj_lighting_box_half, stamp_plan_geometry,
    topology_fingerprint,
};
use anim_iw4::DOBJ_RADIUS_PARENT_ROOT;
use render_scene::{
    HostGfxScene, ModelLightingOwner, ModelLightingRequest, ModelLightingRequests,
    SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas, WorldPresentFacts,
    XModelSurfaceDraw, scene_quat_from_angles,
};

pub const MISSILE_LIGHTING_Z_OFS: f32 = 4.0;

#[derive(Resource, Default)]
pub struct MissileOccupancy {
    pub rows: Vec<OccupiedMissile>,
}

pub struct OccupiedMissile {
    pub index: usize,
    pub id: Option<u32>,
    pub weapon: u32,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub name: String,
    pub lighting_origin: [f32; 3],
    pub lighting_owner: ModelLightingOwner,
}

#[derive(Resource, Default)]
struct MissilePoseProduct {
    rows: Vec<MissilePosed>,
}

struct MissilePosed {
    index: usize,
    id: Option<u32>,
    origin: [f32; 3],
    angles: [f32; 3],
    name: String,
    lighting_origin: [f32; 3],
    lighting_owner: ModelLightingOwner,
    surfaces: Vec<PosedModelSurface>,
}

struct MissileSceneSlot<'a> {
    hidden: bool,
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn missile_scene_slot(scene: &render_scene::GfxScene, entnum: u32) -> MissileSceneSlot<'_> {
    MissileSceneSlot {
        hidden: scene.scene_ent_skips_draw(entnum),
        skin_entries: scene
            .scene_ent_skinned_surfs(entnum)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

#[derive(Resource, Default)]
pub struct MissileBoltState {
    pub rows: HashMap<u32, MissileBoltRow>,

    pub predicted_rows_skipped: u64,

    pub pose_gaps: u64,

    pub play_gaps: u64,

    pub ignition_gaps: u64,
}

pub struct MissileBoltRow {
    pub weapon: u32,
    pub trail_played: bool,
    pub beacon_played: bool,
    pub ignition_played: bool,
}

pub fn register_missile_systems(app: &mut App) {
    app.init_resource::<MissileDrawPlan>()
        .init_resource::<MissileOccupancy>()
        .init_resource::<MissilePoseProduct>()
        .init_resource::<MissileBoltState>()
        .add_systems(
            Update,
            occupy_missile_scene_ents
                .in_set(frame::RenderSet::Anim)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            publish_missile_dobj_poses
                .after(occupy_missile_scene_ents)
                .after(frame::WorkerCmdSet::SkinModel)
                .in_set(frame::ClientSet::Present),
        )
        .add_systems(
            Update,
            (pose_missiles, append_missile_draws)
                .chain()
                .after(frame::WorkerCmdSet::CellSceneEnt)
                .after(frame::WorkerCmdSet::DpvsEnt)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(frame::WorkerCmdSet::SkinModel),
        );
}

pub fn missile_lighting_origin(origin: [f32; 3]) -> [f32; 3] {
    [origin[0], origin[1], origin[2] + MISSILE_LIGHTING_Z_OFS]
}

pub fn missile_pose_catalog(
    assets: Option<&assets::PreparedProjectileMeshes>,
) -> Option<&assets::ProjectileMeshCatalog> {
    assets
        .map(|prepared| &prepared.0)
        .filter(|catalog| !catalog.is_empty())
}

pub fn missile_lighting_atlas<'a>(
    atlas: Option<&'a WorldModelLightingAtlas>,
    scene_atlas: Option<&'a WorldModelLightingAtlas>,
) -> Option<&'a WorldModelLightingAtlas> {
    atlas.or(scene_atlas)
}

fn occupy_missile_scene_ents(
    presented: Option<Res<net::PresentedSnapshot>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    mut occupancy: ResMut<MissileOccupancy>,
    mut scene_skels: ResMut<AnimDObjSceneSkels>,
    mut scene_submissions: MessageWriter<AnimDObjSceneSubmission>,
    cg_clock: Option<Res<net::CgFrameClock>>,
) {
    occupancy.rows.clear();
    let Some(snapshot) = presented.as_ref() else {
        return;
    };
    let rows = snapshot.presented_projectiles();
    let at_time = cg_clock
        .as_ref()
        .filter(|clock| clock.started())
        .map(|clock| clock.time())
        .unwrap_or_else(|| snapshot.tick().map(sim::level_time_ms).unwrap_or(0));
    let weapons_reg = weapons.as_ref().map(|w| &w.0).filter(|reg| !reg.is_empty());
    let catalog = missile_pose_catalog(projectile_meshes.as_deref());
    let Some(catalog) = catalog else {
        return;
    };
    for (index, row) in rows.iter().enumerate() {
        if entity_iw4::cg_missile_nodraw(0, row.launch_time(), at_time).is_some() {
            continue;
        }
        let model = weapons_reg.and_then(|reg| reg.projectile_model_of(row.weapon()));
        let Some(name) = model else {
            continue;
        };
        let Some(entry) = catalog.get(name) else {
            continue;
        };
        let Some(_pose) = entry.skel.pose.as_ref() else {
            continue;
        };
        let origin = row.origin_at(at_time);
        let lighting_origin = missile_lighting_origin(origin);
        let angles = entity_iw4::bg_evaluate_trajectory(&row.apos(), at_time);
        let entnum = row.authoritative_id().map(|id| id.0).unwrap_or(0);
        scene_submissions.write(AnimDObjSceneSubmission {
            render_fx_flags: 0,
            has_tree: false,
            origin,
            lighting_origin,
            radius: entry.skel.radius,
            entnum,
            quat: Some(scene_quat_from_angles(angles)),
            occupy_model_n: 1,
            models: vec![scene_skels.model(name, &entry.skel, 0)],
            hide_part_bits: [0; 6],
            store_skin: true,
        });
        let lighting_owner = match row.authoritative_id() {
            Some(id) => ModelLightingOwner::Missile(id.0),
            None => ModelLightingOwner::PredictedMissile {
                owner: row.owner().0,
                weapon: row.weapon(),
            },
        };
        occupancy.rows.push(OccupiedMissile {
            index,
            id: row.authoritative_id().map(|id| id.0),
            weapon: row.weapon(),
            origin,
            angles,
            name: name.to_owned(),
            lighting_origin,
            lighting_owner,
        });
    }
}

fn publish_missile_dobj_poses(
    occupancy: Res<MissileOccupancy>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    mut dobj_poses: ResMut<crate::anim::dobj_pose::HostDObjPoseFrame>,
) {
    let catalog = missile_pose_catalog(projectile_meshes.as_deref());
    let Some(catalog) = catalog else {
        return;
    };
    for row in &occupancy.rows {
        let Some(id) = row.id else {
            continue;
        };
        let Some(entry) = catalog.get(&row.name) else {
            continue;
        };
        let Some(pose) = entry.skel.pose.as_ref() else {
            continue;
        };
        let Ok(dobj) = assets::DObj::build(&[(pose, None)]) else {
            continue;
        };
        let entity_world = missile_world_from_local(row.origin, row.angles);
        let Ok(local) = assets::dobj::pose_dobj_with_controller(
            &dobj,
            &assets::dobj::DObjPoseRequest::bind_pose(),
            Mat4::IDENTITY,
            |_, _, _| {},
        ) else {
            continue;
        };

        let _ = dobj_poses.publish(id, true, 0, entity_world, &local);
    }
}

fn pose_missiles(
    occupancy: Res<MissileOccupancy>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    gfx: Res<HostGfxScene>,
    mut product: ResMut<MissilePoseProduct>,
) {
    product.rows.clear();
    let catalog = missile_pose_catalog(projectile_meshes.as_deref());
    let Some(catalog) = catalog else {
        return;
    };
    for row in &occupancy.rows {
        let entnum = row.id.unwrap_or(0);
        let slot = missile_scene_slot(&gfx.scene, entnum);
        if slot.hidden {
            continue;
        }
        let Some(entry) = catalog.get(&row.name) else {
            continue;
        };
        let Some(pose) = entry.skel.pose.as_ref() else {
            continue;
        };
        let Some(dobj) = assets::DObj::build(&[(pose, None)]).ok() else {
            continue;
        };
        let dobj_state = assets::dobj::DObjSemanticState::bind_pose(row.name.clone(), 1, 1);
        let Ok(request) = dobj_state.resolve_request(|_| None) else {
            continue;
        };
        let Some((surfaces, _)) = pose_script_dobj_with_materials(
            None,
            &[&entry.skel],
            &dobj,
            &request,
            None,
            slot.skin_entries,
        ) else {
            continue;
        };
        product.rows.push(MissilePosed {
            index: row.index,
            id: row.id,
            origin: row.origin,
            angles: row.angles,
            name: row.name.clone(),
            lighting_origin: row.lighting_origin,
            lighting_owner: row.lighting_owner,
            surfaces,
        });
    }
}

fn append_missile_draws(
    product: Res<MissilePoseProduct>,
    projectile_meshes: Option<Res<assets::PreparedProjectileMeshes>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    tess: Option<Res<TessMaterials>>,
    facts: Res<WorldPresentFacts>,
    mut plan: ResMut<MissileDrawPlan>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
) {
    *plan = MissileDrawPlan::default();
    let catalog = missile_pose_catalog(projectile_meshes.as_deref());
    let atlas_ref = missile_lighting_atlas(atlas.as_deref(), None);
    if catalog.is_none() || atlas_ref.is_none() || !facts.spawned || tess.is_none() {
        return;
    }
    let (Some(catalog), Some(atlas), Some(tess)) = (catalog, atlas_ref, tess.as_deref()) else {
        unreachable!("required missile draw resources checked above");
    };
    let mut material_cache: HashMap<assets::MaterialIndex, SmodelPassMaterial> = HashMap::new();
    for row in &product.rows {
        let entnum = row.id.unwrap_or(0);
        let Some(entry) = catalog.get(&row.name) else {
            continue;
        };
        let materials: Vec<Option<SmodelPassMaterial>> = row
            .surfaces
            .iter()
            .map(|surface| {
                let authored = entry.material_index(surface.surface_index)?;
                if let Some(material) = material_cache.get(&authored) {
                    return Some(material.clone());
                }
                let material = authored_lit_xmodel_pass_material(
                    atlas,
                    &tess.catalog,
                    tess.material_images.as_ref(),
                    authored,
                )?;
                material_cache.insert(authored, material.clone());
                Some(material)
            })
            .collect();
        if materials.iter().all(Option::is_none) {
            continue;
        }
        let box_half = entry
            .skel
            .radius
            .and_then(|radius| dobj_lighting_box_half(&[radius], &[DOBJ_RADIUS_PARENT_ROOT]));
        let lookup_fallback = atpoint.fallback(row.lighting_origin, box_half);
        let pending_lighting = Some(lighting_requests.request(ModelLightingRequest {
            owner: row.lighting_owner,
            origin: row.lighting_origin,
            lookup_fallback,
        }));
        let surfaces_idx = append_missile_surfaces(&mut plan, &row.surfaces, &materials);
        if surfaces_idx.is_empty() {
            continue;
        }
        let object_id = XMODEL_OBJECT_ID_MISSILE_BASE.saturating_add(row.index as u16);
        plan.owners.push(MissileOwnerDraw {
            object_id,
            model: row.name.clone(),
        });
        let world_from_local = missile_world_from_local(row.origin, row.angles);
        for (surface, material) in surfaces_idx {
            plan.draws.push(XModelSurfaceDraw {
                surface,
                material,
                world_from_local,
                lighting_handle: 0,
                pending_lighting,
                colour_refusal: None,
                object_id,
                scene_light_index: 0,
                reflection_probe_index: 0,
                packed_lighting: None,
                is_scope: false,
                scene_entnum: Some(entnum),
            });
        }
    }
    if !plan.draws.is_empty() {
        let topology =
            topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.vertices.len());
        let rev = plan.revision;
        plan.revision = stamp_plan_geometry(&mut plan.revisions, rev, topology);
    }
}

pub(crate) fn missile_world_from_local(origin: [f32; 3], angles: [f32; 3]) -> Mat4 {
    let [pitch, yaw, roll] = angles;
    Transform {
        translation: Vec3::from_array(origin),
        rotation: Quat::from_euler(
            EulerRot::ZYX,
            yaw.to_radians(),
            pitch.to_radians(),
            roll.to_radians(),
        ),
        scale: Vec3::ONE,
    }
    .to_matrix()
}
