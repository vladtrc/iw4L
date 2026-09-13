use crate::frame::FrameWorld;
use crate::{AuthorityDObjState, AuthorityModelOwner, EventAudience, Tick};
use std::sync::Arc;
use xmodel_runtime::T5DestructibleDef;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct State {
    definition: Arc<T5DestructibleDef>,
    health: Vec<i16>,
}

pub fn install(dobj: &mut AuthorityDObjState, definition: Arc<T5DestructibleDef>) {
    dobj.t5_destructible = Some(State {
        health: definition.pieces.iter().map(|p| p.health as i16).collect(),
        definition,
    });
}

#[derive(Clone)]
struct Break {
    piece: usize,
    stage: usize,
}

impl State {
    fn damage(
        &mut self,
        index: usize,
        amount: i32,
        exclude: Option<usize>,
        depth: u32,
        breaks: &mut Vec<Break>,
    ) {
        if depth > 20 {
            diag::warn!(
                Sim,
                "T5 DamagePiece recursion limit: {}",
                self.definition.name
            );
            return;
        }
        let piece = &self.definition.pieces[index];
        let old = piece.stage(index, self.health[index]);
        let mut health = self.health[index]
            .wrapping_sub(amount.clamp(i16::MIN as i32, i16::MAX as i32) as i16)
            .max(-1);
        let mut next = piece.stage(index, health);
        let previous = old.map(|s| &piece.stages[s]);
        if previous.is_some_and(|s| s.flags & 1 != 0) {
            return;
        }
        if let Some(end) = next {
            for j in old.map_or(0, |s| s + 1)..=end {
                if piece.stages[j].flags & 1 != 0 {
                    health = ((piece.health as f32 * piece.stages[j].break_health) as i16)
                        .wrapping_add(1);
                    next = piece.stage(index, health);
                    break;
                }
            }
        }
        let parent = usize::from(piece.parent_piece);
        if old != next
            && previous.is_some_and(|s| s.has_phys_preset && s.flags & 4 != 0)
            && self.health.get(parent).is_some_and(|h| *h > 0)
        {
            return;
        }
        let damage_parent = previous.is_some_and(|s| s.flags & 2 != 0);
        self.health[index] = health;
        if old != next {
            if let Some(start) = old {
                for j in start..next.unwrap_or(5) {
                    if piece.stages[j].show_bone.is_some() {
                        breaks.push(Break {
                            piece: index,
                            stage: j,
                        });
                    }
                }
            }
            if next.is_some_and(|next| piece.stages[next].max_time > 0.0) {
                diag::warn!(
                    Sim,
                    "T5 destructible breakafter callback unhosted: {} piece={}",
                    self.definition.name,
                    index
                );
            }
        }
        for child in 0..self.health.len() {
            let p = &self.definition.pieces[child];
            if usize::from(p.parent_piece) == index && Some(index) != exclude {
                let damage = (amount as f32 * p.parent_damage_percent) as i32;
                if damage != 0 {
                    self.damage(child, damage, exclude, depth + 1, breaks);
                }
            }
        }
        if damage_parent && parent < self.health.len() {
            self.damage(parent, amount, Some(index), depth + 1, breaks);
        }
    }
}

fn update_hide_parts(dobj: &mut AuthorityDObjState) {
    let Some(state) = &dobj.t5_destructible else {
        return;
    };
    let Some(cap) = &dobj.capability else {
        return;
    };
    let hide = state
        .definition
        .hide_parts(&state.health, &cap.pose.bone_names);
    if hide != dobj.semantic_state.hide_part_bits {
        dobj.semantic_state.hide_part_bits = hide;
        dobj.pose_request.hide_part_bits = hide;
        dobj.pose_revision = dobj.pose_revision.wrapping_add(1);
        dobj.semantic_state.pose_revision = dobj.pose_revision;
        dobj.current_collision = None;
        dobj.materialized_pose_revision = None;
    }
}

fn publish(world: &mut FrameWorld, tick: Tick, owner: AuthorityModelOwner, breaks: Vec<Break>) {
    if breaks.is_empty() {
        return;
    }
    let Some(dobj) = world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_mut())
    else {
        return;
    };
    let Some(state) = &dobj.t5_destructible else {
        return;
    };
    let definition = state.definition.clone();
    let pose = dobj
        .capability
        .as_ref()
        .and_then(|cap| cap.pose(&dobj.pose_request, dobj.world_from_model).ok());
    let mut events = Vec::new();
    for event in breaks {
        let piece = &definition.pieces[event.piece];
        let st = &piece.stages[event.stage];

        let bone = dobj.capability.as_ref().and_then(|cap| {
            cap.pose
                .bone_names
                .iter()
                .position(|name| Some(name) == piece.stages[0].show_bone.as_ref())
        });
        let matrix = bone
            .and_then(|b| pose.as_ref()?.get(b))
            .copied()
            .unwrap_or(dobj.world_from_model);
        let origin = matrix.w_axis.truncate().to_array();
        let direction = matrix.x_axis.truncate().normalize().to_array();
        diag::info!(
            Sim,
            "T5 destructible {} piece={} leaving_stage={} fx={:?} health={}",
            definition.name,
            event.piece,
            event.stage,
            st.break_effect,
            state.health[event.piece]
        );
        if st.has_phys_preset || st.spawn_models.iter().any(Option::is_some) {
            diag::warn!(
                Sim,
                "T5 destructible debris unhosted: {} piece={} stage={}",
                definition.name,
                event.piece,
                event.stage
            );
        }
        if let Some(notify) = &st.break_notify {
            diag::warn!(
                Sim,
                "T5 destructible script notify unhosted: {} {}",
                definition.name,
                notify
            );
        }
        if let Some(fx) = &st.break_effect {
            events.push((
                entity_iw4::EntityEventKind::PLAY_FX,
                fx.clone(),
                origin,
                direction,
            ));
        }
        if let Some(sound) = &st.break_sound {
            events.push((
                entity_iw4::EntityEventKind::SOUND_ALIAS,
                sound.clone(),
                origin,
                direction,
            ));
        }
    }
    update_hide_parts(dobj);
    for (kind, name, origin, direction) in events {
        let index = if kind == entity_iw4::EntityEventKind::PLAY_FX {
            world.effect_name_index(&name)
        } else {
            world.sound_alias_index(&name)
        };
        world.push_entity_event(
            tick,
            EventAudience::All,
            kind,
            crate::EntityEventPayload {
                number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                event_parm: i32::from(index),
                origin,
                direction,
                ..Default::default()
            },
        );
    }
}

pub(crate) fn apply_hit(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    bone: u16,
    amount: u32,
) -> bool {
    let Some(dobj) = world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_mut())
    else {
        return false;
    };
    let Some(state) = &mut dobj.t5_destructible else {
        return false;
    };
    let tag = dobj
        .capability
        .as_ref()
        .and_then(|cap| cap.pose.bone_names.get(usize::from(bone)));

    let index = state
        .definition
        .pieces
        .iter()
        .enumerate()
        .position(|(i, p)| {
            p.stage(i, state.health[i])
                .is_some_and(|st| p.stages[st].show_bone.as_ref() == tag)
        })
        .unwrap_or(0);
    let damage = (amount as f32 * state.definition.pieces[index].bullet_damage_scale) as i32;
    let mut breaks = Vec::new();
    state.damage(index, damage, None, 0, &mut breaks);
    publish(world, tick, owner, breaks);
    true
}
