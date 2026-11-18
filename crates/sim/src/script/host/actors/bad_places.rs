use super as actors;
use super::super::{args, triggers};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use glam::{Vec2, Vec3};

#[derive(Clone, Debug, Default)]
pub(crate) struct BadPlaces {
    zones: Vec<Zone>,
}
#[derive(Clone, Debug)]
struct Zone {
    name: String,
    expires: Option<i64>,
    teams: u8,
    shape: Shape,
}
#[derive(Clone, Debug)]
enum Shape {
    Arc {
        center: [f32; 3],
        radius: f32,
        half_height: f32,
        start: f32,
        sweep: f32,
    },
    Brush(u64),
}
fn now(world: &World) -> i64 {
    super::super::players::now_ms(world)
}
fn team(team: u8) -> u8 {
    if team == 4 { 3 } else { team }
}
fn mask(args: &[Value]) -> Result<u8, String> {
    let flags = super::super::spawners::team_flags(args)?;
    let flags = if flags == 0 { 0x0e } else { flags };
    Ok(if flags & 0x10 != 0 {
        (flags & 6) | 8
    } else {
        flags
    })
}
fn prune(world: &mut World) {
    let at = now(world);
    let mut r = world.resource_mut::<RoundScript>();
    let entries = std::mem::take(&mut r.bad_places.zones);
    let entries = entries
        .into_iter()
        .filter(|z| {
            z.expires.is_none_or(|end| at < end)
                && match z.shape {
                    Shape::Brush(id) => r.can_receive_call(&id),
                    _ => true,
                }
        })
        .collect();
    r.bad_places.zones = entries;
}
impl BadPlaces {
    pub(crate) fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }
    pub(crate) fn trace_values(&self, values: &mut Vec<Value>) {
        values.extend(self.zones.iter().filter_map(|z| match z.shape {
            Shape::Brush(id) => Some(Value::Object(id)),
            _ => None,
        }));
    }
}
fn merge(mut intervals: Vec<(f32, f32)>) -> Vec<(f32, f32)> {
    intervals.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = Vec::new();
    for (a, b) in intervals {
        if let Some(last) = out.last_mut()
            && a <= last.1
        {
            last.1 = last.1.max(b);
        } else {
            out.push((a, b));
        }
    }
    out
}
impl Shape {
    fn contains_arc(&self, p: [f32; 3]) -> bool {
        let Self::Arc {
            center,
            radius,
            half_height,
            start,
            sweep,
        } = *self
        else {
            return false;
        };
        let v = Vec2::new(p[0] - center[0], p[1] - center[1]);
        v.length_squared() <= radius * radius
            && (p[2] - center[2]).abs() <= half_height
            && (sweep >= 360.
                || v.length_squared() <= f32::EPSILON
                || (v.y.atan2(v.x).to_degrees() - start).rem_euclid(360.) <= sweep)
    }
    fn intervals(
        &self,
        world: &mut World,
        a: [f32; 3],
        b: [f32; 3],
    ) -> Result<Vec<(f32, f32)>, String> {
        let Self::Arc {
            center,
            radius,
            half_height,
            start,
            sweep,
        } = *self
        else {
            let Self::Brush(id) = self else {
                unreachable!()
            };
            return triggers::brush_intervals(world, *id, a, b).map(merge);
        };
        // Split a segment at geometric boundary crossings, then classify each
        // interval. This handles narrow and reflex sectors without sampling grids.
        let p = Vec2::new(a[0] - center[0], a[1] - center[1]);
        let d = Vec2::new(b[0] - a[0], b[1] - a[1]);
        let mut cuts = vec![0., 1.];
        let mut add = |t: f32| {
            if t > 0. && t < 1. {
                cuts.push(t);
            }
        };
        let aa = d.length_squared();
        if aa > 0. {
            let projection = -p.dot(d) / aa;
            let distance_sq = (p + d * projection).length_squared();
            if distance_sq <= radius * radius {
                let half = ((radius * radius - distance_sq) / aa).sqrt();
                add(projection - half);
                add(projection + half);
            }
        }
        if b[2] != a[2] {
            add((center[2] - half_height - a[2]) / (b[2] - a[2]));
            add((center[2] + half_height - a[2]) / (b[2] - a[2]));
        }
        if sweep < 360. {
            for angle in [start, start + sweep] {
                let (sin, cos) = angle.to_radians().sin_cos();
                let n = Vec2::new(-sin, cos);
                let denom = n.dot(d);
                if denom != 0. {
                    add(-n.dot(p) / denom);
                }
            }
        }
        cuts.sort_by(f32::total_cmp);
        cuts.dedup();
        let intervals = cuts
            .windows(2)
            .filter_map(|window| {
                let (lo, hi) = (window[0], window[1]);
                let t = (lo + hi) * 0.5;
                let point = std::array::from_fn(|i| a[i] + t * (b[i] - a[i]));
                self.contains_arc(point).then_some((lo, hi))
            })
            .collect();
        Ok(merge(intervals))
    }
}
fn active(world: &World, z: &Zone, team: u8) -> bool {
    z.expires.is_none_or(|end| now(world) < end)
        && (team < 8 && z.teams & (1 << team) != 0)
        && match z.shape {
            Shape::Brush(id) => world.resource::<RoundScript>().can_receive_call(&id),
            _ => true,
        }
}
pub(crate) fn contains(world: &mut World, id: u64, at: [f32; 3]) -> Result<bool, String> {
    let r = world.resource::<RoundScript>();
    let Some(t) = r.sentients.get(&id).map(|s| team(s.team)) else {
        return Ok(false);
    };
    let zones = r.bad_places.zones.clone();
    for z in zones {
        if active(world, &z, t) && !z.shape.intervals(world, at, at)?.is_empty() {
            return Ok(true);
        }
    }
    Ok(false)
}
pub(crate) fn allows_segment(world: &mut World, id: u64, a: [f32; 3], b: [f32; 3]) -> bool {
    let r = world.resource::<RoundScript>();
    let Some(t) = r.sentients.get(&id).map(|s| team(s.team)) else {
        return true;
    };
    let zones = r.bad_places.zones.clone();
    for z in zones {
        if !active(world, &z, t) {
            continue;
        }
        let Ok(intervals) = z.shape.intervals(world, a, b) else {
            return false;
        };
        // Allow egress from a zone already occupied, but never re-entry after
        // leaving it. A source outside a zone cannot cross into its interior.
        if !intervals.is_empty() && (intervals.len() != 1 || intervals[0].0 > 0.) {
            return false;
        }
        if !intervals.is_empty() {
            let Ok(start) = z.shape.intervals(world, a, a) else {
                return false;
            };
            if start.is_empty() {
                return false;
            }
        }
    }
    true
}
fn install(
    world: &mut World,
    name: String,
    seconds: f32,
    teams: u8,
    shape: Shape,
) -> Result<Value, String> {
    if !seconds.is_finite() {
        return Err("badplace duration must be finite".into());
    }
    if seconds <= 0. && name.is_empty() {
        return Err("permanent badplace requires a nonempty name".into());
    }
    let expires = if seconds > 0. {
        let ms = f64::from((seconds * 1000.).ceil());
        if ms > i32::MAX as f64 {
            return Err("badplace duration is too large".into());
        }
        Some(now(world) + ms as i64)
    } else {
        None
    };
    prune(world);
    let mut r = world.resource_mut::<RoundScript>();
    let zones = &mut r.bad_places.zones;
    let previous = (!name.is_empty())
        .then(|| zones.iter().position(|z| z.name == name))
        .flatten();
    if previous.is_none() && zones.len() >= 32 {
        return Err("badplace limit (32) exceeded".into());
    }
    let zone = Zone {
        name,
        expires,
        teams,
        shape,
    };
    if let Some(index) = previous {
        zones[index] = zone;
    } else {
        zones.push(zone);
    }
    Ok(Value::Undefined)
}
fn arc(world: &mut World, args: &[Value], sector: bool) -> Result<Value, String> {
    let min = if sector { 8 } else { 5 };
    if args.len() < min {
        return Err("insufficient badplace arguments".into());
    }
    let name = args::string(args, 0)?;
    let duration = args::float(args, 1)?;
    let center = args::vector(args, 2)?;
    let radius = args::float(args, 3)?;
    let height = args::float(args, 4)?;
    if !center
        .into_iter()
        .chain([radius, height])
        .all(f32::is_finite)
        || radius < 0.
        || height < 0.
    {
        return Err("badplace dimensions must be finite and nonnegative".into());
    }
    let (start, sweep) = if sector {
        let direction = args::vector(args, 5)?;
        let left = args::float(args, 6)?;
        let right = args::float(args, 7)?;
        if !direction
            .into_iter()
            .chain([left, right])
            .all(f32::is_finite)
            || left < 0.
            || right < 0.
        {
            return Err("badplace arc angles must be finite and nonnegative".into());
        }
        (
            (direction[1].atan2(direction[0]).to_degrees() - left).rem_euclid(360.),
            (left + right).min(360.),
        )
    } else {
        (0., 360.)
    };
    install(
        world,
        name,
        duration,
        mask(&args[min..])?,
        Shape::Arc {
            center,
            radius,
            half_height: height * 0.5,
            start,
            sweep,
        },
    )
}
pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "badplace_cylinder", |w, _, a| {
        arc(w, a, false)
    });
    registry.register(Namespace::Function, "badplace_arc", |w, _, a| {
        arc(w, a, true)
    });
    registry.register(Namespace::Function, "badplace_brush", |w, _, a| {
        if a.len() < 3 {
            return Err("BadPlace_Brush requires name, duration and brush".into());
        }
        let id = super::super::natives::engine::entity_id(w, args::arg(a, 2)?)?;
        let at = super::super::natives::engine::origin_of(w, &Value::Object(id))?;
        triggers::brush_intervals(w, id, at, at)?;
        install(
            w,
            args::string(a, 0)?,
            args::float(a, 1)?,
            mask(&a[3..])?,
            Shape::Brush(id),
        )
    });
    registry.register(Namespace::Function, "badplace_delete", |w, _, a| {
        if a.len() != 1 {
            return Err("BadPlace_Delete requires one name".into());
        }
        let name = args::string(a, 0)?;
        if name.is_empty() {
            return Err("BadPlace_Delete requires a nonempty name".into());
        }
        w.resource_mut::<RoundScript>()
            .bad_places
            .zones
            .retain(|z| z.name != name);
        Ok(Value::Undefined)
    });
}
pub(super) fn release(actor: &mut actors::Actor) -> bool {
    if let Some(goal) = actor.badplace_escape.take() {
        if actor.claimed_node == goal.node {
            actor.claimed_node = None;
            actor.at_claim_node = false;
        }
        actor.navigation = Default::default();
        actor.move_mode = 0;
        if let Some(state) = actor.initial_state.as_mut().filter(|s| s.state == 4) {
            state.state = 1;
            state.substate = 100;
        }
        return true;
    }
    false
}
pub(crate) fn advance(world: &mut World, _now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_bad_places", "iw4l.sim").enter();
    prune(world);
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .keys()
        .copied()
        .collect();
    for id in ids {
        let eligible = {
            let r = world.resource::<RoundScript>();
            let a = &r.actors[&id];
            a.alive
                && !a.delayed_death
                && a.physics_initialized
                && a.scripted_control.is_none()
                && actors::anim_mode::effective(a) != actors::anim_mode::Mode::PointRelative
                && !a.arrival.transitioning()
                && r.entities[&id].linked_to.is_none()
                && (a.anim_script_kind.is_none_or(|k| k.is_cover())
                    || !actors::anim_mode::effective(a).uses_root_motion())
        };
        if !eligible {
            release(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            );
            continue;
        }
        let Value::Vector(at) = world
            .resource_mut::<RoundScript>()
            .object_field(id, "origin")
        else {
            return Err("badplace actor has no origin".into());
        };
        if !contains(world, id, at)? {
            if release(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            ) {
                crate::script::runtime::raise(world, Value::Object(id), "goal_changed", vec![]);
            }
            continue;
        }
        let a = &world.resource::<RoundScript>().actors[&id];
        if let Some(goal) = a.badplace_escape.clone() {
            let path = a.navigation.clone();
            if !contains(world, id, goal.position)?
                && actors::navigation::route_supported(world, id, &path, at, false)
            {
                continue;
            }
        }
        let mut candidates: Vec<_> = world
            .resource::<RoundScript>()
            .path_nodes
            .nodes()
            .iter()
            .enumerate()
            .filter(|(_, n)| matches!(n.kind, 1..=12 | 19))
            .map(|(i, n)| {
                (
                    Vec3::from_array(n.origin).distance_squared(Vec3::from_array(at)),
                    i as u16,
                    n.origin,
                )
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let mut found = None;
        let mut searched = false;
        let mut search = None;
        for (_, node, position) in candidates {
            if !actors::cover::claimable(world.resource::<RoundScript>(), id, node) {
                continue;
            }
            if contains(world, id, position)? {
                continue;
            }
            let goal = actors::goals::Goal {
                position,
                node: Some(node),
                height: actors::spatial::ACTOR_BOUNDS.half_size[2],
                ..Default::default()
            };
            if actors::navigation::direct_supported(world, id, at, position, false) {
                if let Some(route) = actors::navigation::supported_plan(world, id, at, &goal, false)
                {
                    found = Some((goal, route));
                    break;
                }
            } else {
                if !searched {
                    search = actors::navigation::RouteSearch::build(world, id, at, false);
                    searched = true;
                }
                if let Some(route) = search
                    .as_ref()
                    .and_then(|search| search.to_node(world, node, &goal))
                {
                    found = Some((goal, route));
                    break;
                }
            }
        }
        if let Some((goal, route)) = found {
            {
                let mut r = world.resource_mut::<RoundScript>();
                let a = r.actors.get_mut(&id).unwrap();
                actors::arrival::clear(a);
                actors::reacquire::clear(a, false);
                actors::cover::clear(a);
                a.claimed_node = goal.node;
                a.at_claim_node = false;
                a.badplace_escape = Some(goal);
                a.navigation = route;
                a.script_anim_mode = actors::anim_mode::Mode::None;
                if let Some(state) = a.initial_state.as_mut() {
                    state.state = 4;
                }
            }
            crate::script::runtime::raise(world, Value::Object(id), "goal_changed", vec![]);
        } else {
            release(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            );
        }
    }
    Ok(())
}
