use super::*;
use crate::EntityBodyBounds;
use trace_iw4::Trace;

type V3 = [f32; 3];
const WORLD: u16 = 0x7fe;
const NONE: u16 = 0x7ff;
const PLAYER: u32 = 0x02000000;
pub(crate) const LINK_MASK: u32 = 0x0282_0015;

pub(crate) fn slide_link(
    mut origin: V3,
    target: V3,
    trace: &mut impl FnMut(V3, V3, u32, bool) -> Trace,
) -> V3 {
    let seconds = crate::MATCH_TICK_MS as f32 * 0.001;
    let mut motion = ActorPhysics {
        velocity: std::array::from_fn(|i| (target[i] - origin[i]) / seconds),
        trace_mask: LINK_MASK,
        ..Default::default()
    };
    motion.slide(&mut origin, seconds, 0., None, false, trace);
    origin
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlideResult {
    Clear,
    Blocked,
    Stuck,
}

#[derive(Clone, Debug)]
pub(crate) struct ActorPhysics {
    pub velocity: V3,
    pub wish_delta: V3,
    pub desired_angles: [f32; 2],
    pub look_angles: [f32; 2],
    pub trace_mask: u32,
    pub ground: u16,
    pub(super) fall_velocity: f32,
    pub stairs_state: u8,
    pub close_player: Option<u64>,
    pub pile_up_actor: Option<u64>,
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
            fall_velocity: 0.,
            stairs_state: 0,
            close_player: None,
            pile_up_actor: None,
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
    pub(crate) fn stop(&mut self) {
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
        z_only: bool,
        trace: &mut impl FnMut(V3, V3, u32, bool) -> Trace,
    ) -> SlideResult {
        let mut end_velocity = self.velocity;
        if gravity != 0. {
            end_velocity[2] -= gravity * dt;
            self.velocity[2] = (self.velocity[2] + end_velocity[2]) * 0.5;
        }
        let mut planes = Vec::with_capacity(5);
        if let Some(normal) = plane {
            if gravity != 0. && !z_only {
                self.velocity = clip(self.velocity, normal);
            }
            planes.push(normal);
        }
        if let Some(direction) = normalize(self.velocity) {
            planes.push(direction);
        }
        let mut left = dt;
        for bump in 0..4 {
            let end = add(*origin, scale(self.velocity, left));
            let hit = trace(*origin, end, self.trace_mask, false);
            if hit.allsolid != 0 {
                self.velocity[2] = 0.;
                self.hit = hit_id(&hit);
                return SlideResult::Stuck;
            }
            if hit.fraction > 0. {
                *origin = lerp(*origin, end, hit.fraction);
            }
            if hit.fraction == 1. {
                if gravity != 0. {
                    self.velocity = end_velocity;
                }
                return if bump == 0 {
                    SlideResult::Clear
                } else {
                    SlideResult::Blocked
                };
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
                return SlideResult::Blocked;
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
                            return SlideResult::Blocked;
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
                        return SlideResult::Blocked;
                    }
                }
            }
            self.velocity = velocity;
            end_velocity = end;
        }
        if gravity != 0. {
            self.velocity = end_velocity;
        }
        SlideResult::Blocked
    }

    fn integrate_root(
        &mut self,
        origin: V3,
        dt: f32,
        mode: anim_mode::Mode,
        trace: &mut impl FnMut(V3, V3, u32, bool) -> Trace,
    ) -> Option<V3> {
        self.touches.clear();
        self.hit = NONE;
        self.ground = NONE;
        self.stairs_state = 0;
        self.velocity = scale(self.wish_delta, 1. / dt);
        if mode == anim_mode::Mode::NoClip {
            self.fall_velocity = 0.;
            return Some(add(origin, self.wish_delta));
        }
        let gravity = if mode == anim_mode::Mode::NoGravity {
            self.fall_velocity = 0.;
            0.
        } else {
            self.velocity[2] += self.fall_velocity;
            800.
        };
        let mut position = origin;
        if self.slide(&mut position, dt, gravity, None, false, trace) == SlideResult::Stuck {
            self.fall_velocity = 0.;
            return None;
        }
        self.fall_velocity -= gravity * dt;
        if gravity != 0. && self.velocity[2] <= 0. {
            let floor = trace(
                position,
                add(position, [0., 0., -0.25]),
                self.trace_mask,
                true,
            );
            if floor.fraction < 1. && floor.startsolid == 0 && floor.normal[2] >= 0.7 {
                self.ground = hit_id(&floor);
                self.touch(self.ground);
                self.fall_velocity = 0.;
            }
        }
        Some(position)
    }

    fn integrate(
        &mut self,
        origin: V3,
        dt: f32,
        z_only: bool,
        alive: bool,
        trace: &mut impl FnMut(V3, V3, u32, bool) -> Trace,
    ) -> Option<V3> {
        self.touches.clear();
        self.hit = NONE;
        if self.ground == WORLD && self.wish_delta == [0.; 3] {
            self.velocity = [0.; 3];
            return Some(origin);
        }
        let step_height = if !alive {
            4.
        } else if self.stairs_state == 0 {
            10.
        } else {
            25.
        };
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
        let along = self.velocity[0] * ground.normal[0] + self.velocity[1] * ground.normal[1];
        self.stairs_state = if ground.surface_flags & 0x200 != 0 && ground.normal[2] >= 0.0 {
            if along < -0.001 {
                1
            } else if (along > 0.001 || self.stairs_state == 0) && walking {
                2
            } else {
                0
            }
        } else {
            0
        };
        let plane = (ground.fraction < 1. && ground.allsolid == 0).then_some(ground.normal);
        if walking {
            self.ground = hit_id(&ground);
            self.touch(self.ground);
            let z = 10. + (-depth - 10.) * ground.fraction;
            if z < 0. {
                pos[2] += z;
            }
        }
        if z_only {
            self.velocity = [self.wish_delta[0] / dt, self.wish_delta[1] / dt, 0.];
        } else if walking {
            let requested = [self.wish_delta[0] / dt, self.wish_delta[1] / dt, 0.];
            self.velocity = clip(requested, ground.normal);
            if dot(self.velocity, requested) > 0.
                && let Some(direction) = normalize(self.velocity)
            {
                self.velocity = scale(direction, dot(requested, requested).sqrt());
            }
            self.velocity[2] = self.velocity[2].min(f32::from_bits(0x4347ffff));
        }
        if !walking {
            self.ground = NONE;
        }
        let start = pos;
        let velocity = self.velocity;
        let gravity = if z_only || !walking { 800. } else { 0. };
        let result = self.slide(&mut pos, dt, gravity, plane, z_only, trace);
        if result == SlideResult::Stuck {
            return None;
        }
        if result == SlideResult::Clear {
            if self.velocity[2] > 0. || !walking {
                let down = add(pos, [0., 0., -step_height]);
                let landing = trace(pos, down, self.trace_mask, false);
                if self.velocity[2] > 0. {
                    pos = lerp(pos, down, landing.fraction);
                }
                if start[2] - pos[2] > 1. && ground.walkable == 0 && landing.walkable == 0 {
                    self.hit = hit_id(&ground);
                }
            }
            return Some(pos);
        }
        if self.velocity[2] > 0. && !walking {
            let landing = trace(
                start,
                add(start, [0., 0., -step_height]),
                self.trace_mask,
                false,
            );
            if landing.fraction == 1. || landing.walkable == 0 {
                self.hit = NONE;
                return Some(pos);
            }
        }
        let up = trace(
            start,
            add(start, [0., 0., step_height]),
            self.trace_mask,
            false,
        );
        if up.allsolid != 0 {
            return (up.fraction != 0.).then_some(pos);
        }
        let down = (pos, self.clone());
        pos = lerp(start, add(start, [0., 0., step_height]), up.fraction);
        let raised_z = pos[2];
        self.velocity = velocity;
        self.hit = NONE;
        if up.fraction < 1. && self.velocity[2] > 0. {
            self.velocity[2] = 0.;
        }
        if self.slide(&mut pos, dt, gravity, None, z_only, trace) == SlideResult::Stuck {
            *self = down.1;
            return None;
        }
        let end = [pos[0], pos[1], start[2] - raised_z.max(pos[2]) + pos[2]];
        let landing = trace(pos, end, self.trace_mask, false);
        if landing.startsolid == 0 {
            pos = lerp(pos, end, landing.fraction);
        }
        if landing.fraction < 1. {
            if landing.normal[2] < 0.3 {
                *self = down.1;
                return None;
            }
            self.velocity = clip(self.velocity, landing.normal);
            if start[2].max(down.0[2]) + 0.1 < pos[2] {
                self.hit = NONE;
            }
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
    let _span = perf::span_handle!("actor_physics", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| {
            ((actor.alive || actor.death.is_some())
                && world.resource::<RoundScript>().entities[&id]
                    .linked_to
                    .is_none()
                && actor.physics_initialized
                && !actor.traversal.pending()
                && (actor.scripted_control.is_none()
                    || actor.anim_script_kind == Some(scripted::ControlKind::Custom)))
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
    let content = crate::frame::FrameWorld::from_world(world).content();
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
            let Some(pose) = content.simulation_pose_in(ps.origin, origin) else {
                continue;
            };
            let player_origin = pose.w_axis.truncate().to_array();
            let velocity = pose.transform_vector3(ps.velocity.into()).to_array();
            let predicted = add(player_origin, scale(velocity, 0.1));
            let center = add(
                predicted,
                pose.transform_vector3(glam::Vec3::new(0., 0., 35.))
                    .to_array(),
            );
            let half = (pose.x_axis.truncate().abs() * 15.
                + pose.y_axis.truncate().abs() * 15.
                + pose.z_axis.truncate().abs() * 35.)
                .to_array();
            if !center
                .into_iter()
                .chain(half)
                .chain(velocity)
                .all(f32::is_finite)
            {
                continue;
            }
            let (lo, hi) = bounds.abs_bounds(origin);
            if (0..3).all(|i| hi[i] >= center[i] - half[i] && lo[i] <= center[i] + half[i]) {
                let yaw = ps.viewangles[1].to_radians();
                let forward = pose
                    .transform_vector3(glam::Vec3::new(yaw.cos(), yaw.sin(), 0.))
                    .to_array();
                target = Some((object, player_origin, velocity, forward));
                break;
            }
        }
        if target.is_some() && runtime.actors[&id].at_claim_node {
            return Err("player contact claim transfer is not implemented".into());
        }
        target
    };
    physics.close_player = target.map(|(object, ..)| object);
    let animation_mode = super::anim_mode::effective(&world.resource::<RoundScript>().actors[&id]);
    let root_physics = world.resource::<RoundScript>().actors[&id]
        .traversal
        .active()
        || matches!(
            animation_mode,
            anim_mode::Mode::NoGravity | anim_mode::Mode::NoClip
        );
    physics.wish_delta = if animation_mode == anim_mode::Mode::PointRelative {
        [0.; 3]
    } else if animation_mode.uses_root_motion() {
        let delta = super::anim_mode::delta(world, id)?.translation;
        let yaw = {
            let Value::Vector(angles) = world
                .resource_mut::<RoundScript>()
                .object_field(id, "angles")
            else {
                return Err("actor angles are not a vector".into());
            };
            angles[1].to_radians()
        };
        [
            yaw.cos() * delta[0] - yaw.sin() * delta[1],
            yaw.sin() * delta[0] + yaw.cos() * delta[1],
            if root_physics { delta[2] } else { 0. },
        ]
    } else {
        super::navigation::wish_delta(world, id)
    };
    physics.wish_delta = super::arrival::motion(
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap(),
        physics.wish_delta,
        crate::MATCH_TICK_MS as f32 / 1000.,
    );
    physics.wish_delta = super::traversal::motion(
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap(),
        physics.wish_delta,
        crate::MATCH_TICK_MS as f32 / 1000.,
    );
    let old = physics.clone();
    if let Some((_, player_origin, velocity, forward)) = target.filter(|_| !root_physics) {
        let speed = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
        let direction = if speed > 0. {
            [velocity[0] / speed, velocity[1] / speed, 0.]
        } else {
            normalize([forward[0], forward[1], 0.]).unwrap_or([1., 0., 0.])
        };
        let mut side = [direction[1], -direction[0], 0.];
        if dot(
            side,
            [
                origin[0] - player_origin[0],
                origin[1] - player_origin[1],
                0.,
            ],
        ) < 0.
        {
            side = scale(side, -1.);
        }
        physics.wish_delta = add(physics.wish_delta, scale(side, speed.max(60.) * 0.05));
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
                Value::Vector(other_origin) => content
                    .simulation_pose_in(other_origin, origin)
                    .map(|pose| pose.w_axis.truncate().to_array()),
                _ => None,
            })
            .collect()
    };
    let actor = &world.resource::<RoundScript>().actors[&id];
    let alive = actor.alive;
    let z_only = matches!(
        super::anim_mode::effective(actor),
        super::anim_mode::Mode::ZOnly | super::anim_mode::Mode::PointRelative
    );
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
                hit = body.clip(&content, hit, input);
            }
        }
        hit
    };
    let mut position = origin;
    let initial_delta = physics.wish_delta;
    let mut magnitude = dot(initial_delta, initial_delta).sqrt();
    let mut minimum = magnitude * magnitude * 0.25;
    for attempt in 0..3 {
        let candidate = if root_physics {
            physics.integrate_root(origin, 0.05, animation_mode, &mut trace)
        } else {
            physics.fall_velocity = 0.;
            physics.integrate(origin, 0.05, z_only, alive, &mut trace)
        };
        let accepted = candidate.is_some_and(|candidate| {
            if let Some((_, player_origin, ..)) = target.filter(|_| !root_physics) {
                let distance =
                    |p: V3| (p[0] - player_origin[0]).powi(2) + (p[1] - player_origin[1]).powi(2);
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
    let team_blocked = {
        let runtime = world.resource::<RoundScript>();
        runtime.entities.iter().any(|(other, entity)| {
            *other != id
                && entity.number == i32::from(physics.hit)
                && runtime
                    .sentients
                    .get(other)
                    .zip(runtime.sentients.get(&id))
                    .is_some_and(|(other, me)| other.team == me.team)
        })
    };
    let now = super::super::players::now_ms(world) as i32;
    let mut runtime = world.resource_mut::<RoundScript>();
    let actor = runtime.actors.get_mut(&id).unwrap();
    super::arrival::accepted(actor, origin, position);
    super::traversal::accepted(actor, origin, position);
    if team_blocked {
        // Give a friendly blocker two simulation steps to clear before
        // exposing a new combat route. This is a short movement arbitration wait.
        actor.team_move_wait_until = now.saturating_add(2 * crate::MATCH_TICK_MS as i32);
    }
    actor.motion = physics;
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
