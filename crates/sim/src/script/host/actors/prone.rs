use super::*;
use xmodel_runtime::XAnimNodeId;

mod ground;

#[derive(Clone, Debug)]
pub(crate) struct ProneNodes {
    pub inverse_pitch: [f32; 2],
    pub nodes: [XAnimNodeId; 3],
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Prone {
    pub active: bool,
    pub started_at: i32,
    pub transition_ms: i32,
    pub feet_forward: bool,
    pub valid: bool,
    pub pitch: [f32; 2],
    last_pitch: f32,
    sampled_pose: Option<([f32; 3], [f32; 3])>,
}

impl Prone {
    fn is_prone(&mut self, now: i32) -> bool {
        if !self.active {
            return false;
        }
        if self.transition_ms < 0 {
            if self.started_at.wrapping_sub(self.transition_ms) < now {
                self.active = false;
                return false;
            }
        } else if self.transition_ms > 0 && self.started_at.wrapping_add(self.transition_ms) < now {
            self.transition_ms = 0;
        }
        true
    }

    fn fraction(&mut self, now: i32) -> f32 {
        if !self.is_prone(now) {
            return 0.;
        }
        match self.transition_ms {
            0 => 1.,
            ms if ms > 0 => now.wrapping_sub(self.started_at) as f32 / ms as f32,
            ms => 1. - now.wrapping_sub(self.started_at) as f32 / ms.wrapping_neg() as f32,
        }
    }

    fn enter(&mut self, now: i32, ms: i32, feet_forward: bool) -> bool {
        if !self.is_prone(now) {
            self.active = true;
            self.started_at = now;
            self.transition_ms = ms;
            self.feet_forward = feet_forward;
            return true;
        }
        if self.transition_ms != 0 && ms != self.transition_ms {
            let fraction = self.fraction(now);
            if fraction < 1. {
                self.started_at = now.wrapping_sub((ms as f32 * fraction) as i32);
            }
            self.transition_ms = ms;
            self.feet_forward = feet_forward;
        }
        false
    }

    fn exit(&mut self, now: i32, ms: i32) {
        if !self.is_prone(now) {
            return;
        }
        self.started_at = if self.transition_ms == 0 || ms == self.transition_ms {
            now
        } else {
            now.wrapping_sub((ms as f32 * self.fraction(now)) as i32)
        };
        self.transition_ms = ms.wrapping_neg();
    }
}

fn now(world: &World) -> i32 {
    crate::level_time_ms(world.resource::<crate::step::StepRequest>().tick)
}

fn check_ground(world: &mut World, id: u64) -> Result<(), String> {
    let (origin, angles, feet_forward) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let Value::Vector(angles) = runtime.object_field(id, "angles") else {
            return Err("actor angles are not a vector".into());
        };
        (origin, angles, runtime.actors[&id].prone.feet_forward)
    };
    let result = ground::check(
        origin,
        angles[1],
        feet_forward,
        &mut |start, end, midpoint, half| {
            super::super::trace::capsule(
                world,
                movement_iw4::GroundTraceInput {
                    start,
                    end,
                    mins: std::array::from_fn(|i| midpoint[i] - half[i]),
                    maxs: std::array::from_fn(|i| midpoint[i] + half[i]),
                    tracemask: 0x820015,
                },
                &[id],
            )
        },
    );
    let mut runtime = world.resource_mut::<RoundScript>();
    let prone = &mut runtime.actors.get_mut(&id).unwrap().prone;
    prone.valid = result.valid;
    prone.sampled_pose = Some((origin, angles));
    if let Some(pitch) = result.pitch {
        prone.pitch = pitch;
    }
    Ok(())
}

fn update_weights(world: &mut World, id: u64, force: bool) -> Result<(), String> {
    let mut runtime = world.resource_mut::<RoundScript>();
    let actor = runtime.actors.get_mut(&id).unwrap();
    let Some(nodes) = actor.prone_nodes.clone() else {
        return Err("prone animation nodes have not been set".into());
    };
    let pitch = math_iw4::angle_subtract(actor.prone.pitch[1], actor.prone.pitch[0]);
    if !force && actor.prone.last_pitch == pitch {
        return Ok(());
    }
    let (down, up) = if pitch < 0. {
        ((nodes.inverse_pitch[0] * pitch).min(0.99), 0.)
    } else {
        (0., (nodes.inverse_pitch[1] * pitch).min(0.99))
    };
    let animator = runtime
        .animators
        .get_mut(&id)
        .ok_or("actor has no animation tree")?;
    for (node, weight) in nodes.nodes.into_iter().zip([down, 1. - down - up, up]) {
        animator.goal(node, weight, 0.05, 1., false)?;
    }
    runtime.actors.get_mut(&id).unwrap().prone.last_pitch = pitch;
    Ok(())
}

fn is_ragdoll(world: &mut World, id: u64) -> bool {
    super::super::entities::is_ragdoll(world, id)
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_prone", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter(|(_, actor)| actor.prone.active)
        .map(|(&id, _)| id)
        .collect();
    for id in ids {
        if is_ragdoll(world, id) {
            continue;
        }
        let active = world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .prone
            .is_prone(now);
        let root = if active {
            let moved = {
                let mut runtime = world.resource_mut::<RoundScript>();
                let origin = runtime.object_field(id, "origin");
                let angles = runtime.object_field(id, "angles");
                match (origin, angles) {
                    (Value::Vector(origin), Value::Vector(angles)) => {
                        runtime.actors[&id].prone.sampled_pose != Some((origin, angles))
                    }
                    _ => return Err("actor placement is not a vector".into()),
                }
            };
            if moved {
                check_ground(world, id)?;
                update_weights(world, id, false)?;
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            let prone = &mut runtime.actors.get_mut(&id).unwrap().prone;
            let pitch = prone.pitch[0] * prone.fraction(now);
            Some(xmodel_runtime::RootBonePose {
                rotation: xmodel_runtime::set_angles([pitch, 0., 0.]),
                translation: [0.; 3],
            })
        } else {
            None
        };
        let presence = world.resource::<RoundScript>().entities[&id].presence;
        if let Some(presence) = presence
            && let Some(dobj) = crate::frame::FrameWorld::from_world(world)
                .collision_owner_mut(presence)
                .and_then(|row| row.dobj.as_mut())
        {
            dobj.set_root_override(root);
        }
    }
    Ok(())
}

fn clear_pitch(world: &mut World, id: u64) {
    let presence = {
        let mut r = world.resource_mut::<RoundScript>();
        let actor = r.actors.get_mut(&id).unwrap();
        actor.prone.active = false;
        if let Some(dog) = actor
            .initial_state
            .as_mut()
            .and_then(|s| s.dog_prone.as_mut())
        {
            dog.orient_pitch = false;
            dog.prone = false;
        }
        r.entities[&id].presence
    };
    if let Some(presence) = presence
        && let Some(dobj) = crate::frame::FrameWorld::from_world(world)
            .collision_owner_mut(presence)
            .and_then(|r| r.dobj.as_mut())
    {
        dobj.set_root_override(None);
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(
        Namespace::Method,
        "clearpitchorient",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            clear_pitch(world, id);
            Ok(Value::Undefined)
        },
    );
    registry.register(Namespace::Method, "enterprone", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let ms = (float(args, 0)? * 1000.) as i32;
        let feet_forward = if args.len() > 1 {
            super::super::args::int(args, 1)? == 0
        } else {
            true
        };
        let now = now(world);
        let entered = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let actor = runtime.actors.get_mut(&id).unwrap();
            let nodes = actor
                .prone_nodes
                .as_ref()
                .ok_or("set prone animation nodes before entering prone")?;
            if nodes.nodes.iter().any(|node| node.0 == 0) || nodes.inverse_pitch == [0.; 2] {
                return Err("set prone animation nodes before entering prone".into());
            }
            actor.prone.enter(now, ms, feet_forward)
        };
        if entered {
            check_ground(world, id)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Method, "exitprone", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let ms = (float(args, 0)? * 1000.) as i32;
        let now = now(world);
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .prone
            .exit(now, ms);
        Ok(Value::Undefined)
    });
    registry.register(Namespace::Method, "updateprone", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let now = now(world);
        if !world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .prone
            .is_prone(now)
            || is_ragdoll(world, id)
        {
            return Ok(Value::Undefined);
        }
        let weight = float(args, 2)?;
        let blend = float(args, 3)?;
        let rate = float(args, 4)?;
        if [weight, blend, rate]
            .iter()
            .any(|value| !value.is_finite() || *value < 0.)
        {
            return Err(
                "animation weight, blend time and rate must be finite and nonnegative".into(),
            );
        }
        {
            let mut runtime = world.resource_mut::<RoundScript>();
            let animator = runtime
                .animators
                .get_mut(&id)
                .ok_or("actor has no animation tree")?;
            let nodes = [animator.node(arg(args, 0)?)?, animator.node(arg(args, 1)?)?];
            for node in nodes {
                animator.goal(node, weight, blend, rate, true)?;
            }
        }
        update_weights(world, id, true)?;
        Ok(Value::Undefined)
    });
    registry.register(
        Namespace::Method,
        "setproneanimnodes",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            let down = float(args, 0)?;
            let up = float(args, 1)?;
            if down >= 0.0 {
                return Err("prone down pitch must be negative".into());
            }
            if up <= 0.0 {
                return Err("prone up pitch must be positive".into());
            }
            let mut runtime = world.resource_mut::<RoundScript>();
            let animator = runtime
                .animators
                .get(&id)
                .ok_or("actor has no animation tree")?;
            let nodes = [
                animator.node(arg(args, 2)?)?,
                animator.node(arg(args, 3)?)?,
                animator.node(arg(args, 4)?)?,
            ];
            runtime.actors.get_mut(&id).unwrap().prone_nodes = Some(ProneNodes {
                inverse_pitch: [down.recip(), up.recip()],
                nodes,
            });
            Ok(Value::Undefined)
        },
    );
}
