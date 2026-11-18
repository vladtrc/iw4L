use sim::frame::FrameWorld;
use sim::{AuthorityDObjState, AuthorityModelOwner, EventAudience, Tick};
use std::sync::Arc;
use xmodel_runtime::T5DestructibleDef;

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    definition: Arc<T5DestructibleDef>,
    health: Vec<i16>,
    timers: Vec<(usize, usize, f32, i32)>,
    effects: Vec<(usize, u64)>,
    initial_model: String,
    initial_hide_parts: xmodel_runtime::HidePartBits,
}

pub fn install(dobj: &mut AuthorityDObjState, definition: Arc<T5DestructibleDef>) {
    dobj.extensions.insert(State {
        health: definition.pieces.iter().map(|p| p.health as i16).collect(),
        definition,
        timers: Vec::new(),
        effects: Vec::new(),
        initial_model: dobj.current_model.clone(),
        initial_hide_parts: dobj.semantic_state.hide_part_bits,
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
        explosive: bool,
    ) {
        if depth > 20 {
            diag::warn!(
                Sim,
                "T5 DamagePiece recursion limit: {}",
                self.definition.name
            );
            return;
        }
        if amount <= 0 || index >= self.health.len() {
            return;
        }
        let piece = &self.definition.pieces[index];
        let old = piece.stage(index, self.health[index]);
        let mut health = (i32::from(self.health[index]) - amount).max(-1) as i16;
        let mut next = piece.stage(index, health);
        let previous = old.map(|s| &piece.stages[s]);
        if !explosive && previous.is_some_and(|s| s.flags & 1 != 0) {
            return;
        }
        if !explosive && let Some(end) = next {
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
            self.timers.retain(|(piece, _, _, _)| *piece != index);
            if let Some(next) = next.filter(|next| piece.stages[*next].max_time > 0.0) {
                let stage = &piece.stages[next];
                let amount =
                    i32::from(health) - (piece.health as f32 * stage.break_health) as i32 + 1;
                self.timers.push((index, next, stage.max_time, amount));
            }
        }
        for child in 0..self.health.len() {
            let p = &self.definition.pieces[child];
            if usize::from(p.parent_piece) == index && Some(child) != exclude {
                let damage = (amount as f32 * p.parent_damage_percent) as i32;
                if damage != 0 {
                    self.damage(child, damage, exclude, depth + 1, breaks, explosive);
                }
            }
        }
        if damage_parent && parent < self.health.len() {
            self.damage(parent, amount, Some(index), depth + 1, breaks, explosive);
        }
    }
}

fn update_hide_parts(dobj: &mut AuthorityDObjState) {
    let Some(state) = dobj.extensions.get::<State>() else {
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

fn publish(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    breaks: Vec<Break>,
    attacker: Option<sim::ClientId>,
) {
    if breaks.is_empty() {
        return;
    }
    let base = world
        .entity_collision_capabilities()
        .iter()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_ref())
        .and_then(|d| d.extensions.get::<State>())
        .map(|s| s.definition.model.clone());
    let capability = base
        .as_deref()
        .and_then(|name| world.model_capability(name))
        .flatten();
    let Some(dobj) = world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_mut())
    else {
        return;
    };
    if let Some(base) = base.as_ref().filter(|base| **base != dobj.current_model) {
        dobj.replace_model(base, capability);
    }
    let template = dobj.clone();
    let Some(state) = dobj.extensions.get_mut::<State>() else {
        return;
    };
    let timers = std::mem::take(&mut state.timers);
    let definition = state.definition.clone();
    let loop_sound = definition.pieces.first().and_then(|p| {
        p.stage(0, state.health[0])
            .and_then(|stage| p.stages[stage].loop_sound.clone())
    });
    let mut callbacks = Vec::new();
    let pose = dobj
        .capability
        .as_ref()
        .and_then(|cap| cap.pose(&dobj.pose_request, dobj.world_from_model).ok());
    let mut events = Vec::new();
    let mut effects = Vec::new();
    let mut stopped = Vec::new();
    let mut debris = Vec::new();
    for event in breaks {
        let piece = &definition.pieces[event.piece];
        let st = &piece.stages[event.stage];
        state.effects.retain(|(piece, id)| {
            if *piece == event.piece {
                stopped.push(*id);
                false
            } else {
                true
            }
        });

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
        if st.has_phys_preset
            && piece
                .stage(event.piece, state.health[event.piece])
                .is_none()
            && let Some(bone) = st.show_bone.as_deref().and_then(|name| {
                dobj.capability
                    .as_ref()?
                    .pose
                    .bone_names
                    .iter()
                    .position(|b| b == name)
            })
        {
            let matrix = pose
                .as_ref()
                .and_then(|pose| pose.get(bone))
                .copied()
                .unwrap_or(dobj.world_from_model);
            let mut part = template.clone();
            part.extensions.remove::<State>();
            let mut words = [u32::MAX; 6];
            words[bone / 32] &= !(0x8000_0000 >> (bone % 32));
            part.semantic_state.hide_part_bits = xmodel_runtime::HidePartBits::from_words(words);
            part.pose_request.hide_part_bits = part.semantic_state.hide_part_bits;
            part.current_collision = None;
            part.materialized_pose_revision = None;
            let origin = part.world_from_model.w_axis.truncate();
            let bounds = part
                .capability
                .as_ref()
                .and_then(|cap| cap.bone_collision.get(bone))
                .and_then(Option::as_ref);
            let center = matrix.transform_point3(glam::Vec3::from_array(
                bounds.map_or([0.0; 3], |b| b.midpoint),
            ));
            let offset = (center - origin).to_array();
            let local_half = bounds.map_or([4.0; 3], |b| b.half_size);
            let half = (matrix.x_axis.truncate().abs() * local_half[0]
                + matrix.y_axis.truncate().abs() * local_half[1]
                + matrix.z_axis.truncate().abs() * local_half[2])
                .to_array();
            let velocity = piece.launch.map_or([0.0, 0.0, 100.0], |(angles, power)| {
                let (forward, _, _) = math_iw4::angle_vectors(angles);
                part.world_from_model
                    .transform_vector3(glam::Vec3::from_array(forward) * power)
                    .to_array()
            });
            debris.push((part, offset, half, velocity));
        }
        if st.spawn_models.iter().any(Option::is_some) {
            diag::warn!(
                Sim,
                "destructible spawn-model debris unavailable: {} piece={} stage={}",
                definition.name,
                event.piece,
                event.stage
            );
        }
        if let Some(notify) = &st.break_notify {
            callbacks.push(notify.clone());
        }
        if let Some(fx) = st.break_effect.as_ref().filter(|_| {
            !st.break_effect_looping
                || piece.stage(event.piece, state.health[event.piece]) == Some(event.stage + 1)
        }) {
            effects.push((
                event.piece,
                st.break_effect_looping,
                fx.clone(),
                origin,
                direction,
                matrix.z_axis.truncate().normalize().to_array(),
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
    for id in stopped {
        sim::script::stop_destructible_effect(world.ecs(), id);
    }
    for (part, offset, half, velocity) in debris {
        sim::script::destructible_debris(world.ecs(), part, offset, half, velocity);
    }
    sim::script::set_destructible_model(world.ecs(), owner, base.as_deref(), loop_sound.as_deref());
    let attacker_value = sim::script::destructible_attacker(world.ecs(), attacker);
    for notify in callbacks {
        sim::script::destructible_callback(
            world.ecs(),
            owner,
            "iw4l_maps/destructibles::broken",
            vec![sim::script::Value::string(&notify), attacker_value.clone()],
        );
    }
    for (piece, stage, duration, amount) in timers {
        sim::script::destructible_callback(
            world.ecs(),
            owner,
            "iw4l_maps/destructibles::break_after",
            vec![
                sim::script::Value::Int(piece as i32),
                sim::script::Value::Int(stage as i32),
                sim::script::Value::Float(duration),
                sim::script::Value::Int(amount),
                attacker_value.clone(),
            ],
        );
    }
    for (piece, looping, name, origin, direction, up) in effects {
        if let Some(id) = sim::script::destructible_effect(world.ecs(), name, origin, direction, up)
            && looping
            && let Some(state) = world
                .entity_collision_capabilities_mut()
                .iter_mut()
                .find(|row| row.owner == owner)
                .and_then(|row| row.dobj.as_mut())
                .and_then(|dobj| dobj.extensions.get_mut::<State>())
        {
            state.effects.push((piece, id));
        }
    }
    for (kind, name, origin, direction) in events {
        let index = world.sound_alias_index(&name);
        world.push_entity_event(
            tick,
            EventAudience::All,
            kind,
            sim::EntityEventPayload {
                number: i32::from(trace_iw4::ENTITYNUM_WORLD),
                event_parm: i32::from(index),
                origin,
                direction,
                ..Default::default()
            },
        );
    }
}

pub fn apply_hit(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    bone: u16,
    amount: u32,
    attacker: Option<sim::ClientId>,
) -> bool {
    let Some(dobj) = world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_mut())
    else {
        return false;
    };
    let Some(state) = dobj.extensions.get_mut::<State>() else {
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
    state.damage(index, damage, None, 0, &mut breaks, false);
    publish(world, tick, owner, breaks, attacker);
    true
}

pub fn apply_piece_hit(
    world: &mut FrameWorld,
    tick: Tick,
    owner: AuthorityModelOwner,
    piece: i32,
    amount: i32,
    attacker: Option<sim::ClientId>,
) -> bool {
    let Some(state) = world
        .entity_collision_capabilities_mut()
        .iter_mut()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_mut())
        .and_then(|d| d.extensions.get_mut::<State>())
    else {
        return false;
    };
    let mut breaks = Vec::new();
    if piece < 0 {
        for i in 0..state.health.len() {
            state.damage(i, amount, None, 0, &mut breaks, true);
        }
    } else if (piece as usize) < state.health.len() {
        state.damage(piece as usize, amount, None, 0, &mut breaks, true);
    }
    publish(world, tick, owner, breaks, attacker);
    true
}

use sim::model_behaviors::RadiusDamage;

pub fn apply_radius(world: &mut FrameWorld, tick: Tick, damage: &RadiusDamage) {
    let RadiusDamage {
        origin,
        radius,
        inner,
        outer,
        attacker,
        exclude,
        cone,
    } = *damage;
    if radius <= 0.0 {
        return;
    }
    let mut hits = Vec::new();
    for row in world.entity_collision_capabilities() {
        if row
            .owner
            .script_model()
            .is_some_and(|id| Some(id) == exclude)
        {
            continue;
        }
        let Some(dobj) = &row.dobj else {
            continue;
        };
        let Some(state) = dobj.extensions.get::<State>() else {
            continue;
        };
        let pose = dobj
            .capability
            .as_ref()
            .and_then(|cap| cap.pose(&dobj.pose_request, dobj.world_from_model).ok());
        for (i, piece) in state.definition.pieces.iter().enumerate() {
            let point = dobj
                .capability
                .as_ref()
                .and_then(|cap| {
                    cap.pose
                        .bone_names
                        .iter()
                        .position(|bone| Some(bone) == piece.stages[0].show_bone.as_ref())
                })
                .and_then(|b| pose.as_ref()?.get(b))
                .map_or(dobj.world_from_model.w_axis.truncate(), |matrix| {
                    matrix.w_axis.truncate()
                });
            let dist = point.distance(glam::Vec3::from_array(origin));
            if dist < radius {
                if let Some((forward, cosine)) = cone {
                    let delta = point - glam::Vec3::from_array(origin);
                    if dist > f32::EPSILON
                        && delta.dot(glam::Vec3::from_array(forward)) < dist * cosine
                    {
                        continue;
                    }
                }
                let amount = ((inner + (outer - inner) * dist / radius)
                    * piece.explosive_damage_scale) as i32;
                hits.push((row.owner, i, amount));
            }
        }
    }
    for (owner, piece, amount) in hits {
        apply_piece_hit(world, tick, owner, piece as i32, amount, attacker);
    }
}

pub fn stage_matches(
    world: &FrameWorld,
    owner: AuthorityModelOwner,
    piece: usize,
    stage: usize,
) -> bool {
    world
        .entity_collision_capabilities()
        .iter()
        .find(|r| r.owner == owner)
        .and_then(|r| r.dobj.as_ref())
        .and_then(|d| d.extensions.get::<State>())
        .is_some_and(|s| {
            s.definition
                .pieces
                .get(piece)
                .is_some_and(|p| p.stage(piece, s.health[piece]) == Some(stage))
        })
}

pub fn restart(world: &mut FrameWorld) {
    let models: Vec<_> = world
        .entity_collision_capabilities()
        .iter()
        .filter_map(|row| {
            let state = row.dobj.as_ref()?.extensions.get::<State>()?;
            Some((
                row.owner,
                state.initial_model.clone(),
                state.initial_hide_parts,
            ))
        })
        .collect();
    for (owner, model, hide) in models {
        let capability = world.model_capability(&model).flatten();
        let Some(dobj) = world
            .entity_collision_capabilities_mut()
            .iter_mut()
            .find(|row| row.owner == owner)
            .and_then(|row| row.dobj.as_mut())
        else {
            continue;
        };
        let state = dobj.extensions.get_mut::<State>().unwrap();
        state.health = state
            .definition
            .pieces
            .iter()
            .map(|p| p.health as i16)
            .collect();
        state.timers.clear();
        state.effects.clear();
        dobj.replace_model(&model, capability);
        dobj.semantic_state.hide_part_bits = hide;
        dobj.pose_request.hide_part_bits = hide;
    }
}

pub struct DestructibleBehavior;
impl sim::model_behaviors::ModelBehavior for DestructibleBehavior {
    fn initialize(&self, dobj: &mut AuthorityDObjState, metadata: &dyn std::any::Any) {
        let metadata = metadata
            .downcast_ref::<asset_world::ScriptModelMetadata>()
            .expect("T5 model behavior requires script model metadata");
        if let Some(definition) = &metadata.t5_destructible {
            install(dobj, definition.clone());
        }
    }
    fn apply_hit(
        &self,
        w: &mut FrameWorld,
        t: Tick,
        o: AuthorityModelOwner,
        b: u16,
        a: u32,
        c: Option<sim::ClientId>,
    ) -> bool {
        apply_hit(w, t, o, b, a, c)
    }
    fn apply_piece_hit(
        &self,
        w: &mut FrameWorld,
        t: Tick,
        o: AuthorityModelOwner,
        p: i32,
        a: i32,
        c: Option<sim::ClientId>,
    ) -> bool {
        apply_piece_hit(w, t, o, p, a, c)
    }
    fn apply_radius(&self, w: &mut FrameWorld, t: Tick, d: &RadiusDamage) {
        apply_radius(w, t, d)
    }
    fn stage_matches(&self, w: &FrameWorld, o: AuthorityModelOwner, p: usize, s: usize) -> bool {
        stage_matches(w, o, p, s)
    }
    fn restart(&self, w: &mut FrameWorld) {
        restart(w)
    }
}
