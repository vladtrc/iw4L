use super::{ClientId, PlayerKitCollision};
use anim_iw4::{PLAYER_ANIM_RAW_MASK, PlayerAnimValue};
use playerstate_iw4::PlayerState;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub(super) struct PlayerAnimTreeSlot {
    pub(super) runtime: xmodel_runtime::XAnimTreeRuntime,
    pub(super) leaf: u16,
    pub(super) restart_toggle: bool,
    pub(super) torso: u16,
    pub(super) torso_restart: bool,
    pub(super) kit: usize,
    pub(super) legs_rate_sample: xmodel_runtime::ClientAnimSample,
    pub(super) torso_rate_sample: xmodel_runtime::ClientAnimSample,
    pub(super) persist: i64,
}

#[derive(Clone, Debug)]
pub(super) struct PlayerDobjSlot {
    pub(super) dobj: xmodel_runtime::DObj,
    pub(super) reuse_key: xmodel_runtime::DObjReuseKey,
    pub(super) persist: i64,
}

pub(super) struct PlayerAnimInputs<'a> {
    pub(super) definitions: [Option<&'a Arc<xmodel_runtime::XAnimTreeDefinition>>; 2],
    pub(super) properties: &'a [xmodel_runtime::PlayerAnimProperties],
    pub(super) branches: Option<xmodel_runtime::PlayerBodyBranches>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PlayerBodyRuntime {
    pub(super) player_anim_trees: HashMap<u32, PlayerAnimTreeSlot>,
    pub(super) corpse_anim_trees: HashMap<i32, xmodel_runtime::XAnimTreeRuntime>,
    pub(super) player_dobjs: HashMap<u32, PlayerDobjSlot>,
    pub(super) materialize_error: Option<String>,
}

impl PlayerBodyRuntime {
    pub(super) fn reset(&mut self) {
        self.player_anim_trees.clear();
        self.player_dobjs.clear();
        self.corpse_anim_trees.clear();
        self.materialize_error = None;
    }

    pub(super) fn forget_client(&mut self, id: ClientId) {
        self.player_anim_trees.remove(&id.0);
        self.player_dobjs.remove(&id.0);
    }
    pub(super) fn corpse_delta(&mut self, entnum: i32, msec: i32) -> Option<[f32; 3]> {
        let runtime = self.corpse_anim_trees.get_mut(&entnum)?;
        runtime.update(msec as f32 / 1000.0).ok()?;
        runtime.calc_delta_translation()
    }
    pub(super) fn tick_dobjs<'a>(
        &mut self,
        inputs: impl IntoIterator<Item = (ClientId, &'a PlayerKitCollision)>,
    ) {
        let mut live = std::collections::HashSet::new();
        for (id, kit) in inputs {
            live.insert(id.0);
            let Some(body) = kit.body.as_ref() else {
                self.player_dobjs.remove(&id.0);
                continue;
            };
            let key = kit.reuse_key();
            let reuse = self
                .player_dobjs
                .get(&id.0)
                .is_some_and(|slot| xmodel_runtime::reuse_matches(slot.reuse_key, key));
            if reuse {
                if let Some(slot) = self.player_dobjs.get_mut(&id.0) {
                    slot.persist = 1;
                }
                continue;
            }
            let mut specs: Vec<(
                &xmodel_runtime::ModelPoseSrc,
                Option<xmodel_runtime::Attach>,
            )> = vec![(&body.pose, None)];
            let tag = xmodel_runtime::tp_head_attach_tag(&body.pose.bone_names);
            if let (Some(head), Some(tag)) = (kit.head.as_ref(), tag) {
                specs.push((
                    &head.pose,
                    Some(xmodel_runtime::Attach {
                        parent_model: 0,
                        tag: tag.into(),
                    }),
                ));
            }
            match xmodel_runtime::DObj::build(&specs) {
                Ok(dobj) => {
                    self.player_dobjs.insert(
                        id.0,
                        PlayerDobjSlot {
                            dobj,
                            reuse_key: key,
                            persist: 0,
                        },
                    );
                }
                Err(_) => {
                    self.player_dobjs.remove(&id.0);
                }
            }
        }
        self.player_dobjs.retain(|k, _| live.contains(k));
    }

    pub(super) fn tick_trees(
        &mut self,
        content: PlayerAnimInputs<'_>,
        inputs: impl IntoIterator<Item = (ClientId, usize)>,
        msec: i32,
        player: impl Fn(ClientId) -> Option<PlayerState>,
    ) {
        let dt = msec as f32 / 1000.0;
        if content.definitions.iter().all(Option::is_none) {
            self.player_anim_trees.clear();
            return;
        }
        let mut live = std::collections::HashSet::new();
        for (id, kit) in inputs {
            live.insert(id.0);
            let Some(ps) = player(id) else {
                continue;
            };
            let definition = content.definitions[kit].cloned();
            let Some(definition) = definition else {
                self.player_anim_trees.remove(&id.0);
                continue;
            };
            let Some(legs) =
                PlayerAnimValue::from_raw((ps.legs_anim as u16) & PLAYER_ANIM_RAW_MASK)
            else {
                self.player_anim_trees.remove(&id.0);
                continue;
            };
            let Some(torso) =
                PlayerAnimValue::from_raw((ps.torso_anim as u16) & PLAYER_ANIM_RAW_MASK)
            else {
                self.player_anim_trees.remove(&id.0);
                continue;
            };
            let leaf = legs.effective_index();
            let torso_index = torso.effective_index();
            let restart = legs.restart_toggle();
            let torso_restart = torso.restart_toggle();
            let previous = self
                .player_anim_trees
                .remove(&id.0)
                .filter(|slot| slot.kit == kit);
            let reused = previous.as_ref().is_some_and(|slot| {
                slot.leaf == leaf
                    && slot.torso == torso_index
                    && slot.restart_toggle == restart
                    && slot.torso_restart == torso_restart
            });
            let advanced = (|| -> Result<PlayerAnimTreeSlot, String> {
                let mut slot = if reused {
                    let mut slot = previous.expect("reuse checked");
                    slot.persist = 1;
                    slot
                } else {
                    let clip_at = |index: u16| match definition
                        .nodes()
                        .get(index as usize)
                        .map(|node| &node.kind)
                    {
                        Some(xmodel_runtime::XAnimNodeKind::Leaf { clip, .. }) if index != 0 => {
                            Ok(Arc::clone(clip))
                        }
                        _ => Err(format!("player body leaf {index} missing")),
                    };
                    let clip = clip_at(leaf)?;
                    let torso_clip = if torso_index != 0 {
                        Some(clip_at(torso_index)?)
                    } else {
                        None
                    };
                    let legs_for_tree = torso_clip.as_ref().map_or_else(
                        || Arc::clone(&clip),
                        |torso| Arc::new(xmodel_runtime::overlay_legs_clip(&clip, torso)),
                    );
                    let old_legs = previous.as_ref().map_or(0, |slot| slot.leaf);
                    let old_torso = previous.as_ref().map_or(0, |slot| slot.torso);
                    let legs_restart = previous
                        .as_ref()
                        .is_some_and(|slot| slot.restart_toggle != restart);
                    let torso_restart_changed = previous
                        .as_ref()
                        .is_some_and(|slot| slot.torso_restart != torso_restart);
                    let mut nodes = definition.nodes().to_vec();
                    if let Some(slot) = &previous {
                        for (index, state) in slot.runtime.states().iter().enumerate() {
                            if (state.weight > 0.0 || state.goal_weight > 0.0)
                                && let Some(clip) = slot
                                    .runtime
                                    .leaf_clip(xmodel_runtime::XAnimNodeId(index as u16))
                                && let xmodel_runtime::XAnimNodeKind::Leaf { clip: bound, .. } =
                                    &mut nodes[index].kind
                            {
                                *bound = clip;
                            }
                        }
                    }
                    if let xmodel_runtime::XAnimNodeKind::Leaf { clip, .. } =
                        &mut nodes[leaf as usize].kind
                    {
                        *clip = legs_for_tree;
                    }
                    if let Some(torso_clip) = &torso_clip
                        && let xmodel_runtime::XAnimNodeKind::Leaf { clip, .. } =
                            &mut nodes[torso_index as usize].kind
                    {
                        *clip = Arc::clone(torso_clip);
                    }
                    let bound = Arc::new(
                        xmodel_runtime::XAnimTreeDefinition::new(nodes)
                            .map_err(|e| e.to_string())?,
                    );
                    let mut slot = match previous {
                        Some(mut slot) => {
                            slot.runtime.rebind(bound).map_err(|e| e.to_string())?;
                            slot
                        }
                        None => PlayerAnimTreeSlot {
                            runtime: xmodel_runtime::XAnimTreeRuntime::new(bound),
                            leaf: 0,
                            restart_toggle: false,
                            torso: 0,
                            torso_restart: false,
                            kit,
                            legs_rate_sample: Default::default(),
                            torso_rate_sample: Default::default(),
                            persist: 0,
                        },
                    };
                    let old_legs_moving = slot.legs_rate_sample.move_speed > 0.0;
                    let old_torso_moving = slot.torso_rate_sample.move_speed > 0.0;
                    let properties = |index: u16| {
                        content
                            .properties
                            .get(index as usize)
                            .copied()
                            .unwrap_or(xmodel_runtime::PlayerAnimProperties::default())
                    };
                    let legs_properties = properties(leaf);
                    let torso_properties = properties(torso_index);
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
                    let phase = if old_legs != leaf
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
                    xmodel_runtime::apply_player_anim_goals(
                        &mut slot.runtime,
                        content.branches.ok_or("player body branches missing")?,
                        xmodel_runtime::PlayerAnimTransition {
                            previous: old_legs,
                            index: leaf,
                            restart: legs_restart,
                            was_moving: old_legs_moving,
                            moving: slot.legs_rate_sample.move_speed > 0.0,
                            authored_blend_ms: legs_properties.blend_ms,
                        },
                        xmodel_runtime::PlayerAnimTransition {
                            previous: old_torso,
                            index: torso_index,
                            restart: torso_restart_changed,
                            was_moving: old_torso_moving,
                            moving: slot.torso_rate_sample.move_speed > 0.0,
                            authored_blend_ms: torso_properties.blend_ms,
                        },
                    )?;
                    if let Some(time) = phase {
                        let mut state = slot.runtime.states()[leaf as usize];
                        state.time = time;
                        state.old_time = time;
                        slot.runtime
                            .set_state(xmodel_runtime::XAnimNodeId(leaf), state)
                            .map_err(|e| e.to_string())?;
                    }
                    slot.leaf = leaf;
                    slot.torso = torso_index;
                    slot.restart_toggle = restart;
                    slot.torso_restart = torso_restart;
                    slot.persist = 0;
                    slot
                };
                xmodel_runtime::apply_player_anim_rates(
                    &mut slot.runtime,
                    &mut slot.legs_rate_sample,
                    &mut slot.torso_rate_sample,
                    leaf,
                    torso_index,
                    ps.origin,
                    ps.command_time,
                )?;
                slot.runtime.update(dt).map_err(|e| e.to_string())?;
                Ok(slot)
            })();
            if let Ok(slot) = advanced {
                self.player_anim_trees.insert(id.0, slot);
            }
        }
        self.player_anim_trees.retain(|k, _| live.contains(k));
    }
}
