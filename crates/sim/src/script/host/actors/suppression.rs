use super::super::args::int;
use super::*;

type V3 = [f32; 3];
pub(super) const FIELDS: [&str; 5] = [
    "ignoresuppression",
    "suppressionwait",
    "suppressionduration",
    "suppressionstarttime",
    "suppressionmeter",
];

#[derive(Clone, Copy, Debug, Default)]
struct Entry {
    time: i32,
    suppressor: Option<u64>,
    plane: [f32; 3],
    movement_only: bool,
}
impl Entry {
    fn clear(&mut self) {
        self.time = 0;
        self.suppressor = None;
        self.movement_only = false;
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Suppression {
    entries: [Entry; 4],
    ignore: i32,
    wait: i32,
    duration: i32,
    start: i32,
    meter: f32,
}
impl Default for Suppression {
    fn default() -> Self {
        Self {
            entries: Default::default(),
            ignore: 0,
            wait: 2000,
            duration: 5000,
            start: 0,
            meter: 0.,
        }
    }
}
impl Suppression {
    pub(super) fn load(&self, field: &str) -> Option<Value> {
        Some(match field {
            "ignoresuppression" => Value::Int(self.ignore),
            "suppressionwait" => Value::Int(self.wait),
            "suppressionduration" => Value::Int(self.duration),
            "suppressionstarttime" => Value::Int(self.start),
            "suppressionmeter" => Value::Float(self.meter),
            _ => return None,
        })
    }
    pub(super) fn store(&mut self, field: &str, value: &Value) -> Result<bool, String> {
        match field {
            "ignoresuppression" => self.ignore = int(std::slice::from_ref(value), 0)?,
            "suppressionwait" => self.wait = int(std::slice::from_ref(value), 0)?,
            "suppressionduration" => self.duration = int(std::slice::from_ref(value), 0)?,
            "suppressionstarttime" | "suppressionmeter" => {
                return Err(format!("actor {field} is read-only"));
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn suppressed(&self) -> bool {
        self.meter > 0. || self.start > 0
    }
    fn move_suppressed(&self) -> bool {
        self.entries.iter().any(|e| e.time != 0 && e.movement_only)
    }
    fn waiting(&self, now: i32) -> bool {
        self.ignore == 0
            && self
                .entries
                .iter()
                .any(|e| e.time != 0 && !e.movement_only && now.wrapping_sub(e.time) < self.wait)
    }
    fn enemy_retained(&self) -> bool {
        self.entries.iter().any(|e| e.time > 0 && !e.movement_only)
    }
    fn finish(&mut self) -> bool {
        if self.enemy_retained() {
            return false;
        }
        let ended = self.start != 0;
        self.start = 0;
        ended
    }
    fn decay(&mut self, now: i32, friendly_duration: i32) -> bool {
        self.meter = if self.ignore == 0 {
            (self.meter - 0.01).max(0.)
        } else {
            0.
        };
        for e in &mut self.entries {
            let duration = if e.movement_only {
                friendly_duration
            } else if self.ignore == 0 {
                self.duration
            } else {
                0
            };
            if e.time > 0 && now.wrapping_sub(e.time) >= duration {
                e.clear();
            }
        }
        self.finish()
    }
    fn add(&mut self, shooter: u64, plane: [f32; 3], now: i32, movement_only: bool) {
        let reuse = self.entries.iter().position(|e| {
            if movement_only {
                e.suppressor == Some(shooter)
            } else {
                !e.movement_only
            }
        });
        let index = reuse.unwrap_or_else(|| (0..4).min_by_key(|&i| self.entries[i].time).unwrap());
        let e = &mut self.entries[index];
        e.time = now;
        e.suppressor = Some(shooter);
        e.plane = plane;
        if movement_only || self.ignore == 0 {
            e.movement_only = movement_only;
        }
        if !movement_only && self.ignore == 0 && self.start == 0 {
            self.start = now;
        }
    }
}

#[derive(Clone, Copy)]
struct Settings {
    friendly: bool,
    friendly_duration: i32,
    radius: f32,
}
fn settings(world: &World) -> Settings {
    let dvars = &world.resource::<crate::script::MatchScript>().dvars;
    Settings {
        friendly: dvars.get("ai_friendlysuppression").is_none_or(|v| v != "0"),
        friendly_duration: dvars
            .get("ai_friendlyfireblockduration")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2000),
        radius: dvars
            .get("ai_eventdistbullet")
            .and_then(|v| v.parse().ok())
            .unwrap_or(128.),
    }
}
fn now(world: &World) -> i32 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| crate::level_time_ms(r.tick))
}
fn notify(runtime: &mut RoundScript, owner: u64, name: &'static str, args: Vec<Value>) {
    runtime
        .pending_notifies
        .push((Value::Object(owner), name.into(), args));
}

fn nearest(origin: V3, start: V3, end: V3, radius: f32) -> Option<V3> {
    let v: V3 = std::array::from_fn(|i| end[i] - start[i]);
    let len: f32 = v.iter().map(|v| v * v).sum();
    let projection: f32 = (0..3).map(|i| (origin[i] - start[i]) * v[i]).sum();
    if projection < 0. {
        return None;
    }
    let t = if projection < len {
        projection / len
    } else {
        1.
    };
    let point = std::array::from_fn(|i| start[i] + t * v[i]);
    let xy = (origin[0] - point[0]).powi(2) + (origin[1] - point[1]).powi(2);
    (xy <= radius * radius && (origin[2] - point[2] + 36.).abs() < 45.).then_some(point)
}

fn corner(
    runtime: &RoundScript,
    id: u64,
    origin: V3,
) -> Result<Option<(u16, asset_core::PathNode)>, String> {
    if let Some(index) = runtime.actors[&id].claimed_node
        && let Some(node) = runtime.path_nodes.at(index)
        && matches!(node.kind, 6 | 7)
        && (origin[2] - node.origin[2]).powi(2) <= 6400.
        && (origin[0] - node.origin[0]).powi(2) + (origin[1] - node.origin[1]).powi(2) <= 4096.
    {
        return Ok(Some((index, node.clone())));
    }
    if runtime.path_nodes.is_empty() {
        return Ok(None);
    }
    Err("enemy suppression requires sentient nearest-node resolution".into())
}

pub(crate) fn bullet_line(
    world: &mut World,
    shooter: u64,
    start: V3,
    end: V3,
    now: i32,
) -> Result<(), String> {
    let Some(runtime) = world.get_resource::<RoundScript>() else {
        return Ok(());
    };
    if runtime.profile != asset_core::RuntimeProfile::Campaign {
        return Ok(());
    }
    let Some(team) = runtime.sentients.get(&shooter).map(|s| s.team) else {
        return Ok(());
    };
    if !runtime
        .entities
        .get(&shooter)
        .is_some_and(|e| e.kind == super::super::entities::EntityKind::Player)
    {
        return Err("suppression bullet producer requires player-owned fire".into());
    }
    let settings = settings(world);
    let direction: V3 = std::array::from_fn(|i| start[i] - end[i]);
    let length = math_iw4::vec3_length(direction);
    let start = if length > 0. {
        std::array::from_fn(|i| start[i] + direction[i] * (200. / length))
    } else {
        start
    };
    let mut ids: Vec<_> = runtime
        .actors
        .iter()
        .filter(|(id, a)| **id != shooter && a.alive && a.physics_initialized)
        .map(|(&id, a)| (a.slot, id))
        .collect();
    ids.sort_unstable();
    for (_, id) in ids {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Some(target_team) = runtime.sentients.get(&id).map(|s| s.team) else {
            continue;
        };
        if target_team == 0 || target_team > 4 || (!settings.friendly && target_team == team) {
            continue;
        }
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let Some(point) = nearest(origin, start, end, settings.radius) else {
            continue;
        };
        let friendly = target_team == team;
        if friendly && settings.friendly_duration == 0 {
            continue;
        }
        let distance = (origin[0] - point[0]).hypot(origin[1] - point[1]);
        if !friendly {
            let sup = &mut runtime.actors.get_mut(&id).unwrap().suppression;
            sup.meter = (sup.meter + 0.15).min(1.);
        }
        notify(
            &mut runtime,
            id,
            "bulletwhizby",
            vec![Value::Object(shooter), Value::Float(distance)],
        );
        let plane = if friendly {
            let mut plane = [start[1] - end[1], end[0] - start[0], 0.];
            plane[2] = plane[0] * start[0] + plane[1] * start[1];
            if plane[2] < plane[0] * origin[0] + plane[1] * origin[1] {
                plane = plane.map(|v| -v);
            }
            plane
        } else {
            let Some((index, node)) = corner(&runtime, id, origin)? else {
                continue;
            };
            if (1..=3).contains(&target_team) {
                runtime.path_nodes.mark_dangerous(index, target_team, now)?;
            }
            let mut v = [start[0] - end[0], start[1] - end[1]];
            let length = v[0].hypot(v[1]);
            if length > 0. {
                v = v.map(|v| v / length);
            }
            if node.forward[0] * v[0] + node.forward[1] * v[1] < 0.5 {
                continue;
            }
            let sign = if node.kind == 7 { -1. } else { 1. };
            let normal = [sign * node.forward[1], -sign * node.forward[0]];
            [
                normal[0],
                normal[1],
                normal[0] * (node.origin[0] + normal[0] * 32.)
                    + normal[1] * (node.origin[1] + normal[1] * 32.),
            ]
        };
        let sup = &mut runtime.actors.get_mut(&id).unwrap().suppression;
        sup.add(shooter, plane, now, friendly);
        if !friendly {
            if sup.ignore == 0 {
                notify(
                    &mut runtime,
                    id,
                    "suppression",
                    vec![Value::Object(shooter)],
                );
            } else {
                notify(
                    &mut runtime,
                    id,
                    "bulletwhizby",
                    vec![Value::Object(shooter), Value::Float(distance)],
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn advance(world: &mut World, now: i32) {
    let duration = settings(world).friendly_duration;
    let mut runtime = world.resource_mut::<RoundScript>();
    let mut ended = Vec::new();
    for (&id, actor) in &mut runtime.actors {
        if actor.alive && actor.physics_initialized && actor.suppression.decay(now, duration) {
            ended.push((actor.slot, id));
        }
    }
    ended.sort_unstable();
    for (_, id) in ended {
        notify(&mut runtime, id, "suppression_end", vec![]);
    }
}

pub(crate) fn dissociate(runtime: &mut RoundScript, owner: u64) {
    let mut ended = Vec::new();
    for (&id, actor) in &mut runtime.actors {
        for e in &mut actor.suppression.entries {
            if e.suppressor == Some(owner) {
                e.clear();
            }
        }
        if actor.suppression.finish() {
            ended.push((actor.slot, id));
        }
    }
    ended.sort_unstable();
    for (_, id) in ended {
        notify(runtime, id, "suppression_end", vec![]);
    }
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "issuppressed", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        Ok(Value::Int(i32::from(
            world.resource::<RoundScript>().actors[&id]
                .suppression
                .suppressed(),
        )))
    });
    registry.register(
        Namespace::Method,
        "ismovesuppressed",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            Ok(Value::Int(i32::from(
                world.resource::<RoundScript>().actors[&id]
                    .suppression
                    .move_suppressed(),
            )))
        },
    );
    registry.register(
        Namespace::Method,
        "issuppressionwaiting",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            Ok(Value::Int(i32::from(
                world.resource::<RoundScript>().actors[&id]
                    .suppression
                    .waiting(now(world)),
            )))
        },
    );
}
