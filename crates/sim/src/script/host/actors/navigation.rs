use super::*;

mod nodes;
mod planner;
mod preview;
mod routes;
use preview::Preview;
pub(super) use routes::{RouteSearch, direct_supported, supported_plan};

pub(super) fn register(registry: &mut NativeRegistry) {
    nodes::register(registry);
}

pub(super) const FIELDS: &[&str] = &[
    "lookaheaddir",
    "lookaheaddist",
    "lookaheadhitsstairs",
    "leanamount",
];

#[derive(Clone, Debug, Default)]
pub(crate) struct Navigation {
    goal: Option<goals::Goal>,
    points: Vec<[f32; 3]>,
    nodes: Vec<u16>,
    links: Vec<(u16, u16)>,
    cursor: usize,
    preview: Preview,
    pending: Option<Box<planner::Search>>,
}

impl Navigation {
    pub(super) fn load(&self, field: &str) -> Option<Value> {
        match field {
            "lookaheaddir" => Some(Value::Vector(self.preview.direction)),
            "lookaheaddist" => Some(Value::Float(self.preview.distance)),
            "lookaheadhitsstairs" => Some(Value::Int(i32::from(self.preview.stairs))),
            "leanamount" => Some(Value::Float(self.preview.lean)),
            _ => None,
        }
    }
    pub(super) fn store(&mut self, field: &str, value: &Value) -> Result<bool, String> {
        if !FIELDS.contains(&field) {
            return Ok(false);
        }
        if field != "leanamount" {
            return Err(format!("actor {field} is read-only"));
        }
        let lean = float(std::slice::from_ref(value), 0)?;
        if !lean.is_finite() || !(-1.0..=1.0).contains(&lean) {
            return Err("actor leanamount must be finite and between -1 and 1".into());
        }
        self.preview.lean = lean;
        Ok(true)
    }
    pub(super) fn steering_point(&self) -> Option<[f32; 3]> {
        self.preview.target
    }
    pub(super) fn door(&self, nodes: &super::super::path_nodes::PathNodes) -> Option<u16> {
        self.nodes
            .iter()
            .skip(self.cursor)
            .copied()
            .find(|&index| matches!(nodes.nodes()[usize::from(index)].kind, 13 | 14))
    }
    pub(super) fn negotiation(
        &self,
        nodes: &super::super::path_nodes::PathNodes,
    ) -> Option<(u16, u16)> {
        self.links
            .iter()
            .skip(self.cursor.saturating_sub(1))
            .find_map(|&(from, to)| {
                nodes.links[usize::from(from)]
                    .iter()
                    .any(|link| link.node == to && link.negotiation && link.disconnect_count == 0)
                    .then_some((from, to))
            })
    }
    pub(super) fn point(&self) -> Option<[f32; 3]> {
        self.points.get(self.cursor).copied()
    }
    pub(super) fn end(&self) -> Option<[f32; 3]> {
        self.points.last().copied()
    }
    pub(super) fn remaining_points(&self) -> &[[f32; 3]] {
        &self.points[self.cursor..]
    }
}

pub(super) fn supported_target(
    world: &mut World,
    id: u64,
    origin: [f32; 3],
    target: [f32; 3],
) -> Option<bool> {
    super::super::presence::settle_collision(world);
    supported_target_settled(world, id, origin, target)
}

// A route search only reads the scene: reuse its settled collision for every edge.
fn supported_target_settled(
    world: &mut World,
    id: u64,
    origin: [f32; 3],
    target: [f32; 3],
) -> Option<bool> {
    if !bad_places::allows_segment(world, id, origin, target) {
        return None;
    }
    let mask = world.resource::<RoundScript>().actors[&id]
        .motion
        .trace_mask;
    let (mins, maxs) = spatial::ACTOR_BOUNDS.abs_bounds([0.0; 3]);
    preview::supported(origin, target, mins, maxs, mask, |input| {
        super::super::trace::capsule_settled(world, input, &[id])
    })
}

pub(super) fn authored_edge_allowed(
    world: &mut World,
    id: u64,
    from: u16,
    to: u16,
    ignore_suppression: bool,
    allow_negotiation: bool,
) -> bool {
    let (start, end, negotiation) = {
        let runtime = world.resource::<RoundScript>();
        let graph = &runtime.path_nodes;
        let Some(link) = graph.links.get(usize::from(from)).and_then(|links| {
            links
                .iter()
                .find(|link| link.node == to && link.disconnect_count == 0)
        }) else {
            return false;
        };
        let Some(start) = graph.at(from) else {
            return false;
        };
        let Some(end) = graph.at(to) else {
            return false;
        };
        (start.origin, end.origin, link.negotiation)
    };
    if negotiation {
        allow_negotiation
            && traversal::authored_pair(world, from, to)
            && bad_places::allows_segment(world, id, start, end)
            && (ignore_suppression
                || !world.resource::<RoundScript>().actors[&id]
                    .suppression
                    .blocks_path(start, end))
    } else {
        (ignore_suppression
            || !world.resource::<RoundScript>().actors[&id]
                .suppression
                .blocks_path(start, end))
            && bad_places::allows_segment(world, id, start, end)
    }
}

fn visible(world: &mut World, id: u64, a: [f32; 3], b: [f32; 3]) -> bool {
    let presence = world.resource::<RoundScript>().entities[&id].presence;
    let hit = super::super::presence::settled(world).trace_world_hull_except(
        movement_iw4::GroundTraceInput {
            start: [a[0], a[1], a[2] + 36.0],
            end: [b[0], b[1], b[2] + 36.0],
            mins: [0.0; 3],
            maxs: [0.0; 3],
            tracemask: 0x02820015,
        },
        presence.map(crate::AuthorityModelOwner::ScriptModel),
    );
    hit.fraction == 1.0 && hit.startsolid == 0
}

pub(crate) fn nearest(world: &mut World, id: u64, position: [f32; 3]) -> Option<u16> {
    let mut candidates: Vec<_> = world
        .resource::<RoundScript>()
        .path_nodes
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, node)| !node.links.is_empty())
        .map(|(index, node)| {
            (
                glam::Vec3::from_array(position)
                    .distance_squared(glam::Vec3::from_array(node.origin)),
                index as u16,
                node.origin,
            )
        })
        .collect();
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    candidates
        .into_iter()
        .find(|(_, _, origin)| visible(world, id, position, *origin))
        .map(|(_, index, _)| index)
}

fn plan(world: &mut World, id: u64, origin: [f32; 3], goal: &goals::Goal) -> Navigation {
    let route = if grenades::movement_goal(&world.resource::<RoundScript>().actors[&id]).is_some() {
        supported_plan(world, id, origin, goal, true)
    } else if traversal::can_plan(&world.resource::<RoundScript>().actors[&id]) {
        return planner::advance(world, id, origin, goal);
    } else {
        supported_plan(world, id, origin, goal, false)
    };
    if let Some(route) = route {
        return route;
    }
    Navigation {
        goal: Some(goal.clone()),
        ..Default::default()
    }
}

pub(super) fn prepare(world: &mut World, id: u64, at_goal: bool) -> Result<(), String> {
    let _span = perf::span_handle!("actor_navigation_prepare", "iw4l.sim").enter();
    let (origin, goal, mut valid, mut preview, velocity) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        let actor = &runtime.actors[&id];
        let path = &actor.navigation;
        let valid = path.point().is_some()
            && path.goal.as_ref() == Some(goals::movement_goal(actor))
            && path
                .links
                .iter()
                .skip(path.cursor.saturating_sub(1))
                .all(|&(from, to)| {
                    runtime.path_nodes.links[usize::from(from)]
                        .iter()
                        .any(|link| link.node == to && link.disconnect_count == 0)
                });
        (
            origin,
            goals::movement_goal(actor).clone(),
            valid,
            path.preview,
            actor.motion.velocity,
        )
    };
    if valid && !world.resource::<RoundScript>().bad_places.is_empty() {
        let path = world.resource::<RoundScript>().actors[&id]
            .navigation
            .clone();
        let ignore_suppression =
            grenades::movement_goal(&world.resource::<RoundScript>().actors[&id]).is_some();
        // The retained route was collision-checked by its planner. A bad place
        // can invalidate its segments, but does not change their floor support.
        // Keep checking hazards along the whole route; the steering preview below
        // checks the current collision scene before permitting movement.
        let mut at = origin;
        valid = path.remaining_points().iter().all(|&next| {
            let allowed = (ignore_suppression
                || !world.resource::<RoundScript>().actors[&id]
                    .suppression
                    .blocks_path(at, next))
                && bad_places::allows_segment(world, id, at, next);
            at = next;
            allowed
        });
    }
    if at_goal {
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .navigation = Navigation {
            goal: Some(goal),
            ..Default::default()
        };
    } else if !valid {
        let path = plan(world, id, origin, &goal);
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .navigation = path;
    }
    let boundary = {
        let r = world.resource::<RoundScript>();
        let path = &r.actors[&id].navigation;
        path.links
            .iter()
            .enumerate()
            .skip(path.cursor.saturating_sub(1))
            .find_map(|(i, &(from, to))| {
                r.path_nodes.links[usize::from(from)]
                    .iter()
                    .any(|l| l.node == to && l.negotiation && l.disconnect_count == 0)
                    .then_some(i)
            })
    };
    let route = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let path = &mut runtime.actors.get_mut(&id).unwrap().navigation;
        while path.cursor + 1 < path.points.len() && boundary.is_none_or(|i| path.cursor < i) {
            let target = path.points[path.cursor];
            let close = (origin[0] - target[0]).hypot(origin[1] - target[1]) < 16.0
                && (origin[2] - target[2]).abs() <= 80.0;
            // Pursuit previews can cut a corner and pass the waypoint without
            // entering its arrival circle. Advance when the actor is already
            // inside the body-width corridor of the outgoing route segment.
            let next = path.points[path.cursor + 1];
            let segment = glam::Vec2::new(next[0] - target[0], next[1] - target[1]);
            let offset = glam::Vec2::new(origin[0] - target[0], origin[1] - target[1]);
            let length_sq = segment.length_squared();
            let passed = if length_sq > f32::EPSILON {
                let projection = offset.dot(segment) / length_sq;
                let fraction = projection.clamp(0.0, 1.0);
                projection > 0.0
                    && offset.distance_squared(segment * fraction)
                        <= spatial::ACTOR_BOUNDS.half_size[0].powi(2)
                    && (origin[2] - target[2] - (next[2] - target[2]) * fraction).abs() <= 80.0
            } else {
                false
            };
            if !close && !passed {
                break;
            }
            path.cursor += 1;
        }
        let end = boundary.map_or(path.points.len(), |i| i + 1);
        path.points[path.cursor..end].to_vec()
    };
    super::super::presence::settle_collision(world);
    let before_preview = preview;
    preview.follow(origin, &route, velocity, |target| {
        supported_target_settled(world, id, origin, target)
    });
    if preview.target.is_none() {
        let rejoin = {
            let runtime = world.resource::<RoundScript>();
            let path = &runtime.actors[&id].navigation;
            path.cursor.checked_sub(1).and_then(|previous| {
                rejoin_target(
                    origin,
                    path.points[previous],
                    *path.points.get(path.cursor)?,
                )
            })
        };
        if let Some(rejoin) = rejoin {
            let mut recovery = Vec::with_capacity(route.len() + 1);
            recovery.push(rejoin);
            recovery.extend_from_slice(&route);
            preview = before_preview;
            preview.follow(origin, &recovery, velocity, |target| {
                supported_target_settled(world, id, origin, target)
            });
        }
    }
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .navigation
        .preview = preview;
    Ok(())
}

// Physical steering can leave the retained segment while rounding a corner.
// Reconnect to its nearest point before retrying the forward corridor. The
// preview still validates every candidate with the ordinary body/support trace.
pub(super) fn rejoin_target(origin: [f32; 3], from: [f32; 3], to: [f32; 3]) -> Option<[f32; 3]> {
    let start = glam::Vec2::new(from[0], from[1]);
    let segment = glam::Vec2::new(to[0], to[1]) - start;
    let length_sq = segment.length_squared();
    if length_sq <= f32::EPSILON {
        return None;
    }
    let offset = glam::Vec2::new(origin[0], origin[1]) - start;
    let fraction = (offset.dot(segment) / length_sq).clamp(0.0, 1.0);
    let point: [f32; 3] = std::array::from_fn(|i| from[i] + (to[i] - from[i]) * fraction);
    ((point[0] - origin[0]).hypot(point[1] - origin[1]) > f32::EPSILON).then_some(point)
}

pub(super) fn wish_delta(world: &mut World, id: u64) -> [f32; 3] {
    let mut runtime = world.resource_mut::<RoundScript>();
    let Value::Vector(origin) = runtime.object_field(id, "origin") else {
        return [0.0; 3];
    };
    let actor = &runtime.actors[&id];
    if actor.move_mode < 2 {
        return [0.0; 3];
    }
    let Some(target) = actor.navigation.steering_point() else {
        return [0.0; 3];
    };
    let step = runtime
        .animators
        .get(&id)
        .and_then(|anim| {
            anim.runtime
                .relative_translation(xmodel_runtime::XAnimNodeId(0))
        })
        .unwrap_or([0.0; 3]);
    let length = step[0].hypot(step[1]);
    let delta = glam::Vec2::new(target[0] - origin[0], target[1] - origin[1]);
    let xy = delta.normalize_or_zero() * length.min(delta.length());
    [xy.x, xy.y, 0.0]
}

pub(super) fn route_supported(
    world: &mut World,
    id: u64,
    path: &Navigation,
    origin: [f32; 3],
    ignore_suppression: bool,
) -> bool {
    route_supported_with_negotiation(world, id, path, origin, ignore_suppression, false)
}
fn route_supported_with_negotiation(
    world: &mut World,
    id: u64,
    path: &Navigation,
    origin: [f32; 3],
    ignore_suppression: bool,
    allow_negotiation: bool,
) -> bool {
    super::super::presence::settle_collision(world);
    let graph = &world.resource::<RoundScript>().path_nodes;
    if path
        .links
        .iter()
        .skip(path.cursor.saturating_sub(1))
        .any(|&(from, to)| {
            !graph.links.get(usize::from(from)).is_some_and(|links| {
                links.iter().any(|link| {
                    link.node == to
                        && link.disconnect_count == 0
                        && (!link.negotiation
                            || allow_negotiation && traversal::authored_pair(world, from, to))
                })
            })
        })
    {
        return false;
    }
    let mut at = origin;
    let Some(points) = path.points.get(path.cursor..) else {
        return false;
    };
    if points.is_empty() {
        return false;
    }
    for (offset, &next) in points.iter().enumerate() {
        let index = path.cursor + offset;
        let edge = index.checked_sub(1).and_then(|previous| {
            let (&from, &to) = path.nodes.get(previous).zip(path.nodes.get(index))?;
            let graph = &world.resource::<RoundScript>().path_nodes;
            (path.links.get(previous) == Some(&(from, to))
                && graph.at(from).is_some_and(|node| node.origin == at)
                && graph.at(to).is_some_and(|node| node.origin == next))
            .then_some((from, to))
        });
        let negotiation = edge.is_some_and(|(from, to)| {
            allow_negotiation
                && world.resource::<RoundScript>().path_nodes.links[usize::from(from)]
                    .iter()
                    .any(|link| link.node == to && link.negotiation)
        });
        let usable = if let Some((from, to)) = edge.filter(|_| offset > 0 || negotiation) {
            authored_edge_allowed(world, id, from, to, ignore_suppression, allow_negotiation)
        } else {
            (ignore_suppression
                || !world.resource::<RoundScript>().actors[&id]
                    .suppression
                    .blocks_path(at, next))
                && supported_target_settled(world, id, at, next).is_some()
        };
        if !usable {
            return false;
        }
        at = next;
    }
    true
}
