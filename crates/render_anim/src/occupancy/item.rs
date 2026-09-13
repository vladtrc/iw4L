use std::collections::HashMap;

use bevy::prelude::*;

use crate::anim::scene_submission::{AnimDObjSceneSkels, AnimDObjSceneSubmission, AnimSceneSubmit};
use crate::anim::xmodel_pose::PosedModelSurface;
use crate::occupancy::script_model::pose_script_dobj_with_materials;
use crate::{
    ItemAssetDraw, ItemDrawPlan, ItemOwnerDraw, XMODEL_OBJECT_ID_ITEM_BASE, append_item_surfaces,
    body_lit_pass_material, dobj_lighting_box_half, stamp_plan_geometry, topology_fingerprint,
};
use anim_iw4::DOBJ_RADIUS_PARENT_ROOT;
use entity_iw4::{ET_ITEM, Trajectory, bg_evaluate_trajectory};
use render_scene::{
    HostGfxScene, ModelLightingOwner, ModelLightingRequest, ModelLightingRequests,
    SmodelPassMaterial, TessMaterials, WorldModelLightingAtlas, WorldPresentFacts,
    XModelSurfaceDraw, scene_quat_from_angles,
};

pub const ITEM_INDEX_STRIDE: i32 = 0x578;

pub const ITEM_LIGHTING_Z_OFS: f32 = 4.0;

#[derive(Resource, Default)]
struct ItemOccupancy {
    rows: Vec<OccupiedItem>,
}

struct OccupiedItem {
    index: usize,
    entnum: i32,
    origin: [f32; 3],
    angles: [f32; 3],
    name: String,
    lighting_origin: [f32; 3],
}

#[derive(Resource, Default)]
struct ItemPoseProduct {
    assets: Vec<ItemPosedAsset>,
    owners: Vec<ItemPosedOwner>,
}

struct ItemPosedAsset {
    name: String,
    surfaces: Vec<PosedModelSurface>,
}

struct ItemPosedOwner {
    asset_index: usize,
    index: usize,
    entnum: u32,
    origin: [f32; 3],
    angles: [f32; 3],
    name: String,
    lighting_origin: [f32; 3],
}

struct ItemSceneSlot<'a> {
    hidden: bool,
    skin_entries: &'a [dpvs_iw4::SceneEntSkinEntry],
}

fn item_scene_slot(scene: &render_scene::GfxScene, entnum: u32) -> ItemSceneSlot<'_> {
    ItemSceneSlot {
        hidden: scene.scene_ent_skips_draw(entnum),
        skin_entries: scene
            .scene_ent_skinned_surfs(entnum)
            .map(|surfs| surfs.entries.as_slice())
            .unwrap_or(&[]),
    }
}

pub fn register_item_systems(app: &mut App) {
    app.init_resource::<ItemDrawPlan>()
        .init_resource::<ItemOccupancy>()
        .init_resource::<ItemPoseProduct>()
        .add_systems(
            Update,
            occupy_item_scene_ents
                .in_set(frame::RenderSet::Anim)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(render_scene::GfxSceneAdd)
                .in_set(AnimSceneSubmit),
        )
        .add_systems(
            Update,
            (pose_items, append_item_draws)
                .chain()
                .after(frame::WorkerCmdSet::CellSceneEnt)
                .after(frame::WorkerCmdSet::DpvsEnt)
                .before(frame::WorkerCmdSet::CellDynModel)
                .in_set(frame::WorkerCmdSet::SkinModel),
        );
}

pub fn item_lighting_origin(origin: [f32; 3]) -> [f32; 3] {
    [origin[0], origin[1], origin[2] + ITEM_LIGHTING_Z_OFS]
}

fn item_weapon_index(index: i32) -> Option<u32> {
    u32::try_from(index.rem_euclid(ITEM_INDEX_STRIDE)).ok()
}

fn item_world_from_local(origin: [f32; 3], angles: [f32; 3]) -> Mat4 {
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

const PERK_SCAVENGER: u32 = 1 << 22;

fn item_is_scavenger(snapshot: &sim::Snapshot, entnum: i32) -> bool {
    snapshot
        .meta
        .item_ammo
        .iter()
        .any(|row| row.entnum == entnum && row.scavenger != 0)
}

fn local_has_scavenger_perk(
    presented: Option<&net::PresentedSnapshot>,
    local: Option<&net::LocalPresentClient>,
) -> bool {
    let Some(id) = local else {
        return false;
    };
    presented
        .and_then(|p| p.player(id.0))
        .is_some_and(|ps| (ps.perks[0] & PERK_SCAVENGER) != 0)
}

fn occupy_item_scene_ents(
    presented: Option<Res<net::PresentedSnapshot>>,
    local_client: Option<Res<net::LocalPresentClient>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    mut occupancy: ResMut<ItemOccupancy>,
    mut scene_skels: ResMut<AnimDObjSceneSkels>,
    mut scene_submissions: MessageWriter<AnimDObjSceneSubmission>,
    cg_clock: Option<Res<net::CgFrameClock>>,
) {
    occupancy.rows.clear();
    let Some(presented_inner) = presented.as_deref() else {
        return;
    };
    let Some(snapshot) = presented_inner.snapshot() else {
        return;
    };
    let hide_scavenger = !local_has_scavenger_perk(Some(presented_inner), local_client.as_deref());
    let at_time = cg_clock
        .as_ref()
        .filter(|clock| clock.started())
        .map(|clock| clock.time())
        .unwrap_or_else(|| sim::level_time_ms(snapshot.tick));
    let weapons_reg = weapons.as_ref().map(|w| &w.0).filter(|reg| !reg.is_empty());
    let catalog = world_weapons
        .as_ref()
        .map(|prepared| &prepared.0)
        .filter(|c| !c.is_empty());
    let items: Vec<_> = snapshot
        .meta
        .entities
        .iter()
        .filter(|es| es.e_type == ET_ITEM)
        .collect();
    let Some(catalog) = catalog else {
        return;
    };

    for (index, es) in items.iter().enumerate() {
        let Some(weapon) = item_weapon_index(es.index) else {
            continue;
        };
        if es.e_flags & 0x20 != 0 {
            continue;
        }
        if hide_scavenger && item_is_scavenger(snapshot, es.number) {
            continue;
        }
        let Some(entry) = weapons_reg.and_then(|reg| reg.world_model_entry(weapon, catalog)) else {
            continue;
        };
        let name = entry.skel.name.as_str();
        let Some(_pose) = entry.skel.pose.as_ref() else {
            continue;
        };
        let origin = evaluate_origin(es, at_time);
        let lighting_origin = item_lighting_origin(origin);
        let apos = Trajectory {
            tr_time: es.apos_tr_time,
            tr_type: es.apos_tr_type,
            tr_duration: es.apos_tr_duration,
            tr_delta: es.apos_tr_delta,
            tr_base: es.apos_tr_base,
        };
        let angles = bg_evaluate_trajectory(&apos, at_time);
        let entnum = u32::try_from(es.number).unwrap_or(0);
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
        occupancy.rows.push(OccupiedItem {
            index,
            entnum: es.number,
            origin,
            angles,
            name: name.to_owned(),
            lighting_origin,
        });
    }
}

fn pose_items(
    occupancy: Res<ItemOccupancy>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    gfx: Res<HostGfxScene>,
    mut product: ResMut<ItemPoseProduct>,
) {
    product.owners.clear();
    if world_weapons
        .as_ref()
        .is_some_and(|value| value.is_changed())
    {
        product.assets.clear();
    }
    let catalog = world_weapons
        .as_ref()
        .map(|prepared| &prepared.0)
        .filter(|c| !c.is_empty());
    let Some(catalog) = catalog else {
        product.assets.clear();
        return;
    };
    for row in &occupancy.rows {
        let entnum = u32::try_from(row.entnum).unwrap_or(0);
        let slot = item_scene_slot(&gfx.scene, entnum);
        if slot.hidden {
            continue;
        }
        let asset_index = if let Some(index) = product
            .assets
            .iter()
            .position(|asset| asset.name == row.name)
        {
            index
        } else {
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
            let index = product.assets.len();
            product.assets.push(ItemPosedAsset {
                name: row.name.clone(),
                surfaces,
            });
            index
        };
        product.owners.push(ItemPosedOwner {
            asset_index,
            index: row.index,
            entnum,
            origin: row.origin,
            angles: row.angles,
            name: row.name.clone(),
            lighting_origin: row.lighting_origin,
        });
    }
}

fn append_item_draws(
    product: Res<ItemPoseProduct>,
    world_weapons: Option<Res<assets::PreparedWorldWeapons>>,
    atlas: Option<Res<WorldModelLightingAtlas>>,
    atpoint: Res<render_scene::DynAtPointLookup>,
    tess: Option<Res<TessMaterials>>,
    facts: Res<WorldPresentFacts>,
    mut plan: ResMut<ItemDrawPlan>,
    mut lighting_requests: ResMut<ModelLightingRequests>,
    mut last_material_generation: Local<Option<render_material::MaterialGenerationId>>,
    mut material_cache: Local<HashMap<String, SmodelPassMaterial>>,
    mut draws: Local<Vec<XModelSurfaceDraw>>,
    mut owners: Local<Vec<ItemOwnerDraw>>,
) {
    draws.clear();
    owners.clear();
    let catalog = world_weapons
        .as_ref()
        .map(|prepared| &prepared.0)
        .filter(|c| !c.is_empty());
    let atlas_ref = atlas.as_deref();
    if catalog.is_none() || atlas_ref.is_none() || !facts.spawned || tess.is_none() {
        let (revision, generation) = (plan.revision, plan.generation);
        *plan = ItemDrawPlan::default();
        plan.revision = revision;
        plan.generation = generation;
        plan.publish_no_rows();
        material_cache.clear();
        *last_material_generation = None;
        return;
    }
    let world_weapons_changed = world_weapons
        .as_ref()
        .is_some_and(|value| value.is_changed());
    let atlas_changed = atlas.as_ref().is_some_and(|value| value.is_changed());
    let tess_changed = tess.as_ref().is_some_and(|value| value.is_changed());
    let (Some(catalog), Some(atlas), Some(tess)) = (catalog, atlas_ref, tess.as_deref()) else {
        unreachable!("required item draw resources checked above");
    };
    let material_generation = tess.catalog.generation_id;
    let generation_changed = world_weapons_changed
        || atlas_changed
        || tess_changed
        || *last_material_generation != Some(material_generation);
    if generation_changed {
        let revision = plan.revision.wrapping_add(1);
        let generation = plan.generation.wrapping_add(1);
        *plan = ItemDrawPlan::default();
        plan.revision = revision;
        plan.generation = generation;
        plan.revisions.bump_admission();
        material_cache.clear();
        *last_material_generation = Some(material_generation);
    }
    for row in &product.owners {
        let posed = &product.assets[row.asset_index];
        let asset_index = if let Some(index) = plan
            .assets
            .iter()
            .position(|asset| asset.model == posed.name)
        {
            index
        } else {
            let Some(entry) = catalog.get(&posed.name) else {
                continue;
            };
            let materials: Vec<Option<SmodelPassMaterial>> = posed
                .surfaces
                .iter()
                .map(|surface| {
                    let present_name = entry.material_present_name(surface.surface_index)?;
                    if let Some(material) = material_cache.get(present_name) {
                        return Some(material.clone());
                    }
                    let material = body_lit_pass_material(atlas, &tess.catalog, present_name)?;
                    material_cache.insert(present_name.to_owned(), material.clone());
                    Some(material)
                })
                .collect();
            if materials.iter().all(Option::is_none) {
                continue;
            }
            let surfaces = append_item_surfaces(&mut plan, &posed.surfaces, &materials);
            if surfaces.is_empty() {
                continue;
            }
            let index = plan.assets.len();
            plan.assets.push(ItemAssetDraw {
                model: posed.name.clone(),
                surfaces,
            });
            index
        };
        let Some(entry) = catalog.get(&row.name) else {
            continue;
        };
        let box_half = entry
            .skel
            .radius
            .and_then(|radius| dobj_lighting_box_half(&[radius], &[DOBJ_RADIUS_PARENT_ROOT]));
        let lookup_fallback = atpoint.fallback(row.lighting_origin, box_half);
        let pending_lighting = Some(lighting_requests.request(ModelLightingRequest {
            owner: ModelLightingOwner::Item(row.entnum),
            origin: row.lighting_origin,
            lookup_fallback,
        }));
        let object_id = XMODEL_OBJECT_ID_ITEM_BASE.saturating_add(row.index as u16);
        owners.push(ItemOwnerDraw {
            object_id,
            model: row.name.clone(),
        });
        let world_from_local = item_world_from_local(row.origin, row.angles);
        for &(surface, material) in &plan.assets[asset_index].surfaces {
            draws.push(XModelSurfaceDraw {
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
                scene_entnum: Some(row.entnum),
            });
        }
        perf::item(entity_iw4::ET_ITEM, None, None, Some("posed"));
    }
    if !draws.is_empty() {
        let topology =
            topology_fingerprint(&plan.indices, &plan.surface_ranges, plan.vertices.len());
        let rev = plan.revision;
        plan.revision = stamp_plan_geometry(&mut plan.revisions, rev, topology);
    }
    plan.publish_frame_rows(&mut draws, &mut owners);
}

fn evaluate_origin(es: &entity_iw4::EntityState, at_time: i32) -> [f32; 3] {
    let traj = Trajectory {
        tr_time: es.tr_time,
        tr_type: es.tr_type,
        tr_duration: es.tr_duration,
        tr_delta: es.tr_delta,
        tr_base: es.tr_base,
    };
    bg_evaluate_trajectory(&traj, at_time)
}
