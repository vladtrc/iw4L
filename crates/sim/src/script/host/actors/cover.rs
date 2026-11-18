use super::*;
use glam::{Vec2, Vec3};

#[derive(Clone, Debug, Default)]
pub(crate) struct Cover {
    claim: Option<Claim>,
    previous: Option<u16>,
}
#[derive(Clone, Debug)]
struct Claim {
    goal: goals::Goal,
    script_goal: goals::Goal,
    goal_entity: Option<u64>,
    arrived: bool,
}
impl Cover {
    pub(super) fn previous_node(&self) -> Option<u16> {
        self.previous
    }
    pub(super) fn holding(&self) -> bool {
        self.claim.as_ref().is_some_and(|claim| claim.arrived)
    }
    pub(super) fn movement_goal(&self) -> Option<&goals::Goal> {
        self.claim.as_ref().map(|claim| &claim.goal)
    }
}

// These names select authored GSC behavior, rather than retail attack-script
// pointers or scoring tables. Nodes with unsupported behavior are not offered.
fn behavior(kind: u32) -> Option<scripted::ControlKind> {
    use scripted::ControlKind::*;
    Some(match kind {
        1 | 8..=12 | 19 => Combat,
        2 => CoverStand,
        3 | 4 => CoverCrouch,
        5 => CoverProne,
        6 => CoverRight,
        7 => CoverLeft,
        _ => return None,
    })
}
fn eligible(world: &World, id: u64) -> bool {
    let r = world.resource::<RoundScript>();
    let actor = &r.actors[&id];
    r.can_receive_call(&id)
        && actor.alive
        && !actor.delayed_death
        && actor.physics_initialized
        && actor.species == Species::Human
        && actor.scripted_control.is_none()
        && anim_mode::effective(actor) != anim_mode::Mode::PointRelative
        && (!anim_mode::effective(actor).uses_root_motion()
            || actor.anim_script_kind.is_some_and(|kind| kind.is_cover()))
        && r.entities[&id].linked_to.is_none()
        && actor
            .initial_state
            .as_ref()
            .is_none_or(|state| matches!(state.state, 1 | 2))
}
fn origin(world: &mut World, id: u64) -> Result<[f32; 3], String> {
    match world
        .resource_mut::<RoundScript>()
        .object_field(id, "origin")
    {
        Value::Vector(at) if at.into_iter().all(f32::is_finite) => Ok(at),
        _ => Err("cover requires a finite actor origin".into()),
    }
}
fn conflict(r: &RoundScript, a: u16, b: u16) -> bool {
    a == b
        || [(a, b), (b, a)].into_iter().any(|(from, to)| {
            r.path_nodes.at(from).is_some_and(|node| {
                node.overlap_nodes
                    .iter()
                    .take_while(|&&n| n >= 0)
                    .any(|&n| n as u16 == to)
            })
        })
}
pub(super) fn claimable(r: &RoundScript, id: u64, index: u16) -> bool {
    r.actors.iter().all(|(&owner, actor)| {
        owner == id
            || !actor.alive
            || actor.delayed_death
            || !r.can_receive_call(&owner)
            || actor
                .claimed_node
                .is_none_or(|other| !conflict(r, index, other))
    })
}
fn danger(r: &RoundScript, id: u64, index: u16, now: i32, severe: bool) -> bool {
    let Some(team) = r
        .sentients
        .get(&id)
        .and_then(|s| s.team.checked_sub(1))
        .filter(|&team| team < 3)
    else {
        return false;
    };
    r.path_nodes
        .dangerous_until
        .get(usize::from(index))
        .is_some_and(|until| {
            let remaining = until[usize::from(team)].wrapping_sub(now);
            // The native distinguishes a normal mark from a repeated, extended
            // danger mark: the latter must outlive a full normal 15-second window.
            remaining > if severe { 15_000 } else { 0 }
        })
}
fn faces_enemy(world: &World, index: u16, enemy: Option<[f32; 3]>) -> bool {
    let Some(enemy) = enemy else {
        return true;
    };
    let node = world
        .resource::<RoundScript>()
        .path_nodes
        .at(index)
        .unwrap();
    let toward = Vec2::new(enemy[0] - node.origin[0], enemy[1] - node.origin[1]);
    if toward.length_squared() < node.min_use_dist_sq {
        return false;
    }
    if !matches!(node.kind, 2..=7) {
        return true;
    }
    let (sin, cos) = node.yaw.to_radians().sin_cos();
    let forward = if node.forward == [0.; 2] {
        Vec2::new(cos, sin)
    } else {
        Vec2::from_array(node.forward)
    };
    // Our cover model uses the authored front hemisphere. Detailed retail
    // type-specific angle/range tables are deliberately not transplanted.
    toward.dot(forward) > 0.
}
fn valid(
    world: &mut World,
    id: u64,
    index: u16,
    enemy: Option<[f32; 3]>,
    now: i32,
) -> Result<bool, String> {
    let r = world.resource::<RoundScript>();
    let Some(node) = r.path_nodes.at(index) else {
        return Ok(false);
    };
    let Some(kind) = behavior(node.kind) else {
        return Ok(false);
    };
    let actor = &r.actors[&id];
    let supported = r.program.as_ref().is_some_and(|p| {
        p.names
            .contains_key(&format!("animscripts/{}::main", kind.name()))
    });
    let stances = r.path_nodes.allowed_stances(index).unwrap_or(7) & actor.allowed_stances;
    if !supported
        || stances == 0
        || !claimable(r, id, index)
        || danger(r, id, index, now, false)
        || !faces_enemy(world, index, enemy)
    {
        return Ok(false);
    }
    let at = node.origin;
    let region = actor.goals.code.clone();
    goals::contains(world, &region, at)
}
fn node_goal(at: [f32; 3], index: u16) -> goals::Goal {
    goals::Goal {
        position: at,
        node: Some(index),
        height: spatial::ACTOR_BOUNDS.half_size[2],
        ..Default::default()
    }
}
fn select(world: &mut World, id: u64, shuffle: bool) -> Result<Value, String> {
    if !eligible(world, id) {
        return Ok(Value::Undefined);
    }
    let now = super::super::players::now_ms(world) as i32;
    goals::update_one(world, id, now)?;
    let enemy = reacquire::known_position(world, id)?;
    let from = origin(world, id)?;
    let r = world.resource::<RoundScript>();
    let previous = r.actors[&id].claimed_node;
    let neighbors = if shuffle {
        let Some(previous) = previous else {
            return Ok(Value::Undefined);
        };
        r.path_nodes.links[usize::from(previous)]
            .iter()
            .filter(|link| link.disconnect_count == 0 && !link.negotiation)
            .map(|link| link.node)
            .collect::<Vec<_>>()
    } else {
        (0..r.path_nodes.nodes().len()).map(|i| i as u16).collect()
    };
    let actor = &r.actors[&id];
    let mut candidates: Vec<_> = neighbors
        .into_iter()
        .filter(|node| !shuffle || Some(*node) != previous)
        .filter(|&index| {
            actor.combat_mode != "ambush_nodes_only" || r.path_nodes.at(index).unwrap().kind == 8
        })
        .map(|index| {
            let node = r.path_nodes.at(index).unwrap();
            let preferred = if actor.combat_mode == "no_cover" {
                matches!(node.kind, 1 | 9 | 19)
            } else if actor.doing_ambush {
                node.kind == 8
            } else {
                matches!(node.kind,2..=7 | 10..=12)
            };
            (
                !preferred,
                Vec3::from_array(node.origin).distance_squared(Vec3::from_array(from)),
                index,
            )
        })
        .collect();
    // Prefer the requested gameplay role, then proximity and authored order.
    // A nearby ordinary path node must not crowd out reachable authored cover.
    // No retail scoring coefficients or attack-script pointer tables are used.
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut search = None;
    let mut searched = false;
    for (_, _, index) in candidates {
        if !valid(world, id, index, enemy, now)? {
            continue;
        }
        let at = world
            .resource::<RoundScript>()
            .path_nodes
            .at(index)
            .unwrap()
            .origin;
        if navigation::direct_supported(world, id, from, at, false) {
            return Ok(world.resource::<RoundScript>().path_nodes.value(index));
        }
        if !searched {
            search = navigation::RouteSearch::build(world, id, from, false);
            searched = true;
        }
        if search
            .as_ref()
            .is_some_and(|search| search.cost(index).is_some())
        {
            return Ok(world.resource::<RoundScript>().path_nodes.value(index));
        }
    }
    Ok(Value::Undefined)
}
pub(super) fn clear(actor: &mut Actor) {
    if actor.cover.claim.take().is_some() {
        actor.navigation = Default::default();
        actor.move_mode = 0;
    }
    actor.cover.previous = None;
    actor.claimed_node = None;
    actor.at_claim_node = false;
}
pub(super) fn at_claim(world: &mut World, id: u64) -> Result<bool, String> {
    let Some(index) = world.resource::<RoundScript>().actors[&id].claimed_node else {
        return Ok(false);
    };
    let Some(node) = world.resource::<RoundScript>().path_nodes.at(index) else {
        return Ok(false);
    };
    let goal = node_goal(node.origin, index);
    let at = origin(world, id)?;
    goals::contains(world, &goal, at)
}
pub(super) fn stop_behavior(
    world: &mut World,
    id: u64,
) -> Result<Option<scripted::ControlKind>, String> {
    if !world.resource::<RoundScript>().actors[&id].cover.holding() {
        return Ok(None);
    }
    let r = world.resource::<RoundScript>();
    Ok(r.actors[&id]
        .claimed_node
        .and_then(|index| r.path_nodes.at(index))
        .and_then(|node| behavior(node.kind)))
}
fn use_node(world: &mut World, id: u64, index: u16) -> Result<bool, String> {
    let now = super::super::players::now_ms(world) as i32;
    goals::update_one(world, id, now)?;
    let region = world.resource::<RoundScript>().actors[&id]
        .goals
        .code
        .clone();
    if matches!(world.resource_mut::<RoundScript>().object_field(id,"fixednode"), Value::Int(value) if value!=0)
    {
        return Err("UseCoverNode cannot replace a fixed node".into());
    }
    let keep = {
        let actor = &world.resource::<RoundScript>().actors[&id];
        (
            actor.goals.keep_claimed,
            actor.goals.keep_claimed_if_valid,
            actor.claimed_node,
        )
    };
    let kept_in_goal = if let (false, true, Some(old)) = keep {
        let at = world
            .resource::<RoundScript>()
            .path_nodes
            .at(old)
            .ok_or("claimed node missing")?
            .origin;
        goals::contains(world, &region, at)?
    } else {
        false
    };
    if keep.0 || kept_in_goal {
        return Err("UseCoverNode cannot replace a kept claimed node".into());
    }
    if !eligible(world, id) {
        return Ok(false);
    }
    let enemy = reacquire::known_position(world, id)?;
    if !valid(world, id, index, enemy, now)? {
        return Ok(false);
    }
    let from = origin(world, id)?;
    let at = world
        .resource::<RoundScript>()
        .path_nodes
        .at(index)
        .unwrap()
        .origin;
    let goal = node_goal(at, index);
    let Some(route) = navigation::supported_plan(world, id, from, &goal, false) else {
        return Ok(false);
    };
    {
        let mut r = world.resource_mut::<RoundScript>();
        let actor = r.actors.get_mut(&id).unwrap();
        let previous = actor.claimed_node;
        arrival::clear(actor);
        reacquire::clear(actor, false);
        clear(actor);
        actor.cover.previous = previous;
        actor.claimed_node = Some(index);
        actor.cover.claim = Some(Claim {
            goal,
            script_goal: actor.goals.script.clone(),
            goal_entity: actor.goals.entity,
            arrived: false,
        });
        actor.navigation = route;
        if let Some(state) = actor.initial_state.as_mut() {
            state.state = 1;
            state.substate = 100;
        }
    }
    let arrived = at_claim(world, id)?;
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .at_claim_node = arrived;
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .cover
        .claim
        .as_mut()
        .unwrap()
        .arrived = arrived;
    navigation::prepare(world, id, arrived)?;
    Ok(true)
}
pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_cover", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, a)| a.cover.claim.as_ref().map(|_| id))
        .collect();
    for id in ids {
        goals::update_one(world, id, now)?;
        let claim = world.resource::<RoundScript>().actors[&id]
            .cover
            .claim
            .clone()
            .unwrap();
        let current = world.resource::<RoundScript>().actors[&id].claimed_node;
        let region = world.resource::<RoundScript>().actors[&id]
            .goals
            .code
            .clone();
        let in_goal = goals::contains(world, &region, claim.goal.position)?;
        let actor = &world.resource::<RoundScript>().actors[&id];
        let kept = actor.goals.keep_claimed || (actor.goals.keep_claimed_if_valid && in_goal);
        let changed =
            claim.script_goal != actor.goals.script || claim.goal_entity != actor.goals.entity;
        if (actor.scripted_control.is_some() && kept)
            || (actor.arrival.active()
                && actor.alive
                && current == claim.goal.node
                && (!changed || kept))
        {
            let at = at_claim(world, id)?;
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .at_claim_node = at;
            continue;
        }
        let valid = eligible(world, id)
            && current == claim.goal.node
            && (!changed || kept)
            && (in_goal || actor.goals.keep_claimed);
        if !valid {
            clear(
                world
                    .resource_mut::<RoundScript>()
                    .actors
                    .get_mut(&id)
                    .unwrap(),
            );
            continue;
        }
        let at = at_claim(world, id)?;
        world
            .resource_mut::<RoundScript>()
            .actors
            .get_mut(&id)
            .unwrap()
            .at_claim_node = at;
        if at {
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .cover
                .claim
                .as_mut()
                .unwrap()
                .arrived = true;
        }
        if !at && !claim.arrived {
            let from = origin(world, id)?;
            let route = world.resource::<RoundScript>().actors[&id]
                .navigation
                .clone();
            if !navigation::route_supported(world, id, &route, from, false) {
                clear(
                    world
                        .resource_mut::<RoundScript>()
                        .actors
                        .get_mut(&id)
                        .unwrap(),
                );
            }
        }
    }
    Ok(())
}
pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "findbestcovernode", |w, r, _| {
        let id = actor_id(w, r)?;
        select(w, id, false)
    });
    registry.register(Namespace::Method, "findshufflecovernode", |w, r, _| {
        let id = actor_id(w, r)?;
        select(w, id, true)
    });
    registry.register(Namespace::Method, "usecovernode", |w, r, args| {
        let id = actor_id(w, r)?;
        let node = w
            .resource::<RoundScript>()
            .path_nodes
            .index(arg(args, 0)?)?;
        use_node(w, id, node).map(|ok| Value::Int(ok.into()))
    });
    registry.register(Namespace::Method, "atdangerousnode", |w, r, _| {
        let id = actor_id(w, r)?;
        let now = super::super::players::now_ms(w) as i32;
        let at = at_claim(w, id)?;
        let r = w.resource::<RoundScript>();
        Ok(Value::Int(i32::from(
            at && r.actors[&id]
                .claimed_node
                .is_some_and(|node| danger(r, id, node, now, true)),
        )))
    });
    registry.register(Namespace::Method, "iscovervalidagainstenemy", |w, r, _| {
        let id = actor_id(w, r)?;
        let Some(index) = w.resource::<RoundScript>().actors[&id].claimed_node else {
            return Ok(Value::Int(0));
        };
        let Some(enemy) = reacquire::known_position(w, id)? else {
            return Ok(Value::Int(0));
        };
        Ok(Value::Int(faces_enemy(w, index, Some(enemy)).into()))
    });
}
