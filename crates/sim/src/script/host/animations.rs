use super::{
    args::{arg, float},
    entities::EntityKind,
    natives::engine::entity_id,
};
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;
use std::sync::Arc;
use xmodel_runtime::{
    XAnimNodeId, XAnimNodeKind, XAnimSyncTree, XAnimTreeDefinition, XAnimTreeRuntime,
};

#[derive(Clone, Debug)]
pub struct ScriptAnimTree {
    pub names: Arc<[String]>,
    pub definition: Arc<XAnimTreeDefinition>,
    pub flags: Arc<[u16]>,
    pub sync: Arc<XAnimSyncTree>,
}
impl ScriptAnimTree {
    pub fn new(
        names: Vec<String>,
        definition: Arc<XAnimTreeDefinition>,
        flags: Vec<u16>,
    ) -> Result<Self, String> {
        if names.len() != definition.nodes().len() || flags.len() != names.len() {
            return Err("animation names and tree topology differ".into());
        }
        let sync =
            Arc::new(XAnimSyncTree::new(definition.clone(), &flags).map_err(|e| e.to_string())?);
        Ok(Self {
            sync,
            names: names.into(),
            definition,
            flags: flags.into(),
        })
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Animator {
    pub name: String,
    pub data: ScriptAnimTree,
    pub runtime: XAnimTreeRuntime,
    pub sync: Arc<XAnimSyncTree>,
    notifies: Vec<Option<Arc<str>>>,
    notify_children: Vec<Option<XAnimNodeId>>,
    scripted: Option<scripted::ScriptedMotion>,
}
impl Animator {
    fn new(name: &str, data: ScriptAnimTree) -> Self {
        Self {
            name: name.into(),
            sync: data.sync.clone(),
            notifies: vec![None; data.names.len()],
            notify_children: vec![None; data.names.len()],
            scripted: None,
            runtime: XAnimTreeRuntime::new(data.definition.clone()),
            data,
        }
    }
    fn node(&self, value: &Value) -> Result<XAnimNodeId, String> {
        let Value::Animation { tree, name } = value else {
            return Err("argument is not an animation".into());
        };
        if **tree != self.name {
            return Err(format!(
                "animation tree '{tree}' does not match '{}'",
                self.name
            ));
        }
        self.data
            .names
            .iter()
            .position(|node| node == &**name)
            .map(|index| XAnimNodeId(index as u16))
            .ok_or_else(|| format!("animation '{name}' is not in tree '{tree}'"))
    }
    fn leaf(&self, node: XAnimNodeId) -> Result<(), String> {
        if matches!(
            self.data.definition.nodes()[node.0 as usize].kind,
            XAnimNodeKind::Leaf { .. }
        ) {
            Ok(())
        } else {
            Err("animation does not have a primitive clock".into())
        }
    }
}
pub(super) fn initialize_actor_tree(runtime: &mut RoundScript, id: u64) {
    if runtime
        .actors
        .get(&id)
        .is_none_or(|actor| !actor.species.humanoid())
        || runtime.animators.contains_key(&id)
    {
        return;
    }
    if let Some(data) = runtime.anim_trees.get("generic_human").cloned() {
        runtime
            .animators
            .insert(id, Animator::new("generic_human", data));
        runtime.actors.get_mut(&id).unwrap().tree_changed = true;
    }
}
fn receiver(world: &World, value: &Value) -> Result<u64, String> {
    entity_id(world, value)
}
fn animator(world: &World, id: u64) -> Result<&Animator, String> {
    world
        .resource::<RoundScript>()
        .animators
        .get(&id)
        .ok_or_else(|| "entity has no animation tree".into())
}
fn model(runtime: &mut RoundScript, id: u64) -> Result<(), String> {
    if matches!(runtime.object_field(id,"model"), Value::String(name) if !name.is_empty()) {
        Ok(())
    } else {
        Err("entity has no model".into())
    }
}
fn set_goal(
    anim: &mut Animator,
    node: XAnimNodeId,
    mut goal: f32,
    mut blend: f32,
    mut rate: f32,
) -> Result<(), String> {
    if let Some(notify) = anim.notifies.get_mut(node.0 as usize) {
        *notify = None;
    }
    if let Some(child) = anim.notify_children.get_mut(node.0 as usize) {
        *child = None;
    }
    let mut state = anim.runtime.states()[node.0 as usize];
    if node.0 == 0 {
        goal = 1.0;
        blend = 0.0;
        rate = 1.0;
    }
    if blend == 0.0 {
        state.weight = goal;
        state.goal_time = 0.0;
    } else {
        if state.weight == 0.0 {
            state.weight = goal * 0.001;
        }
        let difference = state.weight - goal;
        state.goal_time = if difference == 0.0 || (state.goal_weight - goal) * difference > 0.0 {
            blend
        } else {
            blend.min(state.goal_time)
        };
        if goal != 0.0 {
            let fraction = if state.weight > goal {
                difference / state.weight
            } else {
                (goal - state.weight) / goal
            };
            state.goal_time = state.goal_time.max(fraction * blend);
        }
    }
    state.goal_weight = goal;
    state.rate = rate;
    anim.runtime
        .set_state(node, state)
        .map_err(|e| e.to_string())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AnimCommand {
    Blend,
    Knob,
    KnobAll,
}

fn clear_goal(anim: &mut Animator, node: XAnimNodeId, blend: f32) -> Result<(), String> {
    if let Some(notify) = anim.notifies.get_mut(node.0 as usize) {
        *notify = None;
    }
    if let Some(child) = anim.notify_children.get_mut(node.0 as usize) {
        *child = None;
    }
    let mut state = anim.runtime.states()[node.0 as usize];
    state.goal_time = if state.goal_weight == 0.0 {
        state.goal_time.min(blend)
    } else {
        blend
    };
    state.goal_weight = 0.0;
    if blend == 0.0 {
        state.weight = 0.0;
    }
    anim.runtime
        .set_state(node, state)
        .map_err(|e| e.to_string())
}

fn clear_siblings(
    anim: &mut Animator,
    node: XAnimNodeId,
    goal: f32,
    blend: f32,
) -> Result<(), String> {
    let parent = anim.runtime.definition().nodes()[node.0 as usize].parent;
    let siblings: Vec<_> = anim
        .runtime
        .definition()
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.parent == parent)
        .map(|(index, _)| XAnimNodeId(index as u16))
        .collect();
    let difference = siblings.iter().fold(0.0_f32, |largest, sibling| {
        let weight = anim.runtime.states()[sibling.0 as usize].weight;
        largest.max(if *sibling == node {
            (goal - weight).abs()
        } else {
            weight
        })
    });
    for sibling in siblings {
        if sibling != node {
            clear_goal(anim, sibling, difference * blend)?;
        }
    }
    Ok(())
}

fn complete_ancestors(anim: &mut Animator, node: XAnimNodeId, blend: f32) -> Result<(), String> {
    let mut parent = anim.data.definition.nodes()[node.0 as usize].parent;
    while let Some(at) = parent {
        if anim.runtime.states()[at.0 as usize].goal_weight == 0.0 {
            set_goal(anim, at, 1.0, blend, 1.0)?;
        }
        parent = anim.data.definition.nodes()[at.0 as usize].parent;
    }
    Ok(())
}

fn restart_animator(anim: &mut Animator, node: XAnimNodeId, blend: f32) -> Result<(), String> {
    if let Some((sync, root)) = anim
        .runtime
        .restart_with_blend(node, blend, &anim.sync)
        .map_err(|e| e.to_string())?
    {
        anim.sync = Arc::new(sync);
        for index in 0..anim.notifies.len() {
            let mut parent = Some(XAnimNodeId(index as u16));
            while let Some(at) = parent {
                if at == root {
                    if let Some(name) = &mut anim.notifies[index] {
                        *name = Arc::from(&**name);
                    }
                    break;
                }
                parent = anim.runtime.definition().nodes()[at.0 as usize].parent;
            }
        }
    }
    Ok(())
}

fn set_anim(
    world: &mut World,
    receiver_value: &Value,
    args: &[Value],
    complete: bool,
    restart: bool,
    command: AnimCommand,
    flagged: bool,
) -> Result<Value, String> {
    let notify = if flagged {
        let Value::String(name) = arg(args, 0)? else {
            return Err("animation notification name must be a string".into());
        };
        Some(Arc::<str>::from(&**name))
    } else {
        None
    };
    let args = if flagged { &args[1..] } else { args };
    let offset = if command == AnimCommand::KnobAll {
        2
    } else {
        1
    };
    if !(offset..=offset + 4).contains(&args.len()) {
        return Err(format!(
            "animation command expects {offset} to {} arguments",
            offset + 4
        ));
    }
    let id = receiver(world, receiver_value)?;
    let weight = if args.len() > offset {
        float(args, offset)?
    } else {
        1.0
    };
    let blend = if args.len() > offset + 1 {
        float(args, offset + 1)?
    } else {
        0.2
    };
    let rate = if args.len() > offset + 2 {
        float(args, offset + 2)?
    } else {
        1.0
    };
    if [weight, blend, rate]
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err("animation weight, blend time and rate must be finite and nonnegative".into());
    }
    if flagged && weight == 0.0 {
        return Err("flagged animation weight must be positive".into());
    }
    let weight = anim_iw4::sanitize_goal_weight(weight, i32::from(flagged), 0);
    let mut runtime = world.resource_mut::<RoundScript>();
    model(&mut runtime, id)?;
    let anim = runtime
        .animators
        .get_mut(&id)
        .ok_or("entity has no animation tree")?;
    let node = anim.node(arg(args, 0)?)?;
    if flagged && anim.sync.group(node).is_none() {
        anim.leaf(node)?;
    }
    let root = if command == AnimCommand::KnobAll {
        let root = anim.node(arg(args, 1)?)?;
        let mut ancestor = anim.data.definition.nodes()[node.0 as usize].parent;
        while let Some(at) = ancestor {
            if at == root {
                break;
            }
            ancestor = anim.runtime.definition().nodes()[at.0 as usize].parent;
        }
        if ancestor.is_none() {
            return Err("knoball root must be a strict ancestor of the animation".into());
        }
        Some(root)
    } else {
        None
    };
    let old = anim.runtime.states()[node.0 as usize];
    if restart {
        restart_animator(anim, node, blend)?;
    }
    if command != AnimCommand::Blend {
        clear_siblings(anim, node, weight, blend)?;
    }
    set_goal(anim, node, weight, blend, rate)?;
    let primitive = matches!(
        anim.data.definition.nodes()[node.0 as usize].kind,
        XAnimNodeKind::Leaf { .. }
    );
    if !restart && primitive && old.weight == 0.0 && weight > 0.0 {
        let mut state = anim.runtime.states()[node.0 as usize];
        state.time = 0.0;
        state.old_time = 0.0;
        state.cycle_count = 0;
        state.old_cycle_count = 0;
        anim.runtime
            .set_state(node, state)
            .map_err(|e| e.to_string())?;
    }
    if let Some(root) = root {
        let mut parent = anim.data.definition.nodes()[node.0 as usize].parent;
        while let Some(at) = parent {
            if at == root {
                break;
            }
            if restart {
                restart_animator(anim, at, blend)?;
            }
            clear_siblings(anim, at, 1.0, blend)?;
            set_goal(anim, at, 1.0, blend, 1.0)?;
            if complete {
                complete_ancestors(anim, at, blend)?;
            }
            parent = anim.data.definition.nodes()[at.0 as usize].parent;
        }
    } else if complete {
        complete_ancestors(anim, node, blend)?;
    }
    if flagged {
        anim.notify_children[node.0 as usize] = Some(
            anim.sync
                .greatest_goal_descendant(&anim.runtime, node)
                .ok_or("flagged animation has no weighted primitive descendant")?,
        );
    }
    anim.notifies[node.0 as usize] = notify;
    Ok(Value::Undefined)
}

mod notifies;
pub(crate) use notifies::advance;

fn publish_trees(world: &mut World, changed_actors_only: bool) {
    let runtime = world.resource::<RoundScript>();
    if (changed_actors_only && runtime.actors.values().all(|actor| !actor.tree_changed))
        || (!changed_actors_only && runtime.actors.is_empty() && runtime.animators.is_empty())
    {
        return;
    }
    let trees: Vec<_> = runtime
        .entities
        .iter()
        .filter(|(id, _)| {
            if changed_actors_only {
                runtime
                    .actors
                    .get(id)
                    .is_some_and(|actor| actor.tree_changed)
            } else {
                runtime.actors.contains_key(id) || runtime.animators.contains_key(id)
            }
        })
        .filter_map(|(&id, entity)| {
            Some((
                id,
                entity.presence?,
                runtime.animators.get(&id).map(|anim| anim.runtime.clone()),
            ))
        })
        .collect();
    if trees.is_empty() {
        return;
    }
    let mut frame = crate::frame::FrameWorld::from_world(world);
    let mut applied = Vec::new();
    for (id, presence, tree) in trees {
        if let Some(dobj) = frame
            .collision_owner_mut(presence)
            .and_then(|row| row.dobj.as_mut())
        {
            if let Some(tree) = tree {
                dobj.set_script_animation_tree(&tree);
            } else {
                dobj.clear_script_animation_tree();
            }
            applied.push(id);
        }
    }
    let mut runtime = world.resource_mut::<RoundScript>();
    for id in applied {
        if let Some(actor) = runtime.actors.get_mut(&id) {
            actor.tree_changed = false;
        }
    }
}

pub(crate) fn publish(world: &mut World) {
    publish_trees(world, false);
}

pub(super) fn settle_actor_trees(world: &mut World) {
    publish_trees(world, true);
}

mod deltas;
mod scripted;
pub(crate) use scripted::{actor_motion_active, stop_actor_motion};
pub(crate) fn apply_scripted_motion(world: &mut World) -> Result<(), String> {
    scripted::apply(world)
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    deltas::register(registry);
    scripted::register(registry);
    use Namespace::Method;
    registry.register(Method, "useanimtree", |world, receiver_value, args| {
        let id = receiver(world, receiver_value)?;
        let Value::AnimationTree(name) = arg(args, 0)? else {
            return Err("argument is not an animation tree".into());
        };
        let mut runtime = world.resource_mut::<RoundScript>();
        if runtime.entities[&id].kind == EntityKind::Player
            || runtime
                .actors
                .get(&id)
                .is_some_and(|actor| actor.species.humanoid())
        {
            return Err("cannot use useanimtree on a player or human actor".into());
        }
        let data = runtime
            .anim_trees
            .get(&**name)
            .ok_or_else(|| format!("animation tree '{name}' is not loaded"))?
            .clone();
        if runtime
            .animators
            .get(&id)
            .is_some_and(|anim| anim.name == **name)
        {
            return Ok(Value::Undefined);
        }
        runtime.animators.insert(id, Animator::new(name, data));
        if let Some(actor) = runtime.actors.get_mut(&id) {
            actor.tree_changed = true;
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "stopuseanimtree", |world, receiver_value, _| {
        let id = receiver(world, receiver_value)?;
        let mut runtime = world.resource_mut::<RoundScript>();
        if runtime.entities[&id].kind == EntityKind::Player
            || runtime
                .actors
                .get(&id)
                .is_some_and(|actor| !actor.species.humanoid())
        {
            return Err("cannot use stopuseanimtree on a player or nonhuman actor".into());
        }
        if let Some(actor) = runtime.actors.get_mut(&id) {
            actor.inactive_script_tree = None;
            return Ok(Value::Undefined);
        }
        runtime.animators.remove(&id);
        let presence = runtime.entities[&id].presence;
        drop(runtime);
        if let Some(presence) = presence {
            if let Some(dobj) = crate::frame::FrameWorld::from_world(world)
                .collision_owner_mut(presence)
                .and_then(|row| row.dobj.as_mut())
            {
                dobj.clear_script_animation_tree();
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "getanimtime", |world, receiver_value, args| {
        let id = receiver(world, receiver_value)?;
        let anim = animator(world, id)?;
        let node = anim.node(arg(args, 0)?)?;
        if anim.sync.group(node).is_none() {
            anim.leaf(node)?;
        }
        Ok(Value::Float(anim.runtime.states()[node.0 as usize].time))
    });
    registry.register(Method, "setanimtime", |world, receiver_value, args| {
        if !(1..=3).contains(&args.len()) {
            return Err("setanimtime expects one to three arguments".into());
        }
        let id = receiver(world, receiver_value)?;
        let time = if args.len() > 1 { float(args, 1)? } else { 0.0 };
        if !(0.0..=1.0).contains(&time) {
            return Err("animation time must be between zero and one".into());
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let anim = runtime
            .animators
            .get_mut(&id)
            .ok_or("entity has no animation tree")?;
        let node = anim.node(arg(args, 0)?)?;
        anim.leaf(node)?;
        if time == 1.0 && anim.runtime.leaf_clip(node).unwrap().looping {
            return Err("cannot set a looping animation to time one".into());
        }
        let mut state = anim.runtime.states()[node.0 as usize];
        state.time = time;
        state.old_time = time;
        state.cycle_count = 0;
        state.old_cycle_count = 0;
        if let Some(name) = &mut anim.notifies[node.0 as usize] {
            *name = Arc::from(&**name);
        }
        anim.runtime
            .set_state(node, state)
            .map_err(|e| e.to_string())?;
        Ok(Value::Undefined)
    });
    registry.register(Method, "clearanim", |world, receiver_value, args| {
        let id = receiver(world, receiver_value)?;
        let blend = float(args, 1)?;
        if !blend.is_finite() || blend < 0.0 {
            return Err("animation blend time must be finite and nonnegative".into());
        }
        let mut runtime = world.resource_mut::<RoundScript>();
        let anim = runtime
            .animators
            .get_mut(&id)
            .ok_or("entity has no animation tree")?;
        let node = anim.node(arg(args, 0)?)?;
        for index in 0..anim.runtime.states().len() {
            let mut ancestor = Some(XAnimNodeId(index as u16));
            while let Some(at) = ancestor {
                if at == node {
                    clear_goal(anim, XAnimNodeId(index as u16), blend)?;
                    break;
                }
                ancestor = anim.runtime.definition().nodes()[at.0 as usize].parent;
            }
        }
        Ok(Value::Undefined)
    });
    registry.register(Method, "setanim", |w, r, a| {
        set_anim(w, r, a, true, false, AnimCommand::Blend, false)
    });
    registry.register(Method, "setanimlimited", |w, r, a| {
        set_anim(w, r, a, false, false, AnimCommand::Blend, false)
    });
    registry.register(Method, "setanimrestart", |w, r, a| {
        set_anim(w, r, a, true, true, AnimCommand::Blend, false)
    });
    registry.register(Method, "setanimlimitedrestart", |w, r, a| {
        set_anim(w, r, a, false, true, AnimCommand::Blend, false)
    });
    macro_rules! knob {
        ($name:literal, $complete:literal, $restart:literal, $command:ident) => {
            registry.register(Method, $name, |w, r, a| {
                set_anim(w, r, a, $complete, $restart, AnimCommand::$command, false)
            });
        };
    }
    knob!("setanimknob", true, false, Knob);
    knob!("setanimknoblimited", false, false, Knob);
    knob!("setanimknobrestart", true, true, Knob);
    knob!("setanimknoblimitedrestart", false, true, Knob);
    knob!("setanimknoball", true, false, KnobAll);
    knob!("setanimknoballlimited", false, false, KnobAll);
    knob!("setanimknoballrestart", true, true, KnobAll);
    knob!("setanimknoballlimitedrestart", false, true, KnobAll);
    macro_rules! flagged {
        ($name:literal, $complete:literal, $restart:literal, $command:ident) => {
            registry.register(Method, $name, |w, r, a| {
                set_anim(w, r, a, $complete, $restart, AnimCommand::$command, true)
            });
        };
    }
    flagged!("setflaggedanim", true, false, Blend);
    flagged!("setflaggedanimlimited", false, false, Blend);
    flagged!("setflaggedanimrestart", true, true, Blend);
    flagged!("setflaggedanimlimitedrestart", false, true, Blend);
    flagged!("setflaggedanimknob", true, false, Knob);
    flagged!("setflaggedanimknoblimited", false, false, Knob);
    flagged!("setflaggedanimknobrestart", true, true, Knob);
    flagged!("setflaggedanimknoblimitedrestart", false, true, Knob);
    flagged!("setflaggedanimknoball", true, false, KnobAll);
    flagged!("setflaggedanimknoballrestart", true, true, KnobAll);
}
