use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use anim_iw4::{
    DOBJ_RADIUS_PARENT_ROOT, PLAYER_ANIM_RAW_MASK, PlayerAnimValue,
    XANIM_LEGS_PARENT_WEIGHT_WHEN_TORSO, xanim_client_anim_blend_ms,
    xanim_client_anim_playback_rate, xanim_goal_time_from_blend_ms,
};
use bevy::prelude::*;

use crate::anim::xmodel_pose::PosedSmodelSurface;

#[derive(Clone, Debug)]
pub struct PersistentRemoteTree {
    pub runtime: assets::dobj::XAnimTreeRuntime,
    pub dobj: Option<assets::DObj>,
    pub reuse_key: Option<assets::dobj::DObjReuseKey>,
    pub legs: u16,
    pub torso: u16,
    pub legs_restart: bool,
    pub torso_restart: bool,

    pub cloned: bool,

    pub occupation_tr_time: Option<i32>,

    pub legs_rate_sample: ClientAnimSample,

    pub torso_rate_sample: ClientAnimSample,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientAnimSample {
    pub origin: [f32; 3],
    pub time_ms: i32,
    pub move_speed: f32,
    pub ladder: bool,
}

pub struct PoseClips {
    pub clip: Arc<assets::AnimClip>,
    pub torso_clip: Option<Arc<assets::AnimClip>>,
    pub legs_for_tree: Arc<assets::AnimClip>,
}

#[derive(Resource, Default)]
pub struct RemoteBodyTrees {
    by_ent: HashMap<u32, PersistentRemoteTree>,
}

impl RemoteBodyTrees {
    pub fn get(&self, persist_key: u32) -> Option<&PersistentRemoteTree> {
        self.by_ent.get(&persist_key)
    }

    pub fn get_mut(&mut self, persist_key: u32) -> Option<&mut PersistentRemoteTree> {
        self.by_ent.get_mut(&persist_key)
    }

    pub fn remove(&mut self, persist_key: u32) -> Option<PersistentRemoteTree> {
        self.by_ent.remove(&persist_key)
    }

    pub fn retain_live(&mut self, live: &HashSet<u32>) {
        self.by_ent.retain(|ent, _| live.contains(ent));
    }
}

pub struct AdvancedRemoteTree {
    pub runtime: assets::dobj::XAnimTreeRuntime,
    pub clips: Option<PoseClips>,
    pub reused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkinAfterPose {
    Culled,
    ReuseCache,
    Blend,
}

pub fn skin_after_pose(culled: bool, pose_same: bool, has_cache: bool) -> SkinAfterPose {
    if culled {
        SkinAfterPose::Culled
    } else if pose_same && has_cache {
        SkinAfterPose::ReuseCache
    } else {
        SkinAfterPose::Blend
    }
}

pub fn skip_frozen_corpse_dobj(
    is_corpse: bool,
    tree_reused: bool,
    last_was_cache_hit: bool,
    has_cache: bool,
) -> bool {
    is_corpse && tree_reused && last_was_cache_hit && has_cache
}

pub fn clone_corpse_tree_from_victim(
    trees: &mut RemoteBodyTrees,
    corpse_ent: u32,
    victim: u32,
    tr_time: i32,
) {
    if trees
        .by_ent
        .get(&corpse_ent)
        .is_some_and(|slot| slot.occupation_tr_time == Some(tr_time))
    {
        return;
    }
    let Some(src) = trees.by_ent.get(&victim).cloned() else {
        trees.by_ent.remove(&corpse_ent);
        return;
    };
    let mut dst = src;
    dst.dobj = None;
    dst.reuse_key = None;
    dst.cloned = true;
    dst.occupation_tr_time = Some(tr_time);
    trees.by_ent.insert(corpse_ent, dst);
}

pub fn packed_anim(raw: i32) -> Option<PlayerAnimValue> {
    let value = PlayerAnimValue::from_raw((raw as u16) & PLAYER_ANIM_RAW_MASK)?;
    (value.effective_index() != 0).then_some(value)
}

pub fn zero_anim() -> PlayerAnimValue {
    PlayerAnimValue::from_raw(0).expect("zero fits PLAYER_ANIM_RAW_MASK")
}

pub fn advance_remote_tree(
    tree: &assets::CompiledAnimTreeDefinition,
    script: &assets::ParsedPlayerAnimScript,
    catalog: &assets::XAnimCatalog,
    legs: PlayerAnimValue,
    torso: PlayerAnimValue,
    persist_key: u32,
    dt: f32,
    origin: [f32; 3],
    pose_time_ms: i32,
    trees: &mut RemoteBodyTrees,
) -> Result<AdvancedRemoteTree, String> {
    let legs_index = legs.effective_index();
    let torso_index = torso.effective_index();
    let legs_restart = legs.restart_toggle();
    let torso_restart = torso.restart_toggle();
    let prev = trees.by_ent.remove(&persist_key);
    let prev_restart_changed_legs = prev
        .as_ref()
        .is_some_and(|slot| slot.legs_restart != legs_restart);
    let prev_restart_changed_torso = prev
        .as_ref()
        .is_some_and(|slot| slot.torso_restart != torso_restart);
    let reused = prev.as_ref().is_some_and(|slot| {
        tree_clips_unchanged(
            slot.cloned,
            slot.legs,
            slot.torso,
            slot.legs_restart,
            slot.torso_restart,
            legs_index,
            torso_index,
            legs_restart,
            torso_restart,
        )
    });

    let mut clips = None;
    let mut slot = if reused {
        let mut slot = prev.expect("reuse checked");
        slot.legs = legs_index;
        slot.torso = torso_index;
        slot.legs_restart = legs_restart;
        slot.torso_restart = torso_restart;
        slot
    } else {
        let clip = decode_leaf_clip(tree, catalog, legs_index)?;
        let (legs_for_tree, torso_clip) = if torso_index == 0 {
            (Arc::clone(&clip), None)
        } else {
            let torso_clip = decode_leaf_clip(tree, catalog, torso_index)?;
            let overlayed = overlay_legs_clip(&clip, &torso_clip);
            (Arc::new(overlayed), Some(torso_clip))
        };
        let old_legs = prev.as_ref().map(|slot| slot.legs).unwrap_or(0);
        let old_torso = prev.as_ref().map(|slot| slot.torso).unwrap_or(0);
        let old_legs_moving = prev
            .as_ref()
            .is_some_and(|slot| slot.legs_rate_sample.move_speed > 0.0);
        let old_torso_moving = prev
            .as_ref()
            .is_some_and(|slot| slot.torso_rate_sample.move_speed > 0.0);
        let mut bound: HashMap<u16, Arc<assets::AnimClip>> = HashMap::new();
        if let Some(slot) = &prev {
            for (index, state) in slot.runtime.states().iter().enumerate() {
                if state.weight > 0.0 || state.goal_weight > 0.0 {
                    if let Some(clip) = slot
                        .runtime
                        .leaf_clip(assets::dobj::XAnimNodeId(index as u16))
                    {
                        bound.insert(index as u16, clip);
                    }
                }
            }
        }
        bound.insert(legs_index, Arc::clone(&legs_for_tree));
        if let Some(torso_for_tree) = torso_clip.clone() {
            bound.insert(torso_index, torso_for_tree);
        }
        let definition = tree
            .to_runtime_definition(|index, _name| bound.get(&index).cloned())
            .map_err(|error| error.to_string())?;
        let mut slot = match prev {
            Some(mut slot) if slot.runtime.states().len() == definition.nodes().len() => {
                slot.runtime
                    .rebind(definition)
                    .map_err(|error| error.to_string())?;
                slot
            }
            Some(_) | None => PersistentRemoteTree {
                runtime: assets::dobj::XAnimTreeRuntime::new(definition),
                dobj: None,
                reuse_key: None,
                legs: 0,
                torso: 0,
                legs_restart: false,
                torso_restart: false,
                cloned: false,
                occupation_tr_time: None,
                legs_rate_sample: ClientAnimSample::default(),
                torso_rate_sample: ClientAnimSample::default(),
            },
        };
        let legs_properties = script.animation_properties(legs_index);
        let torso_properties = script.animation_properties(torso_index);
        slot.legs_rate_sample.move_speed = if legs_properties.stationary {
            0.0
        } else {
            clip.move_speed()
        };
        slot.legs_rate_sample.ladder = legs_properties.ladder;
        slot.torso_rate_sample.move_speed = if torso_properties.stationary {
            0.0
        } else {
            torso_clip.as_ref().map_or(0.0, |clip| clip.move_speed())
        };
        slot.torso_rate_sample.ladder = torso_properties.ladder;

        let locomotion_phase = if old_legs != legs_index
            && old_legs_moving
            && slot.legs_rate_sample.move_speed > 0.0
            && clip.looping
            && slot
                .runtime
                .leaf_clip(assets::dobj::XAnimNodeId(old_legs))
                .is_some_and(|clip| clip.looping)
        {
            Some(slot.runtime.states()[old_legs as usize].time)
        } else {
            None
        };
        apply_remote_client_anim_goals(
            &mut slot.runtime,
            old_legs,
            old_torso,
            legs_index,
            torso_index,
            prev_restart_changed_legs,
            prev_restart_changed_torso,
            old_legs_moving,
            old_torso_moving,
            slot.legs_rate_sample.move_speed > 0.0,
            slot.torso_rate_sample.move_speed > 0.0,
            [legs_properties.blend_ms, torso_properties.blend_ms],
        )?;
        if let Some(time) = locomotion_phase {
            let id = assets::dobj::XAnimNodeId(legs_index);
            let mut state = slot.runtime.states()[legs_index as usize];
            state.time = time;
            state.old_time = time;
            slot.runtime
                .set_state(id, state)
                .map_err(|error| error.to_string())?;
        }
        clips = Some(PoseClips {
            clip,
            torso_clip,
            legs_for_tree,
        });
        slot.legs = legs_index;
        slot.torso = torso_index;
        slot.legs_restart = legs_restart;
        slot.torso_restart = torso_restart;
        slot
    };
    apply_remote_client_anim_rates(
        &mut slot.runtime,
        &mut slot.legs_rate_sample,
        &mut slot.torso_rate_sample,
        legs_index,
        torso_index,
        origin,
        pose_time_ms,
    )?;
    slot.runtime.update(dt).map_err(|error| error.to_string())?;
    if !leaf_enrolled(&slot.runtime, legs_index) {
        return Err("complete goal weight left the legs leaf at 0".into());
    }
    if torso_index != 0 && !leaf_enrolled(&slot.runtime, torso_index) {
        return Err("complete goal weight left the torso leaf at 0".into());
    }
    trees.by_ent.insert(persist_key, slot);
    let slot = trees.by_ent.get_mut(&persist_key).expect("tree inserted");
    slot.cloned = false;
    Ok(AdvancedRemoteTree {
        runtime: slot.runtime.clone(),
        clips,
        reused,
    })
}

fn leaf_enrolled(runtime: &assets::dobj::XAnimTreeRuntime, index: u16) -> bool {
    runtime
        .states()
        .get(index as usize)
        .is_some_and(|state| state.weight > 0.0 || state.goal_weight > 0.0)
}

fn apply_remote_client_anim_rates(
    runtime: &mut assets::dobj::XAnimTreeRuntime,
    legs_sample: &mut ClientAnimSample,
    torso_sample: &mut ClientAnimSample,
    legs_index: u16,
    torso_index: u16,
    origin: [f32; 3],
    pose_time_ms: i32,
) -> Result<(), String> {
    apply_one_client_anim_rate(runtime, legs_index, origin, pose_time_ms, legs_sample)?;
    if torso_index != 0 {
        apply_one_client_anim_rate(runtime, torso_index, origin, pose_time_ms, torso_sample)?;
    }
    Ok(())
}

fn apply_one_client_anim_rate(
    runtime: &mut assets::dobj::XAnimTreeRuntime,
    index: u16,
    origin: [f32; 3],
    pose_time_ms: i32,
    sample: &mut ClientAnimSample,
) -> Result<(), String> {
    if index == 0 {
        return Ok(());
    }
    if pose_time_ms < sample.time_ms {
        sample.time_ms = 0;
    }
    let Some(rate) = xanim_client_anim_playback_rate(
        origin,
        sample.origin,
        pose_time_ms,
        sample.time_ms,
        sample.move_speed,
        sample.ladder,
    ) else {
        return Ok(());
    };
    runtime
        .set_rate(assets::dobj::XAnimNodeId(index), rate)
        .map_err(|error| error.to_string())?;
    *sample = ClientAnimSample {
        origin,
        time_ms: pose_time_ms,
        ..*sample
    };
    Ok(())
}

fn apply_remote_client_anim_goals(
    runtime: &mut assets::dobj::XAnimTreeRuntime,
    old_legs: u16,
    old_torso: u16,
    legs_index: u16,
    torso_index: u16,
    legs_restart: bool,
    torso_restart: bool,
    old_legs_moving: bool,
    old_torso_moving: bool,
    new_legs_moving: bool,
    new_torso_moving: bool,
    authored_blend_ms: [i32; 2],
) -> Result<(), String> {
    let legs_time = xanim_goal_time_from_blend_ms(xanim_client_anim_blend_ms(
        legs_index,
        authored_blend_ms[0],
        old_legs != 0,
        false,
        old_legs_moving,
        new_legs_moving,
    ));
    let torso_time = xanim_goal_time_from_blend_ms(xanim_client_anim_blend_ms(
        torso_index,
        authored_blend_ms[1],
        old_torso != 0,
        true,
        old_torso_moving,
        new_torso_moving,
    ));
    if old_legs != 0 && old_legs != legs_index {
        runtime
            .set_goal_weight(assets::dobj::XAnimNodeId(old_legs), 0.0, legs_time)
            .map_err(|error| error.to_string())?;
    }
    if old_torso != 0 && old_torso != torso_index {
        runtime
            .set_goal_weight(assets::dobj::XAnimNodeId(old_torso), 0.0, torso_time)
            .map_err(|error| error.to_string())?;
    }
    let legs_weight = if torso_index != 0 {
        XANIM_LEGS_PARENT_WEIGHT_WHEN_TORSO
    } else {
        1.0
    };
    if old_legs == legs_index && !legs_restart {
        runtime
            .set_goal_weight(
                assets::dobj::XAnimNodeId(legs_index),
                legs_weight,
                legs_time,
            )
            .map_err(|error| error.to_string())?;
    } else {
        runtime
            .set_complete_goal_weight_in(
                assets::dobj::XAnimNodeId(legs_index),
                0.0,
                legs_weight,
                legs_time,
            )
            .map_err(|error| error.to_string())?;
    }
    if torso_index != 0 {
        if old_torso == torso_index && !torso_restart {
            runtime
                .set_goal_weight(assets::dobj::XAnimNodeId(torso_index), 1.0, torso_time)
                .map_err(|error| error.to_string())?;
        } else {
            runtime
                .set_complete_goal_weight_in(
                    assets::dobj::XAnimNodeId(torso_index),
                    0.0,
                    1.0,
                    torso_time,
                )
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn tree_clips_unchanged(
    cloned: bool,
    last_legs: u16,
    last_torso: u16,
    last_legs_restart: bool,
    last_torso_restart: bool,
    legs: u16,
    torso: u16,
    legs_restart: bool,
    torso_restart: bool,
) -> bool {
    cloned
        || (last_legs == legs
            && last_torso == torso
            && last_legs_restart == legs_restart
            && last_torso_restart == torso_restart)
}

fn decode_leaf_clip(
    tree: &assets::CompiledAnimTreeDefinition,
    catalog: &assets::XAnimCatalog,
    leaf_index: u16,
) -> Result<Arc<assets::AnimClip>, String> {
    let leaf = tree
        .node(leaf_index)
        .ok_or_else(|| format!("packed index {leaf_index} missing from compiled tree"))?;
    match catalog.clip(assets::AssetNamespace::Iw4, &leaf.name) {
        Some(clip) => Ok(clip),
        None => {
            let detail = match catalog.get(assets::AssetNamespace::Iw4, &leaf.name) {
                None => "not in XAnimParts catalog".into(),
                Some(captured) => match assets::AnimClip::from_parts(&captured.parts) {
                    Err(error) => format!("{error:?}"),
                    Ok(_) => "from_parts ok but clip() missed".into(),
                },
            };
            Err(format!("decode failed for `{}`: {detail}", leaf.name))
        }
    }
}

pub fn overlay_legs_clip(legs: &assets::AnimClip, torso: &assets::AnimClip) -> assets::AnimClip {
    let names: HashSet<&str> = torso
        .tracks
        .iter()
        .map(|track| track.name.as_str())
        .collect();
    let mut overlayed = legs.clone();
    overlayed
        .tracks
        .retain(|track| !names.contains(track.name.as_str()));
    overlayed
}

pub fn occupy_lod_byte(lod: Option<u8>) -> i8 {
    lod.and_then(|lod| i8::try_from(lod).ok()).unwrap_or(-1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WrittenSlotLod {
    Missing,

    Invalid,
    Written(u8),
}

pub fn written_slot_lod(slot_lods: &[i8], model: usize) -> WrittenSlotLod {
    match slot_lods.get(model) {
        None => WrittenSlotLod::Missing,
        Some(&lod) => match u8::try_from(lod) {
            Ok(lod) => WrittenSlotLod::Written(lod),
            Err(_) => WrittenSlotLod::Invalid,
        },
    }
}

pub fn resolve_slot_lod(
    slot_lods: &[i8],
    model: usize,
    ramp: impl FnOnce() -> Option<u8>,
) -> Option<u8> {
    match written_slot_lod(slot_lods, model) {
        WrittenSlotLod::Written(lod) => Some(lod),
        WrittenSlotLod::Missing => ramp(),
        WrittenSlotLod::Invalid => None,
    }
}

pub fn kit_dobj_radius(radii: impl IntoIterator<Item = f32>) -> Option<f32> {
    radii
        .into_iter()
        .fold(None::<f32>, |acc, r| Some(acc.map_or(r, |a| a.max(r))))
}

pub struct KitModel<'a> {
    pub name: &'a str,
    pub skel: &'a assets::ModelSkel,
    pub hide_tags: Vec<String>,
}

pub fn occupy_remote_kit_dobj<'a>(
    bodies: &'a assets::PreparedBodies,
    weapons: Option<&'a assets::PreparedWeapons>,
    world_weapons: Option<&'a assets::PreparedWorldWeapons>,
    axis: bool,
    weapon: u32,
) -> Option<(Vec<KitModel<'a>>, Option<f32>)> {
    let kits = bodies.0.kits();
    let kit = kits.kit(axis)?;
    let body = bodies.0.get(&kit.body)?;
    if body.skel.positions.is_empty() || body.skel.bones.is_empty() {
        return None;
    }
    let _pose_src = body.skel.pose.as_ref()?;
    let mut skels: Vec<KitModel<'a>> = vec![KitModel {
        name: body.skel.name.as_str(),
        skel: &body.skel,
        hide_tags: Vec::new(),
    }];
    if let Some(name) = kit.head.as_deref() {
        if let Some(entry) = bodies.0.get(name) {
            if let (Some(_head_pose), Some(_tag)) = (
                entry.skel.pose.as_ref(),
                assets::tp_head_attach_tag(&body.skel.bone_names),
            ) {
                skels.push(KitModel {
                    name: entry.skel.name.as_str(),
                    skel: &entry.skel,
                    hide_tags: Vec::new(),
                });
            }
        }
    }
    if weapon != 0 {
        if let (Some(registry), Some(catalog)) = (weapons, world_weapons) {
            if let Some(entry) = registry.0.world_model_entry(weapon, &catalog.0) {
                if let (Some(_gun_pose), Some(_tag)) = (
                    entry.skel.pose.as_ref(),
                    assets::tp_weapon_attach_tag(&body.skel.bone_names),
                ) {
                    skels.push(KitModel {
                        name: entry.skel.name.as_str(),
                        skel: &entry.skel,
                        hide_tags: weapons
                            .map(|registry| assets::effective_hide_tags(&registry.0, weapon))
                            .unwrap_or_default(),
                    });
                    for attachment in world_attachments(&registry.0, &catalog.0, weapon) {
                        skels.push(KitModel {
                            name: attachment.entry.skel.name.as_str(),
                            skel: &attachment.entry.skel,
                            hide_tags: Vec::new(),
                        });
                    }
                }
            }
        }
    }

    let radius = kit_dobj_radius(skels.iter().filter_map(|model| model.skel.radius));
    Some((skels, radius))
}

pub struct RemoteModelSet<'a> {
    pub body_name: String,
    pub head_name: String,
    pub body: &'a assets::BodyMeshEntry,
    pub head: Option<&'a assets::BodyMeshEntry>,
    pub gun: Option<&'a assets::WorldWeaponEntry>,

    pub world_gun_gap: Option<WorldGunGap>,
    pub dobj_models: Vec<(&'a assets::ModelPoseSrc, Option<assets::Attach>)>,
    pub gun_model_index: usize,
    pub attachments: Vec<(&'a assets::WorldWeaponEntry, usize)>,
}

pub(crate) struct WorldAttachment<'a> {
    pub(crate) entry: &'a assets::WorldWeaponEntry,
    pub(crate) index: assets::WorldWeaponIndex,
    pub(crate) tag: String,
}

pub(crate) fn world_attachments<'a>(
    registry: &assets::WeaponRegistry,
    catalog: &'a assets::WorldWeaponCatalog,
    weapon: u32,
) -> Vec<WorldAttachment<'a>> {
    if registry.world_catalog_identity() != catalog.identity() {
        return Vec::new();
    }
    registry
        .attachment_world_model_edges_of(weapon)
        .iter()
        .zip(registry.attachment_world_mounts_of(weapon))
        .filter_map(|(edge, tag)| {
            let index = edge.bound_index()?;
            let entry = catalog.get_at(index)?;
            entry.skel.pose.as_ref()?;
            let tag = tag.as_ref()?;
            Some(WorldAttachment {
                entry,
                index: assets::WorldWeaponIndex::from_order(index),
                tag: tag.clone(),
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldGunGap {
    pub weapon: u32,

    pub world_model: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteModelLods {
    pub body: u8,
    pub head: Option<u8>,
    pub gun: Option<u8>,
    pub attachments: Vec<Option<u8>>,
}

impl RemoteModelLods {
    pub const fn tuple(&self) -> (Option<u8>, Option<u8>, Option<u8>) {
        (Some(self.body), self.head, self.gun)
    }
}

pub fn select_remote_lods(
    models: &RemoteModelSet<'_>,
    slot_lods: &[i8],
    mut camera_lod: impl FnMut(&assets::ModelSkel) -> Option<u8>,
) -> Option<RemoteModelLods> {
    let mut slot_lod = |model: usize, skel: &assets::ModelSkel| -> Option<u8> {
        resolve_slot_lod(slot_lods, model, || camera_lod(skel))
    };
    let body = slot_lod(0, &models.body.skel)?;
    let head = models.head.and_then(|head| slot_lod(1, &head.skel));
    let gun = models
        .gun
        .and_then(|gun| slot_lod(usize::from(models.head.is_some()) + 1, &gun.skel));
    let attachments = models
        .attachments
        .iter()
        .map(|(entry, model)| slot_lod(*model, &entry.skel))
        .collect();
    Some(RemoteModelLods {
        body,
        head,
        gun,
        attachments,
    })
}

pub fn remote_dobj_reuse_key(
    e_type: i32,
    body_name: &str,
    head_name: &str,
    gun_name: &str,
    attachment_names: &[&str],
) -> assets::dobj::DObjReuseKey {
    let mut parts = vec![body_name, head_name, gun_name];
    parts.extend_from_slice(attachment_names);
    assets::dobj::DObjReuseKey {
        e_type,
        model: assets::dobj::dobj_model_token(&parts),
    }
}

pub fn remote_dobj_reuses(
    has_dobj: bool,
    cached: Option<assets::dobj::DObjReuseKey>,
    next: assets::dobj::DObjReuseKey,
) -> bool {
    has_dobj && cached.is_some_and(|cached| assets::dobj::dobj_reuse_matches(cached, next))
}

pub fn select_remote_models<'a>(
    bodies: &'a assets::PreparedBodies,
    weapons: Option<&assets::PreparedWeapons>,
    world_weapons: Option<&'a assets::PreparedWorldWeapons>,
    axis: bool,
    weapon: u32,
) -> Result<RemoteModelSet<'a>, String> {
    let kits = bodies.0.kits();
    let kit = kits
        .kit(axis)
        .ok_or_else(|| "no third-person kit".to_string())?;
    let body = bodies
        .0
        .get(&kit.body)
        .ok_or_else(|| format!("body `{}` missing from catalog", kit.body))?;
    let pose_src = body
        .skel
        .pose
        .as_ref()
        .ok_or_else(|| format!("body `{}` has no ModelPoseSrc", kit.body))?;

    let mut dobj_models = vec![(pose_src, None)];
    let head = match kit.head.as_deref() {
        None => None,
        Some(name) => {
            let entry = bodies
                .0
                .get(name)
                .ok_or_else(|| format!("head `{name}` missing from catalog"))?;
            let head_pose = entry
                .skel
                .pose
                .as_ref()
                .ok_or_else(|| format!("head `{name}` has no ModelPoseSrc"))?;
            let tag = assets::tp_head_attach_tag(&body.skel.bone_names).ok_or_else(|| {
                format!(
                    "body `{}` has no {}; refusing a headless DObj for `{name}`",
                    kit.body,
                    assets::TP_HEAD_ATTACH_TAG
                )
            })?;
            dobj_models.push((
                head_pose,
                Some(assets::Attach {
                    parent_model: 0,
                    tag: tag.into(),
                }),
            ));
            Some(entry)
        }
    };
    let gun_model_index = dobj_models.len();

    let mut world_gun_gap = None;
    let mut attachments = Vec::new();
    let gun = match weapon {
        0 => None,
        index => {
            let registry = weapons
                .ok_or_else(|| format!("weapon {index} held but PreparedWeapons not installed"))?;
            let catalog = world_weapons.ok_or_else(|| {
                format!(
                    "weapon {index} worldModel requested but PreparedWorldWeapons not installed"
                )
            })?;
            let leftover = registry.0.world_model_of(index).unwrap_or("<none>");
            let mut refuse_gun = |world_model: &str| -> Option<&'a assets::WorldWeaponEntry> {
                world_gun_gap = Some(WorldGunGap {
                    weapon: index,
                    world_model: world_model.to_owned(),
                });
                None
            };
            match registry.0.world_model_entry(index, &catalog.0) {
                None => refuse_gun(leftover),
                Some(entry) => {
                    let name = entry.skel.name.as_str();
                    let gun_pose = entry
                        .skel
                        .pose
                        .as_ref()
                        .ok_or_else(|| format!("world gun `{name}` has no ModelPoseSrc"))?;
                    match assets::tp_weapon_attach_tag(&body.skel.bone_names) {
                        None => refuse_gun(name),
                        Some(tag) => {
                            dobj_models.push((
                                gun_pose,
                                Some(assets::Attach {
                                    parent_model: 0,
                                    tag: tag.into(),
                                }),
                            ));
                            for attachment in world_attachments(&registry.0, &catalog.0, index) {
                                let Some(pose) = attachment.entry.skel.pose.as_ref() else {
                                    continue;
                                };
                                attachments.push((attachment.entry, dobj_models.len()));
                                dobj_models.push((
                                    pose,
                                    Some(assets::Attach {
                                        parent_model: gun_model_index,
                                        tag: attachment.tag.into(),
                                    }),
                                ));
                            }
                            Some(entry)
                        }
                    }
                }
            }
        }
    };
    Ok(RemoteModelSet {
        body_name: kit.body.clone(),
        head_name: kit.head.clone().unwrap_or_default(),
        body,
        head,
        gun,
        world_gun_gap,
        dobj_models,
        gun_model_index,
        attachments,
    })
}

pub fn ensure_remote_dobj(
    models: &RemoteModelSet<'_>,
    e_type: i32,
    persist_key: u32,
    trees: &mut RemoteBodyTrees,
) -> Result<(), String> {
    let gun_name = models
        .gun
        .map(|entry| entry.skel.name.as_str())
        .unwrap_or("");
    let attachment_names: Vec<&str> = models
        .attachments
        .iter()
        .map(|(entry, _)| entry.skel.name.as_str())
        .collect();
    let reuse_key = remote_dobj_reuse_key(
        e_type,
        models.body_name.as_str(),
        models.head_name.as_str(),
        gun_name,
        &attachment_names,
    );
    let slot = trees.get_mut(persist_key).expect("tree slot inserted");
    if !remote_dobj_reuses(slot.dobj.is_some(), slot.reuse_key, reuse_key) {
        slot.dobj =
            Some(assets::DObj::build(&models.dobj_models).map_err(|error| error.to_string())?);
        slot.reuse_key = Some(reuse_key);
    }
    Ok(())
}

pub fn validate_remote_tracks(
    dobj: &assets::DObj,
    clips: Option<&PoseClips>,
    body: &assets::BodyMeshEntry,
    body_name: &str,
) -> Result<(), String> {
    let Some(clips) = clips else {
        return Ok(());
    };
    let match_clip = clips.torso_clip.as_ref().unwrap_or(&clips.clip);
    let matched = dobj
        .tracks_for(match_clip)
        .iter()
        .filter(|bone| bone.is_some())
        .count()
        + if clips.torso_clip.is_some() {
            dobj.tracks_for(&clips.legs_for_tree)
                .iter()
                .filter(|bone| bone.is_some())
                .count()
        } else {
            0
        };
    if matched > 0 {
        return Ok(());
    }
    let tracks = match_clip
        .tracks
        .iter()
        .take(8)
        .map(|track| track.name.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let bones = body
        .skel
        .bone_names
        .iter()
        .take(8)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(",");
    Err(format!(
        "clip `{}` has {} tracks [{tracks}], none match body `{body_name}` bones [{bones}]",
        match_clip.name,
        match_clip.tracks.len(),
    ))
}

pub fn pose_remote_dobj(
    dobj: &assets::DObj,
    runtime: assets::dobj::XAnimTreeRuntime,
    controller: Option<assets::dobj::PlayerControllerInput>,
) -> Result<Vec<Mat4>, String> {
    let request = assets::dobj::DObjPoseRequest::with_tree(runtime);
    assets::dobj::pose_dobj_with_controller(dobj, &request, Mat4::IDENTITY, |dobj, _, locals| {
        if let Some(input) = controller {
            assets::dobj::apply_player_controller(dobj, locals, input);
        }
    })
    .map_err(|error| format!("{error:?}"))
}

pub fn remote_player_controller(
    is_corpse: bool,
    view_pitch_deg: f32,
    prone: bool,
    crouch: bool,
) -> Option<assets::dobj::PlayerControllerInput> {
    if is_corpse {
        None
    } else {
        Some(assets::dobj::PlayerControllerInput {
            view_pitch_deg,
            prone,
            crouch,
            lean_frac: 0.0,
        })
    }
}

pub fn remote_dobj_model_base(
    dobj: &assets::DObj,
    slot: usize,
    attached: &str,
) -> Result<usize, String> {
    dobj.models
        .get(slot)
        .map(|model| model.base)
        .ok_or_else(|| format!("{attached} attached but DObj has no model slot {slot}"))
}

pub struct PendingGunSkin<'a> {
    pub entry: &'a assets::WorldWeaponEntry,
    pub base: usize,
}

pub struct RemoteSkinModels<'a> {
    pub body: &'a assets::BodyMeshEntry,
    pub head: Option<(&'a assets::BodyMeshEntry, usize)>,
    pub gun: Option<PendingGunSkin<'a>>,
    pub head_model: Option<u16>,
    pub gun_model: Option<u16>,
    pub attachments: Vec<(PendingGunSkin<'a>, u16)>,
}

pub fn bind_remote_skin_models<'a>(
    dobj: &assets::DObj,
    models: &RemoteModelSet<'a>,
) -> Result<RemoteSkinModels<'a>, String> {
    let head = match models.head {
        None => None,
        Some(head) => Some((head, remote_dobj_model_base(dobj, 1, "head")?)),
    };
    let gun = match models.gun {
        None => None,
        Some(gun) => Some(PendingGunSkin {
            entry: gun,
            base: remote_dobj_model_base(dobj, models.gun_model_index, "gun")?,
        }),
    };
    let attachments = models
        .attachments
        .iter()
        .map(|&(entry, model)| {
            Ok((
                PendingGunSkin {
                    entry,
                    base: remote_dobj_model_base(dobj, model, "attachment")?,
                },
                model as u16,
            ))
        })
        .collect::<Result<_, String>>()?;
    Ok(RemoteSkinModels {
        body: models.body,
        head,
        gun,
        head_model: head.map(|_| 1),
        gun_model: models.gun.map(|_| models.gun_model_index as u16),
        attachments,
    })
}

pub fn skin_matrices_cover_slot(skin_len: usize, base: usize, bone_n: usize) -> Result<(), String> {
    let need = base.saturating_add(bone_n);
    if skin_len < need {
        return Err(format!(
            "skin matrices {skin_len} shorter than slot base {base} + {bone_n} bones"
        ));
    }
    Ok(())
}

pub fn skin_slot_need(skel: &assets::ModelSkel, skin: &[Mat4], base: usize) -> Result<(), String> {
    skin_matrices_cover_slot(skin.len(), base, skel.bones.len())
}

pub fn dobj_attach_radii(
    body: Option<f32>,
    head: Option<f32>,
    gun: Option<f32>,
) -> (Vec<f32>, Vec<u8>) {
    let mut radii = Vec::new();
    let mut parents = Vec::new();
    if let Some(radius) = body {
        radii.push(radius);
        parents.push(DOBJ_RADIUS_PARENT_ROOT);
        if let Some(radius) = head {
            radii.push(radius);
            parents.push(0);
        }
        if let Some(radius) = gun {
            radii.push(radius);
            parents.push(0);
        }
    }
    (radii, parents)
}

pub fn dobj_radii(
    body: &assets::BodyMeshEntry,
    head: Option<&assets::BodyMeshEntry>,
    gun: Option<&assets::WorldWeaponEntry>,
) -> (Vec<f32>, Vec<u8>) {
    dobj_attach_radii(
        body.skel.radius,
        head.and_then(|head| head.skel.radius),
        gun.and_then(|gun| gun.skel.radius),
    )
}

#[derive(Resource, Default)]
pub struct RemoteSkinPoseHashes {
    geometry_revision: u64,
    last: HashMap<u32, u64>,
    skinned: HashMap<u32, CachedSkinnedBody>,

    last_cache_hits: HashSet<u32>,
}

#[derive(Clone, Default)]
pub struct CpuBodyGeom {
    pub packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
    pub indices: Vec<u32>,
    pub decoded_n: usize,
    pub surfaces: Vec<CpuSurfMeta>,
}

#[derive(Clone)]
pub struct CpuSurfMeta {
    pub index_start: u32,
    pub index_count: u32,
    pub name: Option<String>,
}

pub struct CpuNamedSurface {
    pub vert_n: usize,
    pub indices: Vec<u32>,
    pub name: Option<String>,
    pub packed: Vec<[u8; asset_iw4::size::GFX_PACKED_VERTEX]>,
}

#[derive(Clone)]
pub struct CachedSkinnedBody {
    pub geometry_revision: u64,
    pub geom: Arc<CpuBodyGeom>,
    pub radii: Vec<f32>,
    pub radius_parents: Vec<u8>,

    pub lods: (Option<u8>, Option<u8>, Option<u8>),
}

pub struct RemoteBodySkinnedItem {
    pub geometry_revision: u64,
    pub client: u32,
    pub origin: [f32; 3],
    pub world_from_local: Mat4,
    pub geom: Arc<CpuBodyGeom>,
    pub radii: Vec<f32>,
    pub radius_parents: Vec<u8>,
}

#[derive(Resource, Default)]
pub struct RemoteBodySkinnedQueue {
    items: Vec<RemoteBodySkinnedItem>,
}

impl RemoteBodySkinnedQueue {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RemoteBodySkinnedItem> {
        self.items.iter()
    }

    pub fn take(&mut self) -> Vec<RemoteBodySkinnedItem> {
        std::mem::take(&mut self.items)
    }
}

impl RemoteSkinPoseHashes {
    pub fn take_last_cache_hits(&mut self) -> HashSet<u32> {
        std::mem::take(&mut self.last_cache_hits)
    }

    pub fn retain_live(&mut self, live: &HashSet<u32>) {
        self.last.retain(|ent, _| live.contains(ent));
        self.skinned.retain(|ent, _| live.contains(ent));
        self.last_cache_hits.retain(|ent| live.contains(ent));
    }

    pub fn has_lods(&self, persist_key: u32, lods: (Option<u8>, Option<u8>, Option<u8>)) -> bool {
        self.skinned
            .get(&persist_key)
            .is_some_and(|cached| cached.lods == lods)
    }

    pub fn remember_pose_hash(&mut self, persist_key: u32, hash: u64) -> bool {
        let pose_same = self
            .last
            .get(&persist_key)
            .is_some_and(|&prev| prev == hash);
        self.last.insert(persist_key, hash);
        pose_same
    }
}

pub fn take_unique_geom(pose_hashes: &mut RemoteSkinPoseHashes, persist_key: u32) -> CpuBodyGeom {
    match pose_hashes.skinned.remove(&persist_key) {
        Some(cached) => match Arc::try_unwrap(cached.geom) {
            Ok(mut geom) => {
                geom.packed.clear();
                geom.indices.clear();
                geom.surfaces.clear();
                geom.decoded_n = 0;
                geom
            }
            Err(_) => CpuBodyGeom::default(),
        },
        None => CpuBodyGeom::default(),
    }
}

pub fn hash_skin_matrices(skin: &[Mat4]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    skin.len().hash(&mut hasher);
    for matrix in skin {
        for lane in matrix.to_cols_array() {
            lane.to_bits().hash(&mut hasher);
        }
    }
    hasher.finish()
}

pub fn push_cached_surfaces(
    persist_key: u32,
    transform: &Transform,
    pose_hashes: &mut RemoteSkinPoseHashes,
    submit: &mut RemoteBodySkinnedQueue,
) {
    pose_hashes.last_cache_hits.insert(persist_key);
    let cached = pose_hashes
        .skinned
        .get(&persist_key)
        .expect("caller checked contains_key");
    submit.items.push(RemoteBodySkinnedItem {
        geometry_revision: cached.geometry_revision,
        client: persist_key,
        origin: transform.translation.to_array(),
        world_from_local: transform.to_matrix(),
        geom: Arc::clone(&cached.geom),
        radii: cached.radii.clone(),
        radius_parents: cached.radius_parents.clone(),
    });
}

pub fn commit_assembled_body(
    persist_key: u32,
    transform: &Transform,
    geom: CpuBodyGeom,
    radii: Vec<f32>,
    radius_parents: Vec<u8>,
    lods: (Option<u8>, Option<u8>, Option<u8>),
    submit: &mut RemoteBodySkinnedQueue,
    pose_hashes: &mut RemoteSkinPoseHashes,
) {
    pose_hashes.geometry_revision = pose_hashes
        .geometry_revision
        .checked_add(1)
        .expect("remote geometry revision exhausted");
    let geometry_revision = pose_hashes.geometry_revision;
    let geom = Arc::new(geom);
    pose_hashes.skinned.insert(
        persist_key,
        CachedSkinnedBody {
            geometry_revision,
            geom: Arc::clone(&geom),
            radii: radii.clone(),
            radius_parents: radius_parents.clone(),
            lods,
        },
    );
    submit.items.push(RemoteBodySkinnedItem {
        geometry_revision,
        client: persist_key,
        origin: transform.translation.to_array(),
        world_from_local: transform.to_matrix(),
        geom,
        radii,
        radius_parents,
    });
}

pub fn named_surfaces(
    surfaces: Vec<PosedSmodelSurface>,
    names: &[Option<String>],
    edges: &[assets::AssetEdge<assets::MaterialSpace>],
) -> Vec<CpuNamedSurface> {
    surfaces
        .into_iter()
        .map(|surface| {
            let name = match edges.get(surface.surface_index) {
                Some(assets::AssetEdge::Bound(_)) => {
                    names.get(surface.surface_index).cloned().flatten()
                }
                _ => None,
            };
            CpuNamedSurface {
                vert_n: surface.vert_n,
                indices: surface.indices,
                name,
                packed: surface.packed_vertices,
            }
        })
        .collect()
}

pub fn flatten_cpu_geom(surfaces: Vec<CpuNamedSurface>) -> CpuBodyGeom {
    let mut packed = Vec::new();
    let mut indices = Vec::new();
    let mut metas = Vec::with_capacity(surfaces.len());
    packed.reserve(surfaces.iter().map(|s| s.packed.len()).sum());
    indices.reserve(surfaces.iter().map(|s| s.indices.len()).sum());
    let mut decoded_n = 0usize;
    let mut packed_ok = true;
    for surface in surfaces {
        if surface.indices.is_empty() || surface.vert_n == 0 {
            metas.push(CpuSurfMeta {
                index_start: 0,
                index_count: 0,
                name: surface.name,
            });
            continue;
        }
        if packed_ok && surface.packed.len() == surface.vert_n {
            packed.extend_from_slice(&surface.packed);
        } else {
            packed_ok = false;
            packed.clear();
        }
        let vert_base = decoded_n as u32;
        let index_start = indices.len() as u32;
        indices.extend(surface.indices.iter().map(|&index| vert_base + index));
        metas.push(CpuSurfMeta {
            index_start,
            index_count: surface.indices.len() as u32,
            name: surface.name,
        });
        decoded_n = decoded_n.saturating_add(surface.vert_n);
    }
    if !packed_ok {
        packed.clear();
    }
    CpuBodyGeom {
        packed,
        indices,
        decoded_n,
        surfaces: metas,
    }
}
