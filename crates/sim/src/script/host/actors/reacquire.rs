use super::super::args::{int, optional};
use super::*;
use glam::{Vec2, Vec3};

#[derive(Clone, Debug, Default)]
pub(crate) struct Reacquire {
    plan: Option<Plan>,
    likely: LikelyPath,
}

#[derive(Clone, Debug, Default)]
struct LikelyPath {
    target: Option<u64>,
    node: Option<u16>,
    angles: Option<[f32; 3]>,
    refresh_at: i32,
    search_at: i32,
    moving: bool,
}

impl LikelyPath {
    fn intervals(moving: bool) -> (i32, i32) {
        if moving { (250, 500) } else { (500, 3000) }
    }

    fn prepare(&mut self, target: u64, moving: bool, now: i32) {
        if self.target != Some(target) {
            *self = Self {
                target: Some(target),
                moving,
                ..Default::default()
            };
        } else if moving && !self.moving {
            let (refresh, search) = Self::intervals(true);
            self.refresh_at = self.refresh_at.min(now.saturating_add(refresh));
            self.search_at = self.search_at.min(now.saturating_add(search));
        }
        self.moving = moving;
    }

    fn searched(&mut self, node: Option<u16>, angles: Option<[f32; 3]>, now: i32) {
        let (refresh, search) = Self::intervals(self.moving);
        self.node = node;
        self.angles = angles;
        self.search_at = now.saturating_add(search);
        self.refresh_at = if node.is_some() {
            now.saturating_add(refresh)
        } else {
            self.search_at
        };
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Direct,
    Proximate,
    Step,
}
#[derive(Clone, Debug)]
struct Plan {
    target: u64,
    observed: [f32; 3],
    goal: goals::Goal,
    region: goals::Goal,
    script_goal: goals::Goal,
    goal_entity: Option<u64>,
    route: Option<navigation::Navigation>,
    active: bool,
    kind: Kind,
    ignore_suppression: bool,
}
impl Reacquire {
    pub(super) fn active_goal(&self) -> Option<&goals::Goal> {
        self.plan
            .as_ref()
            .filter(|plan| plan.active)
            .map(|plan| &plan.goal)
    }
    pub(super) fn face_enemy(&self) -> bool {
        self.plan
            .as_ref()
            .is_some_and(|plan| plan.active && plan.kind == Kind::Step)
    }
    pub(super) fn active(&self) -> bool {
        self.active_goal().is_some()
    }
}

const EXPOSED: u8 = 1;
const EXPOSED_IDLE: u16 = 100;
const REACQUIRE_MOVE: u16 = 102;
const REACQUIRE_DONE: u16 = 103;
const KNOWN_ENEMY_MS: i32 = 10_000;
const GOAL_CLIP_PRECISION: f32 = 0.5; // world units, below the minimum goal radius

pub(crate) fn clear(actor: &mut Actor, completed: bool) -> bool {
    let active = actor.reacquire.active();
    actor.reacquire.plan = None;
    if active {
        actor.navigation = Default::default();
        actor.move_mode = 0;
        actor.orientation.select_stop();
        if let Some(state) = actor
            .initial_state
            .as_mut()
            .filter(|state| state.state == EXPOSED)
        {
            state.substate = if completed {
                REACQUIRE_DONE
            } else {
                EXPOSED_IDLE
            };
        }
    }
    active
}
fn cancel(world: &mut World, id: u64, completed: bool) {
    if clear(
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap(),
        completed,
    ) {
        crate::script::runtime::raise(world, Value::Object(id), "movemode", vec![]);
    }
}
fn eligible(world: &World, id: u64) -> bool {
    let runtime = world.resource::<RoundScript>();
    let actor = &runtime.actors[&id];
    actor.alive
        && !actor.delayed_death
        && actor.physics_initialized
        && actor.scripted_control.is_none()
        && anim_mode::effective(actor) != anim_mode::Mode::PointRelative
        && !anim_mode::effective(actor).uses_root_motion()
        && runtime.entities[&id].linked_to.is_none()
        && actor
            .initial_state
            .as_ref()
            .is_none_or(|state| state.state == EXPOSED)
}
#[derive(Clone, Copy)]
struct Contact {
    target: u64,
    root: [f32; 3],
    aim: [f32; 3],
}
pub(super) fn known_position(world: &mut World, id: u64) -> Result<Option<[f32; 3]>, String> {
    contact(world, id).map(|contact| contact.map(|contact| contact.root))
}
fn contact(world: &mut World, id: u64) -> Result<Option<Contact>, String> {
    let Some(target) = targeting::enemy(world, id) else {
        return Ok(None);
    };
    if world
        .resource::<RoundScript>()
        .sentients
        .contains_key(&target)
    {
        if world
            .resource::<RoundScript>()
            .actors
            .get(&target)
            .is_some_and(|actor| !actor.alive || actor.delayed_death)
        {
            return Ok(None);
        }
        let visible = perception::can_see(world, id, target, 0)?;
        let now = super::super::players::now_ms(world) as i32;
        let info = world.resource::<RoundScript>().actors[&id]
            .sentient_info
            .get(&target)
            .cloned();
        let Some(info) = info else {
            return Ok(None);
        };
        let Some(root) = info
            .last_known_position
            .filter(|_| visible || now.saturating_sub(info.last_known_time) < KNOWN_ENEMY_MS)
        else {
            return Ok(None);
        };
        let aim = if visible {
            targeting::point(world, target)?
        } else {
            let (mins, maxs) = super::super::triggers::entity_bounds(world, target);
            std::array::from_fn(|i| root[i] + (mins[i] + maxs[i]) * 0.5)
        };
        return Ok(Some(Contact { target, root, aim }));
    }
    let Value::Vector(root) = super::super::players::entity_field(world, target, "origin") else {
        return Err("reacquire target has no origin".into());
    };
    Ok(Some(Contact {
        target,
        root,
        aim: targeting::point(world, target)?,
    }))
}
fn origin(world: &mut World, id: u64) -> Result<[f32; 3], String> {
    match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(position) if position.into_iter().all(f32::is_finite) => Ok(position),
        _ => Err("reacquire actor has invalid origin".into()),
    }
}
fn grounded(world: &mut World, id: u64, target: u64, position: [f32; 3]) -> Option<[f32; 3]> {
    let height = spatial::ACTOR_BOUNDS.half_size[2] * 2.0;
    let (mins, maxs) = spatial::ACTOR_BOUNDS.abs_bounds([0.; 3]);
    let hit = super::super::trace::capsule(
        world,
        movement_iw4::GroundTraceInput {
            start: [position[0], position[1], position[2] + height],
            end: [position[0], position[1], position[2] - height],
            mins,
            maxs,
            tracemask: world.resource::<RoundScript>().actors[&id]
                .motion
                .trace_mask,
        },
        &[id, target],
    );
    if hit.startsolid != 0
        || hit.allsolid != 0
        || hit.fraction >= 1.0
        || hit.normal[2] < std::f32::consts::FRAC_1_SQRT_2
    {
        return None;
    }
    Some([
        position[0],
        position[1],
        position[2] + height - height * 2.0 * hit.fraction,
    ])
}
fn line_clear(world: &mut World, id: u64, target: u64, from: [f32; 3], to: [f32; 3]) -> bool {
    let hit = super::super::trace::sight(
        world,
        from,
        to,
        crate::bullet_collision::MASK_SHOT,
        &[id, target],
    );
    hit.fraction == 1.0 && hit.startsolid == 0 && hit.allsolid == 0
}
fn firing_position(
    world: &mut World,
    id: u64,
    contact: Contact,
    from: [f32; 3],
    at: [f32; 3],
    require_weapon: bool,
) -> Result<bool, String> {
    let eye = perception::eye(world, id)?.0;
    if !line_clear(
        world,
        id,
        contact.target,
        std::array::from_fn(|i| at[i] + eye[i] - from[i]),
        contact.aim,
    ) {
        return Ok(false);
    }
    if require_weapon {
        if combat::weapon(world, id).is_none() {
            return Ok(false);
        }
        let Some((muzzle, _)) = combat::muzzle(world, id) else {
            return Ok(false);
        };
        return Ok(line_clear(
            world,
            id,
            contact.target,
            std::array::from_fn(|i| at[i] + muzzle[i] - from[i]),
            contact.aim,
        ));
    }
    Ok(true)
}
fn make_goal(position: [f32; 3]) -> goals::Goal {
    goals::Goal {
        position,
        ..Default::default()
    }
}
fn make_plan(
    world: &World,
    id: u64,
    contact: Contact,
    goal: goals::Goal,
    route: navigation::Navigation,
    kind: Kind,
    ignore_suppression: bool,
) -> Plan {
    let actor = &world.resource::<RoundScript>().actors[&id];
    Plan {
        target: contact.target,
        observed: contact.root,
        goal,
        region: actor.goals.code.clone(),
        script_goal: actor.goals.script.clone(),
        goal_entity: actor.goals.entity,
        route: Some(route),
        active: false,
        kind,
        ignore_suppression,
    }
}
// Clip to the authored goal with a half-unit spatial tolerance; retain the
// inside endpoint, including when a brush volume restricts the goal further.
fn clipped(
    world: &mut World,
    region: &goals::Goal,
    from: [f32; 3],
    to: [f32; 3],
) -> Result<Option<[f32; 3]>, String> {
    if goals::contains(world, region, to)? {
        return Ok(Some(to));
    }
    if !goals::contains(world, region, from)? {
        return Ok(None);
    }
    let distance = Vec3::from_array(from).distance(Vec3::from_array(to));
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    while (hi - lo) * distance > GOAL_CLIP_PRECISION {
        let mid = (lo + hi) * 0.5;
        if mid == lo || mid == hi {
            break;
        }
        if goals::contains(
            world,
            region,
            std::array::from_fn(|i| from[i] + (to[i] - from[i]) * mid),
        )? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(Some(std::array::from_fn(|i| {
        from[i] + (to[i] - from[i]) * lo
    })))
}
fn approach(
    world: &mut World,
    id: u64,
    contact: Contact,
    from: [f32; 3],
    region: &goals::Goal,
) -> Result<Option<([f32; 3], goals::Goal)>, String> {
    let delta = Vec2::new(contact.root[0] - from[0], contact.root[1] - from[1]);
    let distance = delta.length();
    let clearance = spatial::ACTOR_BOUNDS.half_size[0] * 2.0 + goals::Goal::default().radius;
    if distance <= clearance {
        return Ok(None);
    }
    let xy = delta / distance * (distance - clearance);
    let desired = [from[0] + xy.x, from[1] + xy.y, contact.root[2]];
    let Some(clipped) = clipped(world, region, from, desired)? else {
        return Ok(None);
    };
    let Some(at) = grounded(world, id, contact.target, clipped) else {
        return Ok(None);
    };
    let goal = make_goal(at);
    if !goals::contains(world, region, at)? || goals::contains(world, &goal, from)? {
        return Ok(None);
    }
    Ok(Some((at, goal)))
}
fn plan_matches(world: &World, id: u64, plan: &Plan) -> bool {
    let runtime = world.resource::<RoundScript>();
    let actor = &runtime.actors[&id];
    runtime.sentients[&id].target == Some(plan.target)
        && runtime.can_receive_call(&plan.target)
        && actor.goals.code == plan.region
        && actor.goals.script == plan.script_goal
        && actor.goals.entity == plan.goal_entity
}
fn same_contact(plan: &Plan, contact: Contact) -> bool {
    plan.target == contact.target
        && Vec3::from_array(plan.observed).distance(Vec3::from_array(contact.root))
            <= spatial::ACTOR_BOUNDS.half_size[0] * 2.0
}
fn prepare(
    world: &mut World,
    id: u64,
    proximate: bool,
    ignore_suppression: bool,
) -> Result<(), String> {
    let now = super::super::players::now_ms(world) as i32;
    goals::update_one(world, id, now)?;
    if !eligible(world, id) {
        cancel(world, id, false);
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .navigation = Default::default();
        return Ok(());
    }
    let observed = contact(world, id)?;
    let from = origin(world, id)?;
    let kind = if proximate {
        Kind::Proximate
    } else {
        Kind::Direct
    };
    if let (Some(contact), Some(plan)) = (
        observed,
        world.resource::<RoundScript>().actors[&id]
            .reacquire
            .plan
            .clone(),
    ) && !plan.active
        && plan.kind == kind
        && plan.ignore_suppression == ignore_suppression
        && plan.observed == contact.root
        && plan_matches(world, id, &plan)
        && let Some(route) = &plan.route
        && navigation::route_supported(world, id, route, from, plan.ignore_suppression)
        && (!proximate || firing_position(world, id, contact, from, plan.goal.position, false)?)
    {
        return Ok(());
    }
    cancel(world, id, false);
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .navigation = Default::default();
    let Some(contact) = observed else {
        return Ok(());
    };
    let from = origin(world, id)?;
    let region = world.resource::<RoundScript>().actors[&id]
        .goals
        .code
        .clone();
    if let Some((at, goal)) = approach(world, id, contact, from, &region)?
        && (!proximate || firing_position(world, id, contact, from, at, false)?)
        && let Some(route) = navigation::supported_plan(world, id, from, &goal, ignore_suppression)
    {
        let plan = make_plan(world, id, contact, goal, route, kind, ignore_suppression);
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .reacquire
            .plan = Some(plan);
        return Ok(());
    }
    if !proximate {
        return Ok(());
    }
    let Some(search) = navigation::RouteSearch::build(world, id, from, ignore_suppression) else {
        return Ok(());
    };
    let nodes = world.resource::<RoundScript>().path_nodes.nodes().to_vec();
    let mut candidates: Vec<_> = nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            search.cost(index as u16).map(|cost| {
                (
                    cost + Vec3::from_array(node.origin).distance(Vec3::from_array(contact.root)),
                    index,
                )
            })
        })
        .collect();
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    for (_, index) in candidates {
        let at = nodes[index].origin;
        let goal = make_goal(at);
        if goals::contains(world, &goal, from)?
            || !goals::contains(world, &region, at)?
            || !firing_position(world, id, contact, from, at, false)?
        {
            continue;
        }
        let Some(route) = search.to_node(world, index as u16, &goal) else {
            continue;
        };
        if !navigation::route_supported(world, id, &route, from, ignore_suppression) {
            continue;
        }
        let plan = make_plan(world, id, contact, goal, route, kind, ignore_suppression);
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .reacquire
            .plan = Some(plan);
        break;
    }
    Ok(())
}
fn start(world: &mut World, id: u64) -> Result<bool, String> {
    if !eligible(world, id) {
        return Ok(false);
    }
    let plan = world.resource::<RoundScript>().actors[&id]
        .reacquire
        .plan
        .clone();
    let Some(mut plan) = plan else {
        if world.resource::<RoundScript>().actors[&id]
            .navigation
            .point()
            .is_some()
        {
            return Err("ReacquireMove requires a path prepared by FindReacquireDirectPath or FindReacquireProximatePath".into());
        }
        return Ok(false);
    };
    if plan.active {
        return Err("ReacquireMove is already active; prepare a new path first".into());
    }
    let from = origin(world, id)?;
    let valid = plan_matches(world, id, &plan)
        && contact(world, id)?.is_some_and(|contact| same_contact(&plan, contact))
        && plan.route.as_ref().is_some_and(|route| {
            navigation::route_supported(world, id, route, from, plan.ignore_suppression)
        });
    if !valid || goals::contains(world, &plan.goal, from)? {
        cancel(world, id, false);
        return Ok(false);
    }
    let route = plan.route.take().ok_or("reacquire route is missing")?;
    plan.active = true;
    let changed = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime.actors.get_mut(&id).unwrap();
        arrival::clear(actor);
        cover::clear(actor);
        actor.navigation = route;
        actor.goals.keep_claimed = false;
        actor.goals.keep_claimed_if_valid = false;
        actor.orientation.select_reacquire(plan.kind == Kind::Step);
        let changed = actor.move_mode < 2;
        actor.move_mode = 2;
        if let Some(state) = actor
            .initial_state
            .as_mut()
            .filter(|state| state.state == EXPOSED)
        {
            state.substate = REACQUIRE_MOVE;
        }
        actor.reacquire.plan = Some(plan);
        changed
    };
    navigation::prepare(world, id, false)?;
    if changed {
        crate::script::runtime::raise(world, Value::Object(id), "movemode", vec![]);
    }
    Ok(true)
}
fn step(world: &mut World, id: u64, distance: f32) -> Result<bool, String> {
    if !distance.is_finite() {
        return Err("ReacquireStep distance must be finite".into());
    }
    if distance == 0.0
        || !eligible(world, id)
        || combat::weapon(world, id).is_none()
        || combat::muzzle(world, id).is_none()
    {
        return Ok(false);
    }
    let Some(contact) = contact(world, id)? else {
        return Ok(false);
    };
    let from = origin(world, id)?;
    let Some(toward) =
        Vec2::new(contact.root[0] - from[0], contact.root[1] - from[1]).try_normalize()
    else {
        return Ok(false);
    };
    let side = Vec2::new(-toward.y, toward.x) * distance;
    let now = super::super::players::now_ms(world) as i32;
    goals::update_one(world, id, now)?;
    let region = world.resource::<RoundScript>().actors[&id]
        .goals
        .code
        .clone();
    // Evaluate both sides geometrically. No retail sign-selection table or RNG.
    for direction in [side, -side] {
        let Some(at) = grounded(
            world,
            id,
            contact.target,
            [from[0] + direction.x, from[1] + direction.y, from[2]],
        ) else {
            continue;
        };
        if !goals::contains(world, &region, at)?
            || !firing_position(world, id, contact, from, at, true)?
        {
            continue;
        }
        let goal = make_goal(at);
        let Some(route) = navigation::supported_plan(world, id, from, &goal, false) else {
            continue;
        };
        cancel(world, id, false);
        let plan = make_plan(world, id, contact, goal, route, Kind::Step, false);
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .reacquire
            .plan = Some(plan);
        return start(world, id);
    }
    Ok(false)
}
pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_reacquire", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| actor.reacquire.plan.is_some().then_some(id))
        .collect();
    for id in ids {
        goals::update_one(world, id, now)?;
        let plan = world.resource::<RoundScript>().actors[&id]
            .reacquire
            .plan
            .clone()
            .unwrap();
        if !eligible(world, id)
            || !plan_matches(world, id, &plan)
            || !contact(world, id)?.is_some_and(|contact| same_contact(&plan, contact))
        {
            cancel(world, id, false);
            continue;
        }
        let from = origin(world, id)?;
        if plan.active && goals::contains(world, &plan.goal, from)? {
            cancel(world, id, true);
            continue;
        }
        let path = if plan.active {
            world.resource::<RoundScript>().actors[&id]
                .navigation
                .clone()
        } else {
            plan.route
                .clone()
                .ok_or("reacquire prepared route is missing")?
        };
        if !navigation::route_supported(world, id, &path, from, plan.ignore_suppression) {
            cancel(world, id, false);
        }
    }
    Ok(())
}
fn find<const PROXIMATE: bool>(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let ignore_suppression = optional(args, 0, int)?.unwrap_or(0) != 0;
    prepare(world, id, PROXIMATE, ignore_suppression)?;
    Ok(Value::Undefined)
}
pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "findreacquiredirectpath", find::<false>);
    registry.register(
        Namespace::Method,
        "findreacquireproximatepath",
        find::<true>,
    );
    registry.register(Namespace::Method, "reacquiremove", |world, receiver, _| {
        let id = actor_id(world, receiver)?;
        start(world, id).map(|success| Value::Int(success.into()))
    });
    registry.register(
        Namespace::Method,
        "reacquirestep",
        |world, receiver, args| {
            let id = actor_id(world, receiver)?;
            step(world, id, float(args, 0)?).map(|success| Value::Int(success.into()))
        },
    );
}

pub(super) fn likely_angles(world: &mut World, id: u64) -> Result<Option<[f32; 3]>, String> {
    let Some(target) = targeting::enemy(world, id) else {
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .reacquire
            .likely = LikelyPath::default();
        return Ok(None);
    };
    let now = super::super::players::now_ms(world) as i32;
    let cache = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime.actors.get_mut(&id).unwrap();
        let moving = actor.navigation.point().is_some();
        actor.reacquire.likely.prepare(target, moving, now);
        actor.reacquire.likely.clone()
    };
    if now < cache.refresh_at {
        return Ok(cache.angles);
    }
    let from = origin(world, id)?;
    if now < cache.search_at
        && let Some(node) = cache.node
        && let Some(observer) = super::super::sentients::nearest::query(world, id)?
        && world
            .resource::<RoundScript>()
            .path_nodes
            .visible(observer, node)
    {
        let point = world
            .resource::<RoundScript>()
            .path_nodes
            .at(node)
            .ok_or("likely path node is missing")?
            .origin;
        let angles = likely_point_angles(world, id, from, point)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        let cache = &mut runtime.actors.get_mut(&id).unwrap().reacquire.likely;
        cache.angles = Some(angles);
        cache.refresh_at = now.saturating_add(LikelyPath::intervals(cache.moving).0);
        return Ok(Some(angles));
    }
    let node = find_likely_node(world, id, target)?;
    let angles = node
        .map(|node| {
            let point = world
                .resource::<RoundScript>()
                .path_nodes
                .at(node)
                .ok_or("likely path node is missing")?
                .origin;
            likely_point_angles(world, id, from, point)
        })
        .transpose()?;
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .reacquire
        .likely
        .searched(node, angles, now);
    perf::instant!("likely_path_research", "iw4l.sim", "actor" => id,
        "target" => target, "time_ms" => now, "moving" => cache.moving,
        "found" => node.is_some(), "node" => node.map_or(-1, i64::from));
    Ok(angles)
}

fn likely_point_angles(
    world: &mut World,
    id: u64,
    from: [f32; 3],
    point: [f32; 3],
) -> Result<[f32; 3], String> {
    let delta = Vec3::from_array(point) - Vec3::from_array(from);
    if delta.truncate().length_squared() >= 1. {
        Ok(math_iw4::vect_to_angles(delta.to_array()))
    } else {
        match world
            .resource_mut::<RoundScript>()
            .object_field(id, "angles")
        {
            Value::Vector(angles) => Ok(angles),
            _ => Err("actor angles are not a vector".into()),
        }
    }
}

fn find_likely_node(world: &mut World, id: u64, target: u64) -> Result<Option<u16>, String> {
    let (position, team) = {
        let runtime = world.resource::<RoundScript>();
        let Some(sentient) = runtime.sentients.get(&target) else {
            return Ok(None);
        };
        let Some(position) = runtime.actors[&id]
            .sentient_info
            .get(&target)
            .and_then(|info| info.last_known_position)
        else {
            return Ok(None);
        };
        (position, sentient.team)
    };
    let Some(observer) = super::super::sentients::nearest::query(world, id)? else {
        return Ok(None);
    };
    let Some(start) = super::super::sentients::nearest::query_position(world, position) else {
        return Ok(None);
    };
    let runtime = world.resource::<RoundScript>();
    Ok(runtime
        .path_nodes
        .facing_node(start, observer, team, |from, to| {
            if runtime.path_nodes.nodes()[usize::from(from)].kind == 16
                && runtime.path_nodes.nodes()[usize::from(to)].kind == 17
            {
                cover::claimable(runtime, target, from) && cover::claimable(runtime, target, to)
            } else {
                true
            }
        }))
}
