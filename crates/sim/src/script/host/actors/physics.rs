use super::*;
use crate::EntityBodyBounds;
use trace_iw4::Trace;

type V3 = [f32; 3];
const WORLD: u16 = 0x7fe;
const NONE: u16 = 0x7ff;
const PLAYER: u32 = 0x02000000;

#[derive(Clone, Debug)]
pub(crate) struct ActorPhysics {
    pub velocity: V3,
    pub wish_delta: V3,
    pub desired_angles: [f32; 2],
    pub look_angles: [f32; 2],
    pub trace_mask: u32,
    pub ground: u16,
    pub close_player: Option<u64>,
    pub touches: Vec<u16>,
    hit: u16,
    hit_normal: V3,
}
impl Default for ActorPhysics {
    fn default() -> Self {
        Self {
            velocity: [0.; 3],
            wish_delta: [0.; 3],
            desired_angles: [0.; 2],
            look_angles: [0.; 2],
            trace_mask: 0x02820015,
            ground: WORLD,
            close_player: None,
            touches: vec![],
            hit: NONE,
            hit_normal: [0.; 3],
        }
    }
}
impl ActorPhysics {
    pub(super) fn set_angles(&mut self, angles: V3) {
        let angles = [
            math_iw4::angle_normalize_360(angles[0]),
            math_iw4::angle_normalize_360(angles[1]),
        ];
        self.desired_angles = angles;
        self.look_angles = angles;
    }
    pub(super) fn stop(&mut self) {
        self.velocity = [0.; 3];
        self.wish_delta = [0.; 3];
    }
    fn touch(&mut self, number: u16) {
        if number != WORLD
            && number != NONE
            && self.touches.len() < 32
            && !self.touches.contains(&number)
        {
            self.touches.push(number);
        }
    }
    fn slide(
        &mut self,
        origin: &mut V3,
        dt: f32,
        gravity: f32,
        plane: Option<V3>,
        trace: &mut impl FnMut(V3, V3, u32, bool) -> Trace,
    ) -> bool {
        let mut end_velocity = self.velocity;
        if gravity != 0. {
            end_velocity[2] -= gravity * dt;
            self.velocity[2] = (self.velocity[2] + end_velocity[2]) * 0.5;
        }
        let mut planes = Vec::with_capacity(5);
        if let Some(normal) = plane {
            if gravity != 0. {
                self.velocity = clip(self.velocity, normal);
            }
            planes.push(normal);
        }
        if let Some(direction) = normalize(self.velocity) {
            planes.push(direction);
        }
        let mut left = dt;
        for _ in 0..4 {
            let end = add(*origin, scale(self.velocity, left));
            let hit = trace(*origin, end, self.trace_mask, false);
            if hit.allsolid != 0 {
                self.velocity[2] = 0.;
                self.hit = hit_id(&hit);
                return false;
            }
            if hit.fraction > 0. {
                *origin = lerp(*origin, end, hit.fraction);
            }
            if hit.fraction == 1. {
                if gravity != 0. {
                    self.velocity = end_velocity;
                }
                return true;
            }
            self.touch(hit_id(&hit));
            if self.hit == NONE && (hit.normal[0] != 0. || hit.normal[1] != 0.) {
                self.hit = hit_id(&hit);
                self.hit_normal = hit.normal;
            }
            left *= 1. - hit.fraction;
            if planes.iter().any(|&normal| dot(normal, hit.normal) > 0.99) {
                self.velocity = add(self.velocity, hit.normal);
                continue;
            }
            if planes.len() == 5 {
                self.velocity = [0.; 3];
                return true;
            }
            planes.push(hit.normal);
            let mut velocity = self.velocity;
            let mut end = end_velocity;
            for (i, &normal) in planes.iter().enumerate() {
                if dot(velocity, normal) >= 0.1 {
                    continue;
                }
                velocity = clip(velocity, normal);
                end = clip(end, normal);
                for (j, &other) in planes.iter().enumerate() {
                    if i == j || dot(velocity, other) >= 0.1 {
                        continue;
                    }
                    velocity = clip(velocity, other);
                    end = clip(end, other);
                    if dot(velocity, normal) < 0. {
                        let Some(crease) = normalize(cross(normal, other)) else {
                            self.velocity = [0.; 3];
                            return true;
                        };
                        velocity = scale(crease, dot(crease, self.velocity));
                        end = scale(crease, dot(crease, end_velocity));
                    }
                    if planes
                        .iter()
                        .enumerate()
                        .any(|(k, &third)| k != i && k != j && dot(velocity, third) < 0.1)
                    {
                        self.velocity = [0.; 3];
                        return true;
                    }
                }
            }
            self.velocity = velocity;
            end_velocity = end;
        }
        if gravity != 0. {
            self.velocity = end_velocity;
        }
        true
    }

    fn integrate(
        &mut self,
        origin: V3,
        dt: f32,
        trace: &mut impl FnMut(V3, V3, u32, bool) -> Trace,
    ) -> Option<V3> {
        self.touches.clear();
        self.hit = NONE;
        if self.ground == WORLD && self.wish_delta == [0.; 3] {
            self.velocity = [0.; 3];
            return Some(origin);
        }
        let mut pos = origin;
        let top = add(pos, [0., 0., 10.]);
        let depth = if self.velocity[2] <= 0. && self.ground != NONE {
            18.
        } else {
            0.25
        };
        let mut ground = trace(top, add(pos, [0., 0., -depth]), self.trace_mask, true);
        if ground.allsolid != 0 && ground.contents & PLAYER != 0 {
            self.touch(hit_id(&ground));
            self.trace_mask &= !PLAYER;
            ground = trace(top, add(pos, [0., 0., -depth]), self.trace_mask, true);
        }
        let walking = ground.fraction < 1. && ground.allsolid == 0 && ground.normal[2] >= 0.7;
        let plane = (ground.fraction < 1. && ground.allsolid == 0).then_some(ground.normal);
        if walking {
            self.ground = hit_id(&ground);
            self.touch(self.ground);
            let z = 10. + (-depth - 10.) * ground.fraction;
            if z < 0. {
                pos[2] += z;
            }
            let requested = [self.wish_delta[0] / dt, self.wish_delta[1] / dt, 0.];
            self.velocity = clip(requested, ground.normal);
            if dot(self.velocity, requested) > 0.
                && let Some(direction) = normalize(self.velocity)
            {
                self.velocity = scale(direction, dot(requested, requested).sqrt());
            }
            self.velocity[2] = self.velocity[2].min(f32::from_bits(0x4347ffff));
        } else {
            self.ground = NONE;
        }
        let start = pos;
        let velocity = self.velocity;
        if !self.slide(&mut pos, dt, if walking { 0. } else { 800. }, plane, trace) {
            return None;
        }
        if self.hit == NONE {
            return Some(pos);
        }
        let down = (pos, self.clone());
        let up = trace(start, add(start, [0., 0., 10.]), self.trace_mask, false);
        if up.allsolid != 0 || up.fraction == 0. {
            return Some(pos);
        }
        pos = lerp(start, add(start, [0., 0., 10.]), up.fraction);
        self.velocity = velocity;
        self.hit = NONE;
        if !self.slide(&mut pos, dt, if walking { 0. } else { 800. }, None, trace) {
            *self = down.1;
            return Some(down.0);
        }
        let landing = trace(pos, add(pos, [0., 0., -10.]), self.trace_mask, false);
        if landing.allsolid != 0 || (landing.fraction < 1. && landing.normal[2] < 0.3) {
            *self = down.1;
            return Some(down.0);
        }
        pos = lerp(pos, add(pos, [0., 0., -10.]), landing.fraction);
        if landing.fraction < 1. {
            self.velocity = clip(self.velocity, landing.normal);
        }
        Some(pos)
    }
}
fn dot(a: V3, b: V3) -> f32 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn scale(a: V3, s: f32) -> V3 {
    a.map(|v| v * s)
}
fn lerp(a: V3, b: V3, t: f32) -> V3 {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
fn normalize(a: V3) -> Option<V3> {
    let length = dot(a, a).sqrt();
    (length > 0.).then(|| scale(a, 1. / length))
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn clip(v: V3, n: V3) -> V3 {
    if v[2] * v[2] <= v[0] * v[0] + v[1] * v[1] && n[2] >= 0.7 {
        return [v[0] * n[2], v[1] * n[2], -v[0] * n[0] - v[1] * n[1]];
    }
    let back = dot(v, n);
    let back = if back < 0. {
        back * 1.001
    } else {
        back / 1.001
    };
    std::array::from_fn(|i| v[i] - n[i] * back)
}
fn hit_id(hit: &Trace) -> u16 {
    if hit.hit_type == trace_iw4::HITTYPE_ENTITY {
        hit.hit_id
    } else {
        WORLD
    }
}

pub(crate) fn advance(world: &mut World) -> Result<(), String> {
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| {
            (actor.alive && actor.physics_initialized && actor.scripted_control.is_none())
                .then_some(id)
        })
        .collect();
    for id in ids {
        advance_one(world, id)?;
        super::super::presence::settle_collision(world);
    }
    Ok(())
}
fn advance_one(world: &mut World, id: u64) -> Result<(), String> {
    let (origin, presence, bounds, mut physics, team) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let entity = &runtime.entities[&id];
        let presence = entity.presence.ok_or("actor has no physical presence")?;
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        (
            origin,
            presence,
            super::spatial::ACTOR_BOUNDS,
            runtime.actors[&id].motion.clone(),
            runtime.sentients[&id].team,
        )
    };
    let players: Vec<_> = {
        let frame = crate::frame::FrameWorld::from_world(world);
        let mut players = vec![];
        frame.visit_players(|client, ps| {
            if ps.pm_type < 2 {
                players.push((client, *ps));
            }
        });
        players
    };
    let target = if physics.trace_mask & PLAYER == 0 {
        None
    } else {
        let runtime = world.resource::<RoundScript>();
        let mut target = None;
        for (client, ps) in &players {
            let Some(object) = runtime.players.get(&client.0).map(|slot| slot.object) else {
                continue;
            };
            let Some(sentient) = runtime.sentients.get(&object) else {
                continue;
            };
            if [0u8, 12, 10, 6, 0, 0]
                .get(team as usize)
                .copied()
                .unwrap_or(0)
                & (1 << sentient.team)
                != 0
            {
                continue;
            }
            let predicted = add(ps.origin, scale(ps.velocity, 0.1));
            let (lo, hi) = bounds.abs_bounds(origin);
            if (0..3).all(|i| {
                hi[i] >= predicted[i] + [0., 0., 35.][i] - [15., 15., 35.][i]
                    && lo[i] <= predicted[i] + [0., 0., 35.][i] + [15., 15., 35.][i]
            }) {
                target = Some((object, *ps));
                break;
            }
        }
        if target.is_some() && runtime.actors[&id].at_claim_node {
            return Err("player contact claim transfer is not implemented".into());
        }
        target
    };
    physics.close_player = target.map(|(object, _)| object);
    physics.wish_delta = [0.; 3];
    let old = physics.clone();
    if let Some((_, ps)) = target {
        let speed = (ps.velocity[0] * ps.velocity[0] + ps.velocity[1] * ps.velocity[1]).sqrt();
        let direction = if speed > 0. {
            [ps.velocity[0] / speed, ps.velocity[1] / speed, 0.]
        } else {
            let yaw = ps.viewangles[1].to_radians();
            [yaw.cos(), yaw.sin(), 0.]
        };
        let mut side = [direction[1], -direction[0], 0.];
        if dot(
            side,
            [origin[0] - ps.origin[0], origin[1] - ps.origin[1], 0.],
        ) < 0.
        {
            side = scale(side, -1.);
        }
        physics.wish_delta = scale(side, speed.max(60.) * 0.05);
    }
    let neighbors: Vec<_> = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let others: Vec<_> = runtime
            .actors
            .iter()
            .filter_map(|(&other, actor)| {
                (other != id && actor.alive && actor.physics_initialized).then_some(other)
            })
            .collect();
        others
            .into_iter()
            .filter_map(|other| match runtime.object_field(other, "origin") {
                Value::Vector(origin) => Some(origin),
                _ => None,
            })
            .collect()
    };
    let mut trace = |start: V3, end: V3, mask: u32, ground: bool| {
        let frame = crate::frame::FrameWorld::from_world(world);
        let mut bounds = bounds;
        if ground {
            bounds.midpoint[2] -= 5.;
            bounds.half_size[2] -= 5.;
        }
        let (mins, maxs) = bounds.abs_bounds([0.; 3]);
        let input = movement_iw4::GroundTraceInput {
            start,
            end,
            mins,
            maxs,
            tracemask: mask,
        };
        let mut hit = frame.trace_world_hull_except(
            input,
            Some(crate::AuthorityModelOwner::ScriptModel(presence)),
        );
        if mask & PLAYER != 0 {
            for (client, ps) in &players {
                let body = crate::entity_body::EntityBodyPose {
                    bounds: EntityBodyBounds {
                        midpoint: [0., 0., 35.],
                        half_size: [15., 15., 35.],
                        contents: PLAYER,
                    },
                    origin: ps.origin,
                    number: client.0 as u16,
                };
                let mut other = body.trace(input);
                if other.fraction < hit.fraction {
                    other.hit_type = trace_iw4::HITTYPE_ENTITY;
                    other.hit_id = client.0 as u16;
                    let startsolid = hit.startsolid | other.startsolid;
                    let allsolid = hit.allsolid | other.allsolid;
                    hit = other;
                    hit.startsolid = startsolid;
                    hit.allsolid = allsolid;
                }
            }
        }
        hit
    };
    let mut position = origin;
    let initial_delta = physics.wish_delta;
    let mut magnitude = dot(initial_delta, initial_delta).sqrt();
    let mut minimum = magnitude * magnitude * 0.25;
    for attempt in 0..3 {
        let candidate = physics.integrate(origin, 0.05, &mut trace);
        let accepted = candidate.is_some_and(|candidate| {
            if let Some((_, ps)) = target {
                let distance =
                    |p: V3| (p[0] - ps.origin[0]).powi(2) + (p[1] - ps.origin[1]).powi(2);
                distance(candidate) > distance(origin)
                    && (candidate[0] - origin[0]).powi(2) + (candidate[1] - origin[1]).powi(2)
                        > minimum
                    && neighbors.iter().all(|other| {
                        let old = (origin[0] - other[0]).powi(2) + (origin[1] - other[1]).powi(2);
                        let new =
                            (candidate[0] - other[0]).powi(2) + (candidate[1] - other[1]).powi(2);
                        !(new < 900. && old >= 900. && (origin[2] - other[2]).abs() < 80.)
                    })
            } else {
                true
            }
        });
        if accepted {
            position = candidate.unwrap();
            break;
        }
        let hit = physics.hit;
        let normal = normalize([physics.hit_normal[0], physics.hit_normal[1], 0.]);
        physics = old.clone();
        if target.is_none() || hit == NONE || attempt == 2 {
            break;
        }
        let Some(normal) = normal else { break };
        magnitude *= 0.75;
        minimum *= 0.5625;
        let toward_left = normal[1] * initial_delta[0] <= initial_delta[1] * normal[0];
        let side = if toward_left {
            [-normal[1], normal[0], 0.]
        } else {
            [normal[1], -normal[0], 0.]
        };
        physics.wish_delta = scale(side, magnitude);
    }
    physics.trace_mask = old.trace_mask;
    let mut runtime = world.resource_mut::<RoundScript>();
    runtime.actors.get_mut(&id).unwrap().motion = physics;
    runtime.set_object_field(id, "origin", Value::Vector(position));
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "pushplayer", |world, receiver, args| {
        let id = actor_id(world, receiver)?;
        let enabled = super::super::args::int(args, 0)? != 0;
        let mut runtime = world.resource_mut::<RoundScript>();
        let physics = &mut runtime.actors.get_mut(&id).unwrap().motion;
        if enabled {
            physics.trace_mask &= !PLAYER;
        } else {
            physics.trace_mask |= PLAYER;
        }
        Ok(Value::Undefined)
    });
}
