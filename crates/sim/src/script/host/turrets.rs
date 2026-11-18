use super::args::{arg, float, int, optional, string, vector};
use super::entities::EntityKind;
use super::natives::engine::{TraceIgnore, collider_entity, entity_id, entity_trace, keyed_array};
use crate::bullet_collision::{ColliderId, MASK_SHOT, TraceOutcome};
use crate::frame::FrameWorld;
use crate::script::Namespace::{Function, Method};
use crate::script::runtime::raise;
use crate::script::{Arc, NativeRegistry, RoundScript, Value};
use crate::world::ClientId;
use bevy_ecs::prelude::World;
use glam::Vec3;

mod operator;
pub(super) use operator::toggle;
mod spawn;
mod spin;
pub(crate) use spawn::initialize_map;
const MAX_TURRETS: usize = 32;

const TURN_RATE_DEG_PER_S: f32 = 360.0;
const SCAN_RATE_DEG_PER_S: f32 = 30.0;
const ON_TARGET_DEGREES: f32 = 2.0;
const DEFAULT_RANGE: f32 = 4096.0;
const MUZZLE_HEIGHT: f32 = 40.0;
const TARGET_HEIGHT: f32 = 40.0;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Turret {
    pub(crate) weapon: u32,
    mode: Arc<str>,
    pub(super) owner: Option<u64>,
    pub(crate) active_user: Option<u64>,
    pub(crate) ignore_goals: bool,
    pub(crate) operable: bool,
    pub(super) can_ai_detach: bool,
    pub(super) drop_hint: i32,
    player_position_dist: f32,
    pub(super) aim_column_degrees: f32,
    team: Option<Arc<str>>,
    pub(super) carried: bool,
    fire_enabled: bool,
    manual_firing: bool,
    manual: Option<(u64, [f32; 3])>,
    target: Option<u64>,
    firing: bool,
    on_target: bool,
    aim: [f32; 3],
    aim_initialized: bool,
    scan_direction: f32,
    arcs: [f32; 4],
    drop_pitch: f32,
    convergence: [f32; 2],
    suppression_seconds: f32,
    ai_spread_degrees: f32,
    player_spread_degrees: f32,
    max_range: f32,
    acquired_at: i32,
    last_seen_at: i32,
    last_seen_point: Option<[f32; 3]>,
    next_fire_at: i32,
    minimap: bool,
    minigun: bool,
    auto_rotation_stop_delay: i32,
    mode_change_wait: f32,
    spin: spin::Spin,
}

impl Turret {
    fn new(weapon: u32, settings: weapon_iw4::TurretSettings) -> Self {
        Self {
            weapon,
            mode: "manual".into(),
            owner: None,
            active_user: None,
            ignore_goals: false,
            operable: true,
            can_ai_detach: true,
            drop_hint: -1,
            player_position_dist: settings.player_position_dist,
            aim_column_degrees: settings.aim_column_degrees,
            team: None,
            carried: false,
            fire_enabled: true,
            manual_firing: false,
            manual: None,
            target: None,
            firing: false,
            on_target: false,
            aim: [1., 0., 0.],
            aim_initialized: false,
            scan_direction: 1.,
            arcs: settings.arcs.map(|value| value.clamp(0., 180.)),
            drop_pitch: -90.,
            convergence: settings.convergence_seconds.map(|value| value.max(0.)),
            suppression_seconds: settings.suppression_seconds.max(0.),
            ai_spread_degrees: settings.ai_spread_degrees.max(0.),
            player_spread_degrees: settings.player_spread_degrees.max(0.),
            max_range: if settings.max_range > 0. {
                settings.max_range
            } else {
                f32::MAX
            },
            acquired_at: 0,
            last_seen_at: 0,
            last_seen_point: None,
            next_fire_at: 0,
            minimap: false,
            minigun: false,
            auto_rotation_stop_delay: 1000,
            mode_change_wait: 0.,
            spin: spin::Spin::new(settings.barrel_spin),
        }
    }
    pub(super) fn model_rotations(&self, base: [f32; 3]) -> Vec<xmodel_runtime::TagRotation> {
        let mut rotations = Vec::new();
        if self.aim_initialized && !self.carried && &*self.mode != "sentry_offline" {
            let axis = math_iw4::angles_to_axis(base);
            let aim = Vec3::from_array(self.aim);
            let local = axis.map(|axis| aim.dot(Vec3::from_array(axis)));
            rotations.push(xmodel_runtime::TagRotation {
                tag: "tag_aim".into(),
                angles: math_iw4::vect_to_angles(local),
            });
        }
        rotations.extend(self.spin.rotation());
        rotations
    }
}

pub(super) fn turret_of(world: &World, receiver: &Value) -> Result<u64, String> {
    let runtime = world.resource::<RoundScript>();
    match runtime.entity(receiver) {
        Some((object, _)) if runtime.engine.turrets.contains_key(&object) => Ok(object),
        _ => Err("receiver is not a turret".into()),
    }
}

fn edit(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut Turret),
) -> Result<Value, String> {
    let object = turret_of(world, receiver)?;
    change(
        world
            .resource_mut::<RoundScript>()
            .engine
            .turrets
            .get_mut(&object)
            .unwrap(),
    );
    Ok(Value::Undefined)
}

fn field_vector(world: &mut World, object: u64, name: &str) -> [f32; 3] {
    match super::players::entity_field(world, object, name) {
        Value::Vector(v) => v,
        _ => [0.0; 3],
    }
}

pub(crate) fn muzzle(world: &mut World, object: u64) -> [f32; 3] {
    if let Some((origin, _)) = super::presence::tag_world(world, object, "tag_flash") {
        return origin;
    }
    let origin = field_vector(world, object, "origin");
    [origin[0], origin[1], origin[2] + MUZZLE_HEIGHT]
}

pub(crate) fn ignore_self(world: &World, object: u64) -> TraceIgnore {
    TraceIgnore {
        model: world
            .resource::<RoundScript>()
            .entities
            .get(&object)
            .and_then(|e| e.presence),
        ..Default::default()
    }
}

fn weapon_range(world: &mut World, weapon: u32) -> f32 {
    FrameWorld::from_world(world)
        .combat_facts_for(weapon)
        .map(|f| f.bullet_range())
        .filter(|r| *r > 0.0)
        .unwrap_or(DEFAULT_RANGE)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    spin::register(registry);
    registry.register(Function, "spawnturret", spawn::dynamic);
    registry.register(Function, "canspawnturret", |world, _, _| {
        Ok(Value::Int(i32::from(
            world.resource::<RoundScript>().engine.turrets.len() < MAX_TURRETS,
        )))
    });

    registry.register(Method, "setmode", |world, receiver, args| {
        let mode: Arc<str> = string(args, 0)?.into();
        if !matches!(
            &*mode,
            "auto_ai" | "manual" | "manual_ai" | "auto_nonai" | "sentry" | "sentry_offline"
        ) {
            return Err(format!("unknown turret mode '{mode}'"));
        }
        edit(world, receiver, |t| t.mode = mode)
    });
    registry.register(Method, "getmode", |world, receiver, _| {
        let object = turret_of(world, receiver)?;
        Ok(Value::String(
            world.resource::<RoundScript>().engine.turrets[&object]
                .mode
                .clone()
                .into(),
        ))
    });
    fn set_target(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
        let target = entity_id(world, arg(args, 0)?)?;
        let offset = optional(args, 1, vector)?.unwrap_or([0.0; 3]);
        if super::vehicles::is_heli(world, receiver) {
            let aim = super::vehicles::TurretAim::Entity(target, offset);
            return super::vehicles::aim_turret(world, receiver, Some(aim));
        }
        edit(world, receiver, |t| t.manual = Some((target, offset)))
    }
    registry.register(Method, "setturretignoregoals", |world, receiver, args| {
        let ignore = int(args, 0)? != 0;
        edit(world, receiver, |t| t.ignore_goals = ignore)
    });
    registry.register(Method, "setturretcanaidetach", |world, receiver, args| {
        let allowed = int(args, 0)? != 0;
        edit(world, receiver, |t| t.can_ai_detach = allowed)
    });
    registry.register(Function, "isturretactive", |world, _, args| {
        if args.len() != 1 {
            return Err("IsTurretActive requires exactly one entity".into());
        }
        let object = turret_of(world, arg(args, 0)?)?;
        Ok(Value::Int(
            world.resource::<RoundScript>().engine.turrets[&object]
                .active_user
                .is_some()
                .into(),
        ))
    });
    registry.register(Method, "settargetentity", set_target);
    registry.register(Method, "setturrettargetent", set_target);
    registry.register(Method, "cleartargetentity", |world, receiver, _| {
        edit(world, receiver, |t| t.manual = None)
    });
    registry.register(Method, "getturrettarget", |world, receiver, _| {
        let object = turret_of(world, receiver)?;
        let runtime = world.resource::<RoundScript>();
        Ok(runtime.engine.turrets[&object]
            .target
            .filter(|t| runtime.live(t))
            .map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "getturretowner", |world, receiver, _| {
        let object = turret_of(world, receiver)?;
        Ok(world.resource::<RoundScript>().engine.turrets[&object]
            .active_user
            .map_or(Value::Undefined, Value::Object))
    });
    registry.register(Method, "isfiringturret", |world, receiver, _| {
        let runtime = world.resource::<RoundScript>();
        Ok(Value::Int(
            runtime
                .entity(receiver)
                .and_then(|(object, _)| runtime.engine.turrets.get(&object))
                .is_some_and(|t| t.firing)
                .into(),
        ))
    });
    registry.register(Method, "shootturret", |world, receiver, _| {
        let object = turret_of(world, receiver)?;
        shoot(world, object)?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setturretteam", |world, receiver, args| {
        let team: Arc<str> = string(args, 0)?.into();
        edit(world, receiver, |t| t.team = Some(team))
    });
    registry.register(Method, "setsentryowner", |world, receiver, args| {
        let object = turret_of(world, receiver)?;
        if !matches!(
            &*world.resource::<RoundScript>().engine.turrets[&object].mode,
            "sentry" | "sentry_offline"
        ) {
            return Err("SetSentryOwner requires a sentry turret".into());
        }
        let owner = match arg(args, 0)? {
            Value::Undefined => None,
            value => Some(entity_id(world, value)?),
        };
        edit(world, receiver, |t| t.owner = owner)
    });
    registry.register(Method, "setsentrycarried", |world, receiver, args| {
        let carried = int(args, 0)? != 0;
        edit(world, receiver, |t| t.carried = carried)
    });
    registry.register(Method, "maketurretinoperable", |world, receiver, _| {
        edit(world, receiver, |t| t.operable = false)
    });
    registry.register(Method, "maketurretoperable", |world, receiver, _| {
        edit(world, receiver, |t| t.operable = true)
    });
    registry.register(Method, "startfiring", |world, receiver, _| {
        edit(world, receiver, |t| t.manual_firing = true)
    });
    registry.register(Method, "stopfiring", |world, receiver, _| {
        edit(world, receiver, |t| t.manual_firing = false)
    });
    registry.register(Method, "turretfiredisable", |world, receiver, _| {
        edit(world, receiver, |t| t.fire_enabled = false)
    });
    registry.register(Method, "turretfireenable", |world, receiver, _| {
        edit(world, receiver, |t| t.fire_enabled = true)
    });
    registry.register(Method, "maketurretsolid", |world, receiver, _| {
        let object = turret_of(world, receiver)?;
        let contents = world.resource::<RoundScript>().entities[&object].contents as u32
            | crate::bullet_collision::CONTENTS_SOLID;
        spawn::set_body(
            world,
            object,
            crate::EntityBodyBounds {
                midpoint: [0., 0., 28.],
                half_size: [14., 14., 28.],
                contents,
            },
        )?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "setautorotationdelay", |world, receiver, args| {
        let seconds = float(args, 0)?;
        if !seconds.is_finite() || seconds < 0. {
            return Err("auto rotation delay must be finite and nonnegative".into());
        }
        let delay = (seconds * 1000. + 0.5).floor() as i32;
        edit(world, receiver, |turret| {
            turret.auto_rotation_stop_delay = delay
        })
    });
    registry.register(Method, "setsentryminigun", |world, receiver, _| {
        edit(world, receiver, |t| t.minigun = true)
    });
    registry.register(
        Method,
        "setturretminimapvisible",
        |world, receiver, args| {
            let visible = int(args, 0)? != 0;
            edit(world, receiver, |t| t.minimap = visible)
        },
    );
    registry.register(
        Method,
        "setturretmodechangewait",
        |world, receiver, args| {
            let wait = optional(args, 0, int)?.map_or(1.0, |n| n as f32);
            edit(world, receiver, |t| t.mode_change_wait = wait)
        },
    );
    registry.register(Method, "setdefaultdroppitch", |world, receiver, args| {
        let pitch = float(args, 0)?;
        edit(world, receiver, |t| t.drop_pitch = pitch)
    });
    registry.register(Method, "setconvergencetime", |world, receiver, args| {
        let seconds = float(args, 0)?;
        let axis = optional(args, 1, string)?;
        let index = match axis.as_deref() {
            None | Some("yaw") => 1,
            Some("pitch") => 0,
            _ => return Err("convergence axis must be pitch or yaw".into()),
        };
        if !seconds.is_finite() {
            return Err("convergence time must be finite".into());
        }
        edit(world, receiver, |t| t.convergence[index] = seconds.max(0.))
    });
    registry.register(Method, "setsuppressiontime", |world, receiver, args| {
        let seconds = float(args, 0)?;
        if !seconds.is_finite() {
            return Err("suppression time must be finite".into());
        }
        edit(world, receiver, |t| t.suppression_seconds = seconds.max(0.))
    });
    registry.register(Method, "setaispread", |world, receiver, args| {
        let spread = float(args, 0)?;
        if !spread.is_finite() || !(0.0..90.0).contains(&spread) {
            return Err("AI spread must be finite, nonnegative and below 90 degrees".into());
        }
        edit(world, receiver, |t| t.ai_spread_degrees = spread)
    });
    registry.register(Method, "setplayerspread", |world, receiver, args| {
        let spread = float(args, 0)?;
        if !spread.is_finite() || !(0.0..90.0).contains(&spread) {
            return Err("player spread must be finite, nonnegative and below 90 degrees".into());
        }
        edit(world, receiver, |t| t.player_spread_degrees = spread)
    });
    fn arc(
        world: &mut World,
        receiver: &Value,
        args: &[Value],
        index: usize,
    ) -> Result<Value, String> {
        let degrees = float(args, 0)?.clamp(0.0, 180.0);
        edit(world, receiver, |t| t.arcs[index] = degrees)
    }
    registry.register(Method, "setleftarc", |w, r, a| arc(w, r, a, 0));
    registry.register(Method, "setrightarc", |w, r, a| arc(w, r, a, 1));
    registry.register(Method, "settoparc", |w, r, a| arc(w, r, a, 2));
    registry.register(Method, "setbottomarc", |w, r, a| arc(w, r, a, 3));

    registry.register(Method, "canplayerplacesentry", |world, receiver, _| {
        let client = super::natives::player::player(world, receiver)?;
        let (origin, yaw, eye) = FrameWorld::from_world(world)
            .player(ClientId(client))
            .map_or(([0.0; 3], 0.0, 0.0), |ps| {
                (ps.origin, ps.viewangles[1], ps.view_height_current)
            });
        let (placed, origin, angles) =
            super::sentry_placement::place(world, client, origin, yaw, eye);
        keyed_array(
            world,
            vec![
                ("origin", Value::Vector(origin)),
                ("angles", Value::Vector(angles)),
                ("result", Value::Int(placed.into())),
            ],
        )
    });
}

fn aim_point(world: &mut World, object: u64, offset: [f32; 3]) -> Option<Vec3> {
    if !world.resource::<RoundScript>().live(&object) {
        return None;
    }
    let origin = field_vector(world, object, "origin");
    Some(Vec3::from_array(origin) + Vec3::from_array(offset))
}

fn hostile(world: &mut World, turret: &Turret, target: u64) -> bool {
    if Some(target) == turret.owner || Some(target) == turret.active_user {
        return false;
    }
    let team = |world: &mut World, object: u64| match super::players::entity_field(
        world, object, "team",
    ) {
        Value::String(team) => Some(Arc::<str>::from(team)),
        _ => None,
    };
    let own = turret.team.clone().or_else(|| {
        turret
            .owner
            .or(turret.active_user)
            .and_then(|o| team(world, o))
    });
    match own.as_deref() {
        None | Some("free") => true,
        Some(own) => team(world, target).as_deref() != Some(own),
    }
}

fn visible(world: &mut World, from: [f32; 3], to: Vec3, ignore: TraceIgnore, target: u64) -> bool {
    match entity_trace(world, from, to.to_array(), MASK_SHOT, ignore) {
        TraceOutcome::Miss { .. } => true,
        TraceOutcome::Hit { collider, .. } => {
            collider_entity(world, collider) == Value::Object(target)
        }
        _ => false,
    }
}

fn within_arcs(turret: &Turret, base: [f32; 3], dir: Vec3) -> bool {
    let angles = math_iw4::vect_to_angles(dir.to_array());
    let yaw = math_iw4::angle_subtract(angles[1], base[1]);
    let pitch = math_iw4::angle_subtract(angles[0], base[0]);
    let [left, right, top, bottom] = turret.arcs;
    (-right..=left).contains(&yaw) && (-top..=bottom).contains(&pitch)
}

pub(super) fn actor_can_engage(world: &mut World, actor: u64, object: u64) -> Result<bool, String> {
    let turret = world.resource::<RoundScript>().engine.turrets[&object].clone();
    let target = world
        .resource::<RoundScript>()
        .sentients
        .get(&actor)
        .and_then(|state| state.target);
    let Some(target) = target else {
        return Ok(true);
    };
    let Some(at) = aim_point(world, target, [0.; 3]) else {
        return Ok(true);
    };
    let actor_origin = Vec3::from_array(field_vector(world, actor, "origin"));
    const CLOSE_THREAT_UNITS: f32 = 256.;
    if matches!(&*turret.mode, "manual" | "manual_ai")
        && actor_origin.distance_squared(at) >= CLOSE_THREAT_UNITS.powi(2)
    {
        return Ok(true);
    }
    let from = muzzle(world, object);
    let delta = at - Vec3::from_array(from);
    if delta.length_squared() > turret.max_range * turret.max_range {
        return Ok(true);
    }
    let visible_to_actor = world.resource::<RoundScript>().actors[&actor]
        .sentient_info
        .get(&target)
        .is_some_and(|info| info.visible);
    if !visible_to_actor {
        return Ok(true);
    }
    let Some(direction) = delta.try_normalize() else {
        return Ok(false);
    };
    let base = field_vector(world, object, "angles");
    Ok(within_arcs(&turret, base, direction)
        && visible(world, from, at, ignore_self(world, object), target))
}

pub(super) fn actor_should_detach(
    world: &mut World,
    actor: u64,
    object: u64,
) -> Result<bool, String> {
    let turret = &world.resource::<RoundScript>().engine.turrets[&object];
    if !turret.can_ai_detach || !matches!(&*turret.mode, "sentry" | "auto_ai" | "auto_nonai") {
        return Ok(false);
    }
    actor_can_engage(world, actor, object).map(|can_engage| !can_engage)
}

fn acquire(world: &mut World, object: u64, turret: &Turret, from: [f32; 3]) -> Option<(u64, Vec3)> {
    if let Some((target, offset)) = turret.manual {
        return aim_point(world, target, offset).map(|at| (target, at));
    }
    if !matches!(&*turret.mode, "sentry" | "auto_ai" | "auto_nonai") {
        return None;
    }
    let base = field_vector(world, object, "angles");
    let range = match world
        .resource_mut::<RoundScript>()
        .object_field(object, "maxrange")
    {
        Value::Float(value) => value,
        Value::Int(value) => value as f32,
        Value::String(value) => value.parse::<f32>().unwrap_or(turret.max_range),
        _ => turret.max_range,
    };
    if !range.is_finite() {
        return None;
    }
    let range = if range > 0. { range } else { f32::MAX };
    let ignore = ignore_self(world, object);
    let runtime = world.resource::<RoundScript>();
    let candidates: std::collections::BTreeSet<u64> = runtime
        .players
        .values()
        .filter(|slot| &*slot.sessionstate == "playing")
        .map(|slot| slot.object)
        .chain(runtime.actors.keys().copied())
        .collect();
    let mut best: Option<(f32, u64, Vec3)> = None;
    for player in candidates {
        let health = super::players::entity_field(world, player, "health");
        if !matches!(health,Value::Int(n) if n>0) && !matches!(health,Value::Float(n) if n>0.) {
            continue;
        }
        let Value::Vector(origin) = super::players::entity_field(world, player, "origin") else {
            continue;
        };
        if !hostile(world, turret, player) {
            continue;
        }
        let at = Vec3::from_array(origin) + Vec3::Z * TARGET_HEIGHT;
        let delta = at - Vec3::from_array(from);
        let distance = delta.length();
        if distance > range || best.is_some_and(|(d, ..)| d <= distance) {
            continue;
        }
        let Some(dir) = delta.try_normalize() else {
            continue;
        };
        if !within_arcs(turret, base, dir) || !visible(world, from, at, ignore, player) {
            continue;
        }
        best = Some((distance, player, at));
    }
    best.map(|(_, player, at)| (player, at))
}

pub(crate) fn advance(world: &mut World) {
    let now = super::players::now_ms(world) as i32;
    let objects: Vec<u64> = world
        .resource::<RoundScript>()
        .engine
        .turrets
        .keys()
        .copied()
        .collect();
    for object in objects {
        if !world
            .resource::<RoundScript>()
            .entities
            .contains_key(&object)
        {
            world
                .resource_mut::<RoundScript>()
                .engine
                .turrets
                .remove(&object);
            continue;
        }
        let mut turret = world.resource::<RoundScript>().engine.turrets[&object].clone();
        if operator::advance(world, object, &mut turret, now) {
            continue;
        }
        let base = field_vector(world, object, "angles");
        if !turret.aim_initialized || turret.carried || &*turret.mode == "sentry_offline" {
            turret.aim = math_iw4::angles_to_axis(base)[0];
            turret.aim_initialized = true;
        }
        let from = muzzle(world, object);
        let active = !turret.carried && &*turret.mode != "sentry_offline";
        let mut found = if active {
            acquire(world, object, &turret, from)
        } else {
            None
        };
        if let Some((_, point)) = found {
            turret.last_seen_at = now;
            turret.last_seen_point = Some(point.to_array());
        } else if active
            && (now - turret.last_seen_at) as f32 <= turret.suppression_seconds * 1000.
            && let Some(target) = turret
                .target
                .filter(|target| world.resource::<RoundScript>().live(target))
            && let Some(point) = turret.last_seen_point
        {
            let health = super::players::entity_field(world, target, "health");
            if matches!(health,Value::Int(n) if n>0) || matches!(health,Value::Float(n) if n>0.) {
                found = Some((target, Vec3::from_array(point)));
            }
        }
        let was_firing = turret.firing;
        let was_on_target = turret.on_target;
        match found {
            Some((target, at)) => {
                if turret.target != Some(target) {
                    turret.on_target = false;
                    turret.acquired_at = now;
                }
                turret.target = Some(target);
                if let Some(wanted) = (at - Vec3::from_array(from)).try_normalize() {
                    let mut facing = math_iw4::vect_to_angles(turret.aim);
                    let desired = math_iw4::vect_to_angles(wanted.to_array());
                    for axis in 0..2 {
                        let delta = math_iw4::angle_subtract(desired[axis], facing[axis]);
                        let duration = turret.convergence[axis] * 1000.;
                        let step = if duration > 0. {
                            let remaining = (duration - (now - turret.acquired_at) as f32)
                                .max(crate::MATCH_TICK_MS as f32);
                            delta.abs() * crate::MATCH_TICK_MS as f32 / remaining
                        } else {
                            TURN_RATE_DEG_PER_S * crate::MATCH_TICK_MS as f32 * 0.001
                        };
                        facing[axis] += delta.clamp(-step, step);
                    }
                    let aim = Vec3::from_array(math_iw4::angles_to_axis(facing)[0]);
                    turret.aim = aim.to_array();
                    turret.on_target = aim.angle_between(wanted) <= ON_TARGET_DEGREES.to_radians();
                }
            }
            None => {
                turret.target = None;
                turret.on_target = false;
                if active && &*turret.mode == "sentry" {
                    let angles = math_iw4::vect_to_angles(turret.aim);
                    let yaw = math_iw4::angle_subtract(angles[1], base[1]);
                    let left = turret.arcs[0].min(90.0);
                    let right = turret.arcs[1].min(90.0);
                    let next = yaw
                        + turret.scan_direction
                            * SCAN_RATE_DEG_PER_S
                            * crate::MATCH_TICK_MS as f32
                            * 0.001;
                    if next >= left {
                        turret.scan_direction = -1.0;
                    } else if next <= -right {
                        turret.scan_direction = 1.0;
                    }
                    let wanted = math_iw4::angles_to_axis([
                        base[0],
                        base[1] + next.clamp(-right, left),
                        base[2],
                    ])[0];
                    turret.aim = super::guidance::turn_toward(
                        Vec3::from_array(turret.aim),
                        Vec3::from_array(wanted),
                        SCAN_RATE_DEG_PER_S.to_radians() * crate::MATCH_TICK_MS as f32 * 0.001,
                    )
                    .to_array();
                }
            }
        }
        turret.firing = turret.on_target
            && turret.fire_enabled
            && (!matches!(&*turret.mode, "manual" | "manual_ai") || turret.manual_firing);
        turret
            .spin
            .advance(i64::from(now), turret.firing || turret.manual_firing);
        let raise_on_target = turret.on_target && !was_on_target;
        let raise_state = turret.firing != was_firing;
        world
            .resource_mut::<RoundScript>()
            .engine
            .turrets
            .insert(object, turret);
        if raise_on_target {
            raise(world, Value::Object(object), "turret_on_target", Vec::new());
        }
        if raise_state {
            raise(
                world,
                Value::Object(object),
                "turretstatechange",
                Vec::new(),
            );
        }
    }
}

fn attack_credit(
    runtime: &RoundScript,
    turret: &Turret,
) -> (Option<ClientId>, Option<crate::ScriptModelId>) {
    let owner = turret.owner.or(turret.active_user);
    let client = owner.and_then(|id| runtime.player_client(id)).map(ClientId);
    let entity = owner
        .and_then(|id| runtime.entities.get(&id))
        .and_then(|entity| entity.presence);
    (client, entity.filter(|_| client.is_none()))
}

fn shoot(world: &mut World, object: u64) -> Result<(), String> {
    let turret = world.resource::<RoundScript>().engine.turrets[&object].clone();
    let now = super::players::now_ms(world) as i32;
    if !turret.fire_enabled
        || turret.carried
        || &*turret.mode == "sentry_offline"
        || now < turret.next_fire_at
    {
        return Ok(());
    }
    let interval = FrameWorld::from_world(world)
        .combat_facts_for(turret.weapon)
        .ok_or("turret weapon has no firing facts")?
        .fire_time_ms
        .max(1);
    let from = muzzle(world, object);
    let (attacker, attacker_entity) = attack_credit(world.resource::<RoundScript>(), &turret);
    // Script-controlled manual fire can have a player owner; autonomous sentries
    // use the AI cone even when their kill credit belongs to a player.
    let player_controlled = attacker.is_some() && &*turret.mode == "manual" && turret.manual_firing;
    let spread = if player_controlled {
        turret.player_spread_degrees
    } else {
        turret.ai_spread_degrees
    };
    let direction =
        super::natives::math::sample_shot_direction(world, Vec3::from_array(turret.aim), spread)?;
    fire_bullet(
        world,
        object,
        from,
        direction.to_array(),
        turret.weapon,
        attacker,
        attacker_entity,
    )?;
    let number = world.resource::<RoundScript>().entities[&object].number;
    let tick = world.resource::<crate::step::StepRequest>().tick;
    FrameWorld::from_world(world).push_entity_event(
        tick,
        crate::EventAudience::All,
        entity_iw4::EntityEventKind::FIRE_WEAPON,
        crate::EntityEventPayload {
            number,
            attacker_entity_num: number,
            weapon: turret.weapon,
            origin: from,
            direction: direction.to_array(),
            ..Default::default()
        },
    );
    if let Some(turret) = world
        .resource_mut::<RoundScript>()
        .engine
        .turrets
        .get_mut(&object)
    {
        turret.next_fire_at = now.saturating_add(interval);
    }
    Ok(())
}

pub(crate) fn fire_bullet(
    world: &mut World,
    object: u64,
    from: [f32; 3],
    dir: [f32; 3],
    weapon: u32,
    attacker: Option<ClientId>,
    attacker_entity: Option<crate::ScriptModelId>,
) -> Result<(), String> {
    let facts = FrameWorld::from_world(world)
        .combat_facts_for(weapon)
        .ok_or("bullet weapon has no executable firing facts")?;
    let range = weapon_range(world, weapon);
    let end = (Vec3::from_array(from) + Vec3::from_array(dir) * range).to_array();
    let mut ignore = ignore_self(world, object);
    if world
        .resource::<RoundScript>()
        .engine
        .turrets
        .get(&object)
        .is_some_and(|t| t.active_user.is_some())
    {
        ignore.client = attacker;
    }
    let outcome = entity_trace(world, from, end, MASK_SHOT, ignore);
    if let TraceOutcome::Invalid { reason } = outcome {
        return Err(format!("bullet trace is unavailable: {reason:?}"));
    }
    if attacker_entity.is_some() && world.resource::<RoundScript>().actors.contains_key(&object) {
        let reached = match &outcome {
            TraceOutcome::Hit { end, .. }
            | TraceOutcome::Miss { end }
            | TraceOutcome::StartSolid { end, .. } => *end,
            TraceOutcome::Invalid { .. } => unreachable!(),
        };
        super::ai_events::bullet(world, object, from, reached, facts.silenced, false);
        super::actors::suppression::bullet_line(
            world,
            object,
            from,
            reached,
            super::players::now_ms(world) as i32,
        )?;
    }
    let TraceOutcome::Hit {
        collider,
        end,
        normal,
        ..
    } = outcome
    else {
        return Ok(());
    };
    let distance = (0..3)
        .map(|i| (end[i] - from[i]).powi(2))
        .sum::<f32>()
        .sqrt();
    let hitloc = match collider {
        ColliderId::Player { hitloc, .. } => hitloc,
        ColliderId::EntityDObjBone {
            part_classification,
            ..
        } => part_classification,
        _ => 0,
    };
    let amount = (crate::bullet::bullet_damage_at_distance(&facts, distance) as f32
        * facts.location_scale(hitloc)) as i32;
    let hit = match collider {
        ColliderId::Player { .. } | ColliderId::World { .. } => None,
        _ => match collider_entity(world, collider) {
            Value::Object(hit) => Some(hit),
            _ => return Ok(()),
        },
    };
    let runtime = world.resource::<RoundScript>();
    let number = runtime.entities[&object].number;
    let other_entity_num = match collider {
        ColliderId::Player { client, .. } => client.0 as i32,
        _ => hit
            .and_then(|hit| runtime.entities.get(&hit))
            .map_or(playerstate_iw4::ENTITYNUM_NONE, |entity| entity.number),
    };
    let surface_flags = match collider {
        ColliderId::World { surface_flags, .. }
        | ColliderId::EntityDObjBone { surface_flags, .. }
        | ColliderId::EntityLinkedBrush { surface_flags, .. } => surface_flags,
        ColliderId::Player { .. } => 0,
        ColliderId::EntityBody { .. } => weapon_iw4::SURF_TYPE_FLESH << 20,
    };
    let surf_type = if matches!(collider, ColliderId::Player { .. }) {
        weapon_iw4::SURF_TYPE_FLESH as u8
    } else {
        trace_iw4::surface_type_from_flags(surface_flags) as u8
    };
    let tick = world.resource::<crate::step::StepRequest>().tick;
    let mut frame = FrameWorld::from_world(world);
    let correlation = frame.alloc_shot_id().0;
    let payload = crate::EntityEventPayload {
        number,
        attacker_entity_num: number,
        other_entity_num,
        weapon,
        correlation,
        origin: end,
        origin2: from,
        direction: normal,
        surf_type,
        surface_flags,
        ..Default::default()
    };
    if let Some(event) = entity_iw4::bullet_hit_event(facts.impact_type, false) {
        frame.push_entity_event(tick, crate::EventAudience::All, event, payload);
    } else if fx_iw4::impact_table_row(facts.impact_type, false).is_some() {
        frame.push_pellet_fx(crate::PelletFxRecord {
            attacker: number,
            weapon,
            correlation,
            fire_cause: None,
            pellet: 0,
            segment: 0,
            hand: 0,
            start: from,
            end,
            normal,
            surf_type,
            surface_flags,
            flesh_flags: 0,
        });
    }
    let runtime = world.resource::<RoundScript>();
    let target = match collider {
        ColliderId::Player { client, .. } => crate::script::HitTarget::Player(client),
        ColliderId::World { .. } => return Ok(()),
        _ => match hit.and_then(|hit| runtime.entities.get(&hit)) {
            Some(e) if e.can_damage => match e.presence {
                Some(presence) => crate::script::HitTarget::Entity(presence),
                None => return Ok(()),
            },
            _ => return Ok(()),
        },
    };
    let inflictor = runtime.entities.get(&object).and_then(|e| e.presence);
    world
        .resource_mut::<RoundScript>()
        .hits
        .push(crate::script::ScriptHit {
            piece: None,
            target,
            amount,
            origin: from,
            attacker,
            attacker_entity,
            inflictor,
            means: "MOD_RIFLE_BULLET",
            weapon,
            flags: 0,
            hitloc,
        });
    Ok(())
}
