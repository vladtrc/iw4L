use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use bevy_ecs::prelude::World;
use xmodel_runtime::{XAnimNodeId, XAnimNodeKind, XAnimTreeDefinition, XAnimTreeRuntime};

use super::args::{arg, float, optional, string};
use super::entities::EntityKind;
use crate::frame::FrameWorld;
use crate::script::runtime::raise;
use crate::script::{Namespace, NativeRegistry, Runtime, Value};

/// An animation tree actors play: the runtime definition and its node names.
#[derive(Debug)]
pub struct ActorAnimTree {
    name: String,
    definition: Arc<XAnimTreeDefinition>,
    index: HashMap<String, u16>,
    children: Vec<Vec<u16>>,
}

impl ActorAnimTree {
    /// `names[i]` names node `i` of `definition`.
    pub fn new(
        name: impl Into<String>,
        names: Vec<String>,
        definition: Arc<XAnimTreeDefinition>,
    ) -> Self {
        let mut children = vec![Vec::new(); definition.nodes().len()];
        for (node, definition) in definition.nodes().iter().enumerate() {
            if let Some(parent) = definition.parent {
                children[parent.0 as usize].push(node as u16);
            }
        }
        let index = names
            .into_iter()
            .enumerate()
            .map(|(node, name)| (name.to_ascii_lowercase(), node as u16))
            .collect();
        Self {
            name: name.into().to_ascii_lowercase(),
            definition,
            index,
            children,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn node(&self, name: &str) -> Option<u16> {
        self.index.get(&name.to_ascii_lowercase()).copied()
    }

    fn parent(&self, node: u16) -> Option<u16> {
        self.definition.nodes()[node as usize].parent.map(|p| p.0)
    }

    fn siblings(&self, node: u16) -> impl Iterator<Item = u16> + '_ {
        self.parent(node)
            .map(|parent| self.children[parent as usize].as_slice())
            .unwrap_or_default()
            .iter()
            .copied()
            .filter(move |&sibling| sibling != node)
    }

    fn descendants(&self, node: u16) -> Vec<u16> {
        let mut out = Vec::new();
        let mut stack = vec![node];
        while let Some(next) = stack.pop() {
            out.push(next);
            stack.extend(self.children[next as usize].iter().copied());
        }
        out
    }

    fn is_leaf(&self, node: u16) -> bool {
        matches!(
            self.definition.nodes()[node as usize].kind,
            XAnimNodeKind::Leaf { .. }
        )
    }
}

/// One actor's animation state: its tree and which nodes report notetracks
/// under which notify name.
#[derive(Clone, Debug)]
pub(crate) struct ActorAnim {
    tree: Arc<ActorAnimTree>,
    runtime: XAnimTreeRuntime,
    flags: BTreeMap<u16, Arc<str>>,
}

impl ActorAnim {
    fn new(tree: Arc<ActorAnimTree>) -> Self {
        Self {
            runtime: XAnimTreeRuntime::new(Arc::clone(&tree.definition)),
            tree,
            flags: BTreeMap::new(),
        }
    }

    fn set_goal(&mut self, node: u16, weight: f32, time: f32) {
        let _ = self
            .runtime
            .set_goal_weight(XAnimNodeId(node), weight, time);
    }

    fn state(&self, node: u16) -> xmodel_runtime::XAnimNodeState {
        self.runtime.states()[node as usize]
    }

    fn put(&mut self, node: u16, state: xmodel_runtime::XAnimNodeState) {
        let _ = self.runtime.set_state(XAnimNodeId(node), state);
    }
}

/// The variants of the `SetAnim` family.
#[derive(Clone, Copy)]
struct SetAnim {
    flagged: bool,
    knob: bool,
    all: bool,
    limited: bool,
    restart: bool,
}

const SET_ANIMS: &[(&str, SetAnim)] = &[
    ("setanim", SetAnim::plain()),
    ("setanimlimited", SetAnim::plain().limited()),
    ("setanimrestart", SetAnim::plain().restart()),
    ("setanimknob", SetAnim::plain().knob()),
    ("setanimknoblimited", SetAnim::plain().knob().limited()),
    ("setanimknobrestart", SetAnim::plain().knob().restart()),
    (
        "setanimknoblimitedrestart",
        SetAnim::plain().knob().limited().restart(),
    ),
    ("setanimknoball", SetAnim::plain().knob().all()),
    (
        "setanimknoballrestart",
        SetAnim::plain().knob().all().restart(),
    ),
    ("setflaggedanim", SetAnim::plain().flagged()),
    (
        "setflaggedanimlimited",
        SetAnim::plain().flagged().limited(),
    ),
    (
        "setflaggedanimrestart",
        SetAnim::plain().flagged().restart(),
    ),
    ("setflaggedanimknob", SetAnim::plain().flagged().knob()),
    (
        "setflaggedanimknobrestart",
        SetAnim::plain().flagged().knob().restart(),
    ),
    (
        "setflaggedanimknoblimitedrestart",
        SetAnim::plain().flagged().knob().limited().restart(),
    ),
    (
        "setflaggedanimknoball",
        SetAnim::plain().flagged().knob().all(),
    ),
    (
        "setflaggedanimknoballrestart",
        SetAnim::plain().flagged().knob().all().restart(),
    ),
];

impl SetAnim {
    const fn plain() -> Self {
        Self {
            flagged: false,
            knob: false,
            all: false,
            limited: false,
            restart: false,
        }
    }
    const fn flagged(self) -> Self {
        Self {
            flagged: true,
            ..self
        }
    }
    const fn knob(self) -> Self {
        Self { knob: true, ..self }
    }
    const fn all(self) -> Self {
        Self { all: true, ..self }
    }
    const fn limited(self) -> Self {
        Self {
            limited: true,
            ..self
        }
    }
    const fn restart(self) -> Self {
        Self {
            restart: true,
            ..self
        }
    }
}

/// Blend time when a script names none.
const DEFAULT_GOAL_TIME: f32 = 0.2;

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Method;
    macro_rules! set_anims {
        ($($index:literal),*) => {$(
            registry.register(Method, SET_ANIMS[$index].0, |world, receiver, args| {
                set_anim(world, receiver, args, SET_ANIMS[$index].1)
            });
        )*};
    }
    set_anims!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);
    registry.register(Method, "clearanim", |world, receiver, args| {
        let time = optional(args, 1, float)?.unwrap_or(DEFAULT_GOAL_TIME);
        with_anim(world, receiver, args, 0, |anim, node| {
            for node in anim.tree.descendants(node) {
                anim.set_goal(node, 0.0, time);
            }
            Ok(Value::Undefined)
        })
    });
    registry.register(Method, "getanimtime", |world, receiver, args| {
        with_anim(world, receiver, args, 0, |anim, node| {
            Ok(Value::Float(anim.state(node).time))
        })
    });
    registry.register(Method, "setanimtime", |world, receiver, args| {
        let time = float(args, 1)?.clamp(0.0, 1.0);
        with_anim(world, receiver, args, 0, |anim, node| {
            let mut state = anim.state(node);
            state.time = time;
            state.old_time = time;
            anim.put(node, state);
            Ok(Value::Undefined)
        })
    });
    registry.register(Method, "useanimtree", |world, receiver, args| {
        let Value::AnimationTree(name) = arg(args, 0)? else {
            return Err("expects an animation tree".into());
        };
        let name = name.clone();
        match actor_id(world, receiver) {
            Ok(id) => attach(world, id, &name),
            // A script model plays single clips (`AnimScripted`); it needs
            // no tree of its own.
            Err(_) => {
                super::natives::engine::entity_id(world, receiver)?;
                Ok(Value::Undefined)
            }
        }
    });
}

/// Gives an actor the named tree unless it already plays it.
pub(crate) fn attach(world: &mut World, actor: u64, tree: &str) -> Result<Value, String> {
    let tree = FrameWorld::from_world(world)
        .actor_anim_tree(tree)
        .ok_or_else(|| format!("animation tree '{tree}' is not loaded"))?;
    let mut runtime = world.resource_mut::<Runtime>();
    if runtime
        .actor_anims
        .get(&actor)
        .is_none_or(|anim| anim.tree.name != tree.name)
    {
        runtime.actor_anims.insert(actor, ActorAnim::new(tree));
    }
    Ok(Value::Undefined)
}

fn actor_id(world: &World, receiver: &Value) -> Result<u64, String> {
    match world.resource::<Runtime>().entity(receiver) {
        Some((id, entity)) if entity.kind == EntityKind::Actor => Ok(id),
        _ => Err("receiver is not an actor".into()),
    }
}

fn anim_name(args: &[Value], index: usize) -> Result<Arc<str>, String> {
    match arg(args, index)? {
        Value::Animation { name, .. } => Ok(name.clone()),
        _ => Err(format!("parameter {} is not an animation", index + 1)),
    }
}

fn with_anim(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    index: usize,
    apply: impl FnOnce(&mut ActorAnim, u16) -> Result<Value, String>,
) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    let name = anim_name(args, index)?;
    let mut runtime = world.resource_mut::<Runtime>();
    let anim = runtime
        .actor_anims
        .get_mut(&id)
        .ok_or("actor has no animation tree")?;
    let node = anim
        .tree
        .node(&name)
        .ok_or_else(|| format!("animation '{name}' is not in tree '{}'", anim.tree.name))?;
    apply(anim, node)
}

fn set_anim(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    mode: SetAnim,
) -> Result<Value, String> {
    let mut at = 0;
    let flag: Option<Arc<str>> = if mode.flagged {
        at += 1;
        Some(string(args, 0)?.into())
    } else {
        None
    };
    let anim_at = at;
    at += 1;
    let root = if mode.all {
        at += 1;
        Some(anim_name(args, at - 1)?)
    } else {
        None
    };
    let weight = optional(args, at, float)?.unwrap_or(1.0);
    let time = optional(args, at + 1, float)?
        .unwrap_or(DEFAULT_GOAL_TIME)
        .max(0.0);
    let rate = optional(args, at + 2, float)?.unwrap_or(1.0);
    with_anim(world, receiver, args, anim_at, |anim, node| {
        let tree = Arc::clone(&anim.tree);
        let root = match &root {
            Some(name) => Some(
                tree.node(name)
                    .ok_or_else(|| format!("animation '{name}' is not in tree '{}'", tree.name))?,
            ),
            None => None,
        };
        if mode.restart {
            for leaf in tree.descendants(node) {
                let mut state = anim.state(leaf);
                state.time = 0.0;
                state.old_time = 0.0;
                state.cycle_count = 0;
                state.old_cycle_count = 0;
                anim.put(leaf, state);
            }
        }
        anim.set_goal(node, weight, time);
        for leaf in tree.descendants(node) {
            if tree.is_leaf(leaf) {
                let _ = anim.runtime.set_rate(XAnimNodeId(leaf), rate);
            }
        }
        if mode.knob {
            for sibling in tree.siblings(node).collect::<Vec<_>>() {
                anim.set_goal(sibling, 0.0, time);
            }
        }
        let mut parent = tree.parent(node);
        while let Some(ancestor) = parent {
            if mode.all {
                anim.set_goal(ancestor, 1.0, time);
                if Some(ancestor) == root {
                    break;
                }
                for sibling in tree.siblings(ancestor).collect::<Vec<_>>() {
                    anim.set_goal(sibling, 0.0, time);
                }
            } else if !mode.limited && anim.state(ancestor).goal_weight == 0.0 {
                anim.set_goal(ancestor, 1.0, time);
            }
            parent = tree.parent(ancestor);
        }
        match flag {
            Some(flag) => {
                anim.flags.insert(node, flag);
            }
            None => {
                anim.flags.remove(&node);
            }
        }
        Ok(Value::Undefined)
    })
}

/// Advances every actor's tree one authority tick and reports the notetracks
/// that flagged animations crossed, then `end` for ones that finished.
pub(crate) fn advance(world: &mut World, seconds: f32) {
    let mut notifies = Vec::new();
    {
        let mut runtime = world.resource_mut::<Runtime>();
        for (actor, anim) in &mut runtime.actor_anims {
            if anim.runtime.update(seconds).is_err() {
                continue;
            }
            let tree = Arc::clone(&anim.tree);
            for (leaf, state) in anim.runtime.states().iter().enumerate() {
                if state.weight == 0.0 {
                    continue;
                }
                let XAnimNodeKind::Leaf { clip, .. } = &tree.definition.nodes()[leaf].kind else {
                    continue;
                };
                let Some(flag) = flag_for(&tree, &anim.flags, leaf as u16) else {
                    continue;
                };
                let wrapped = state.cycle_count != state.old_cycle_count;
                for note in &clip.notifies {
                    let crossed = if wrapped {
                        note.time > state.old_time || note.time <= state.time
                    } else {
                        note.time > state.old_time && note.time <= state.time
                    };
                    if crossed {
                        notifies.push((*actor, flag.clone(), note.name.to_ascii_lowercase()));
                    }
                }
                if !clip.looping && state.old_time < 1.0 && state.time >= 1.0 {
                    notifies.push((*actor, flag, "end".to_owned()));
                }
            }
        }
    }
    for (actor, flag, note) in notifies {
        raise(
            world,
            Value::Object(actor),
            &flag,
            vec![Value::string(&note)],
        );
    }
}

fn flag_for(tree: &ActorAnimTree, flags: &BTreeMap<u16, Arc<str>>, leaf: u16) -> Option<Arc<str>> {
    let mut node = Some(leaf);
    while let Some(current) = node {
        if let Some(flag) = flags.get(&current) {
            return Some(flag.clone());
        }
        node = tree.parent(current);
    }
    None
}

/// Plays `anim` as a scripted animation: knobbed in from `root` (the
/// actor's `body` branch by default), restarted, notetracks reported under
/// `notify`. Returns the animation's node.
pub(crate) fn play_scripted(
    world: &mut World,
    actor: u64,
    notify: &str,
    anim: &Value,
    root: Option<&Value>,
    rate: f32,
    goal_time: f32,
) -> Result<u16, String> {
    let tree = world
        .resource::<Runtime>()
        .actor_anims
        .get(&actor)
        .map(|anim| anim.tree.name.clone())
        .ok_or("actor has no animation tree")?;
    let root = root.cloned().unwrap_or_else(|| Value::Animation {
        tree: tree.as_str().into(),
        name: "body".into(),
    });
    let args = [
        Value::string(notify),
        anim.clone(),
        root,
        Value::Float(1.0),
        Value::Float(goal_time),
        Value::Float(rate),
    ];
    set_anim(
        world,
        &Value::Object(actor),
        &args,
        SetAnim::plain().flagged().knob().all().restart(),
    )?;
    let name = anim_name(&args, 1)?;
    let runtime = world.resource::<Runtime>();
    let state = runtime
        .actor_anims
        .get(&actor)
        .ok_or("actor has no animation tree")?;
    state
        .tree
        .node(&name)
        .ok_or_else(|| format!("animation '{name}' is not in tree '{tree}'"))
}

/// The clip a node plays and its normalized time.
pub(crate) fn node_clip(
    world: &World,
    actor: u64,
    node: u16,
) -> Option<(Arc<xmodel_runtime::AnimClip>, f32)> {
    let anim = world.resource::<Runtime>().actor_anims.get(&actor)?;
    let clip = anim.runtime.leaf_clip(XAnimNodeId(node))?;
    Some((clip, anim.state(node).time))
}

/// Root motion and yaw of an actor over the last tick, blended by the
/// effective weight of every playing leaf.
pub(crate) fn root_delta(world: &World, actor: u64) -> Option<([f32; 3], f32)> {
    let anim = world.resource::<Runtime>().actor_anims.get(&actor)?;
    let tree = &anim.tree;
    let states = anim.runtime.states();
    let mut translation = [0.0f32; 3];
    let mut yaw = 0.0f32;
    let mut total = 0.0f32;
    for (leaf, state) in states.iter().enumerate() {
        let XAnimNodeKind::Leaf { clip, .. } = &tree.definition.nodes()[leaf].kind else {
            continue;
        };
        let mut weight = state.weight;
        let mut parent = tree.parent(leaf as u16);
        while let Some(node) = parent {
            weight *= states[node as usize].weight;
            parent = tree.parent(node);
        }
        if weight <= 0.0 {
            continue;
        }
        let cycles = f32::from(state.cycle_count - state.old_cycle_count);
        let now = clip.abs_delta_trans(state.time);
        let then = clip.abs_delta_trans(state.old_time);
        let full = clip.abs_delta_trans(1.0);
        let start_yaw = clip.abs_delta_yaw(state.old_time).to_radians();
        let local = [
            now[0] - then[0] + cycles * full[0],
            now[1] - then[1] + cycles * full[1],
            now[2] - then[2] + cycles * full[2],
        ];
        let (sin, cos) = (-start_yaw).sin_cos();
        translation[0] += weight * (local[0] * cos - local[1] * sin);
        translation[1] += weight * (local[0] * sin + local[1] * cos);
        translation[2] += weight * local[2];
        yaw += weight
            * (math_iw4::angle_subtract(
                clip.abs_delta_yaw(state.time),
                clip.abs_delta_yaw(state.old_time),
            ) + cycles * clip.abs_delta_yaw(1.0));
        total += weight;
    }
    (total > 0.0).then_some((translation, yaw))
}

/// The part of an actor's tree that shapes its pose: every node carrying
/// weight under a weighted parent, renumbered in tree order, with the clips
/// its leaves play.
fn weighted_tree(
    anim: &ActorAnim,
) -> (
    xmodel_runtime::XAnimTreeSnapshot,
    HashMap<String, Arc<xmodel_runtime::AnimClip>>,
) {
    use xmodel_runtime::{XAnimSemanticNode, XAnimSemanticNodeKind};

    let tree = &anim.tree;
    let states = anim.runtime.states();
    let mut nodes = Vec::new();
    let mut clips = HashMap::new();
    const FNV_PRIME: u32 = 0x0100_0193;
    let mut shape = 0x811c_9dc5u32;
    let mut stack: Vec<(u16, Option<u16>)> = (0..tree.children.len() as u16)
        .rev()
        .filter(|&node| tree.parent(node).is_none())
        .map(|node| (node, None))
        .collect();
    while let Some((node, parent)) = stack.pop() {
        let state = states[node as usize];
        if parent.is_some() && state.weight <= 0.0 {
            continue;
        }
        let (kind, clip, parts) = match &tree.definition.nodes()[node as usize].kind {
            XAnimNodeKind::Blend => (XAnimSemanticNodeKind::Blend, None, None),
            XAnimNodeKind::Additive => (XAnimSemanticNodeKind::Additive, None, None),
            XAnimNodeKind::Leaf { clip, parts } => {
                clips.insert(clip.name.clone(), Arc::clone(clip));
                (XAnimSemanticNodeKind::Leaf, Some(clip.name.clone()), *parts)
            }
        };
        let index = nodes.len() as u16;
        for byte in node.to_le_bytes() {
            shape = (shape ^ u32::from(byte)).wrapping_mul(FNV_PRIME);
        }
        nodes.push(XAnimSemanticNode {
            parent: parent.map(XAnimNodeId),
            kind,
            clip,
            parts,
            state,
        });
        stack.extend(
            tree.children[node as usize]
                .iter()
                .rev()
                .map(|&child| (child, Some(index))),
        );
    }
    (
        xmodel_runtime::XAnimTreeSnapshot {
            definition_revision: shape,
            state_revision: 0,
            nodes,
        },
        clips,
    )
}

/// Puts each actor's current animation on its presented model, so clients
/// draw the pose and traces hit the actor where its body is.
pub(crate) fn present(world: &mut World) {
    let posed: Vec<(
        crate::ScriptModelId,
        xmodel_runtime::XAnimTreeSnapshot,
        Option<XAnimTreeRuntime>,
    )> = {
        let runtime = world.resource::<Runtime>();
        runtime
            .actor_anims
            .iter()
            .filter_map(|(actor, anim)| {
                let presence = runtime.entities.get(actor)?.presence?;
                let (tree, clips) = weighted_tree(anim);
                let resolved = tree.resolve(|name| clips.get(name).cloned()).ok();
                Some((presence, tree, resolved))
            })
            .collect()
    };
    let mut frame = FrameWorld::from_world(world);
    for (presence, tree, resolved) in posed {
        if let Some(dobj) = frame
            .collision_owner_mut(presence)
            .and_then(|row| row.dobj.as_mut())
        {
            dobj.set_tree_pose(tree, resolved);
            dobj.materialize();
        }
    }
}
