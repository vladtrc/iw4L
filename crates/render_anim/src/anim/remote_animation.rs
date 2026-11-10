use super::*;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct PersistentRemoteTree {
    binding: Arc<asset_game::SoldierBodyAnimation>,
    pub(crate) runtime: xmodel_runtime::XAnimTreeRuntime,
    pub(crate) namespace: asset_core::FamilyId,
    pub(crate) dobj: Option<std::sync::Arc<xmodel_runtime::DObj>>,
    pub(crate) reuse_key: Option<xmodel_runtime::DObjReuseKey>,
    pub(crate) legs: u16,
    pub(crate) torso: u16,
    pub(crate) legs_restart: bool,
    pub(crate) torso_restart: bool,

    pub(crate) legs_rate_sample: ClientAnimSample,

    pub(crate) torso_rate_sample: ClientAnimSample,
}

impl PersistentRemoteTree {
    fn clips_unchanged(&self, legs: PlayerAnimValue, torso: PlayerAnimValue) -> bool {
        self.legs == legs.effective_index()
            && self.torso == torso.effective_index()
            && self.legs_restart == legs.restart_toggle()
            && self.torso_restart == torso.restart_toggle()
    }
}

pub struct PoseClips {
    pub clip: Arc<xmodel_runtime::AnimClip>,
    pub torso_clip: Option<Arc<xmodel_runtime::AnimClip>>,
    pub legs_for_tree: Arc<xmodel_runtime::AnimClip>,
}

pub struct AdvancedRemoteTree {
    pub runtime: xmodel_runtime::XAnimTreeRuntime,
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

pub fn fresh_corpse_clip_time(age_ms: i32, clip_length_ms: i32) -> f32 {
    if clip_length_ms <= 0 || age_ms <= 0 {
        return 0.0;
    }
    (age_ms as f32 / clip_length_ms as f32).min(1.0)
}

pub fn packed_anim(raw: i32) -> Option<PlayerAnimValue> {
    let value = PlayerAnimValue::from_raw((raw as u16) & PLAYER_ANIM_RAW_MASK)?;
    (value.effective_index() != 0).then_some(value)
}

pub fn zero_anim() -> PlayerAnimValue {
    PlayerAnimValue::from_raw(0).expect("zero fits PLAYER_ANIM_RAW_MASK")
}

#[derive(Clone, Copy)]
pub struct RemoteAnimTracks {
    legs: PlayerAnimValue,
    torso: PlayerAnimValue,
}

impl RemoteAnimTracks {
    pub fn from_pair(pair: playerstate_iw4::AnimPair) -> Option<Self> {
        let legs = packed_anim(pair.legs_anim)?;
        let torso = packed_anim(pair.torso_anim).unwrap_or_else(zero_anim);
        Some(Self { legs, torso })
    }
}

pub fn advance_remote_tree(
    binding: &Arc<asset_game::SoldierBodyAnimation>,
    tracks: RemoteAnimTracks,
    dt: f32,
    origin: [f32; 3],
    pose_time_ms: i32,
    corpse_age_ms: Option<i32>,
    tree_slot: &mut Option<PersistentRemoteTree>,
) -> Result<AdvancedRemoteTree, String> {
    let RemoteAnimTracks { legs, torso } = tracks;
    let tree = binding.tree();
    let script = binding.script();
    let body = binding.body();
    let legs_index = legs.effective_index();
    let torso_index = torso.effective_index();
    let legs_restart = legs.restart_toggle();
    let torso_restart = torso.restart_toggle();
    let previous = tree_slot.take();
    let prev = previous.filter(|slot| Arc::ptr_eq(&slot.binding, binding));
    let prev_restart_changed_legs = prev
        .as_ref()
        .is_some_and(|slot| slot.legs_restart != legs_restart);
    let prev_restart_changed_torso = prev
        .as_ref()
        .is_some_and(|slot| slot.torso_restart != torso_restart);
    let reused = prev
        .as_ref()
        .is_some_and(|slot| slot.namespace == body.namespace && slot.clips_unchanged(legs, torso));

    let mut clips = None;
    let mut slot = if reused {
        let mut slot = prev.expect("reuse checked");
        slot.legs = legs_index;
        slot.torso = torso_index;
        slot.legs_restart = legs_restart;
        slot.torso_restart = torso_restart;
        slot
    } else {
        let clip = binding.decode_leaf(legs_index)?;
        let (legs_for_tree, torso_clip) = if torso_index == 0 {
            (Arc::clone(&clip), None)
        } else {
            let torso_clip = binding.decode_leaf(torso_index)?;
            let overlayed = overlay_legs_clip(&clip, &torso_clip);
            (Arc::new(overlayed), Some(torso_clip))
        };
        let fresh = prev.is_none();
        let old_legs = prev.as_ref().map(|slot| slot.legs).unwrap_or(0);
        let old_torso = prev.as_ref().map(|slot| slot.torso).unwrap_or(0);
        let old_legs_moving = prev
            .as_ref()
            .is_some_and(|slot| slot.legs_rate_sample.move_speed > 0.0);
        let old_torso_moving = prev
            .as_ref()
            .is_some_and(|slot| slot.torso_rate_sample.move_speed > 0.0);
        let mut bound: HashMap<u16, Arc<xmodel_runtime::AnimClip>> = HashMap::new();
        if let Some(slot) = &prev
            && slot.namespace == body.namespace
        {
            for (index, state) in slot.runtime.states().iter().enumerate() {
                if (state.weight > 0.0 || state.goal_weight > 0.0)
                    && let Some(clip) = slot
                        .runtime
                        .leaf_clip(xmodel_runtime::XAnimNodeId(index as u16))
                {
                    bound.insert(index as u16, clip);
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
                binding: binding.clone(),
                runtime: xmodel_runtime::XAnimTreeRuntime::new(definition),
                namespace: body.namespace,
                dobj: None,
                reuse_key: None,
                legs: 0,
                torso: 0,
                legs_restart: false,
                torso_restart: false,
                legs_rate_sample: ClientAnimSample::default(),
                torso_rate_sample: ClientAnimSample::default(),
            },
        };
        slot.namespace = body.namespace;
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
                .leaf_clip(xmodel_runtime::XAnimNodeId(old_legs))
                .is_some_and(|clip| clip.looping)
        {
            Some(slot.runtime.states()[old_legs as usize].time)
        } else {
            None
        };
        apply_player_anim_goals(
            &mut slot.runtime,
            xmodel_runtime::PlayerBodyBranches {
                legs: xmodel_runtime::XAnimNodeId(
                    tree.index_of("legs")
                        .ok_or("player tree legs branch missing")?,
                ),
                torso: xmodel_runtime::XAnimNodeId(
                    tree.index_of("torso")
                        .ok_or("player tree torso branch missing")?,
                ),
            },
            xmodel_runtime::PlayerAnimTransition {
                previous: old_legs,
                index: legs_index,
                restart: prev_restart_changed_legs,
                was_moving: old_legs_moving,
                moving: slot.legs_rate_sample.move_speed > 0.0,
                authored_blend_ms: legs_properties.blend_ms,
            },
            xmodel_runtime::PlayerAnimTransition {
                previous: old_torso,
                index: torso_index,
                restart: prev_restart_changed_torso,
                was_moving: old_torso_moving,
                moving: slot.torso_rate_sample.move_speed > 0.0,
                authored_blend_ms: torso_properties.blend_ms,
            },
        )?;
        if let Some(time) = locomotion_phase {
            let id = xmodel_runtime::XAnimNodeId(legs_index);
            let mut state = slot.runtime.states()[legs_index as usize];
            state.time = time;
            state.old_time = time;
            slot.runtime
                .set_state(id, state)
                .map_err(|error| error.to_string())?;
        }
        if fresh && let Some(age_ms) = corpse_age_ms {
            for (index, leaf_clip) in [
                (legs_index, Some(&clip)),
                (torso_index, torso_clip.as_ref()),
            ] {
                let Some(leaf_clip) = leaf_clip else {
                    continue;
                };
                let time = fresh_corpse_clip_time(age_ms, leaf_clip.length_msec());
                let mut state = slot.runtime.states()[index as usize];
                state.time = time;
                state.old_time = time;
                slot.runtime
                    .set_state(xmodel_runtime::XAnimNodeId(index), state)
                    .map_err(|error| error.to_string())?;
            }
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
    apply_player_anim_rates(
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
    *tree_slot = Some(slot);
    let slot = tree_slot.as_mut().expect("tree inserted");
    Ok(AdvancedRemoteTree {
        runtime: slot.runtime.clone(),
        clips,
        reused,
    })
}

fn leaf_enrolled(runtime: &xmodel_runtime::XAnimTreeRuntime, index: u16) -> bool {
    runtime
        .states()
        .get(index as usize)
        .is_some_and(|state| state.weight > 0.0 || state.goal_weight > 0.0)
}
