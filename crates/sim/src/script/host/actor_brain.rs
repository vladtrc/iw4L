use bevy_ecs::prelude::World;

use crate::script::runtime::{raise, run_now};
use crate::script::{Runtime, Value};

/// The animscript an actor runs; the engine picks it, the script plays it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnimScript {
    Stop,
    Move,
    Combat,
    Scripted,
    Traverse,
    Death,
}

impl AnimScript {
    fn module(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Move => "move",
            Self::Combat => "combat",
            Self::Scripted => "scripted",
            Self::Traverse => "traverse",
            Self::Death => "death",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ActorBrain {
    /// Animscript family, `zombie_` or `zombie_dog_`.
    prefix: &'static str,
    script: Option<AnimScript>,
    /// Killed; its death animscript plays out, then the actor is removed.
    dead: bool,
    death_thread: Option<u64>,
}

/// An enemy this close is fought rather than approached.
const MELEE_RANGE: f32 = 64.0;

/// Marks an actor killed: its death animscript replaces whatever it ran.
pub(crate) fn kill(world: &mut World, actor: u64) {
    let mut runtime = world.resource_mut::<Runtime>();
    if let Some(brain) = runtime.actor_brains.get_mut(&actor) {
        brain.dead = true;
    }
    if let Some(movement) = runtime.actor_moves.get_mut(&actor) {
        *movement = super::actor_nav::ActorMove::default();
    }
}

/// The actor's animscript family prefix.
pub(crate) fn prefix(world: &World, actor: u64) -> &'static str {
    world
        .resource::<Runtime>()
        .actor_brains
        .get(&actor)
        .map_or("zombie_", |brain| brain.prefix)
}

/// Runs the actor's animscript init and takes charge of its animscripts.
pub(crate) fn begin(world: &mut World, actor: u64) -> Result<(), String> {
    let prefix = match world.resource_mut::<Runtime>().object_field(actor, "type") {
        Value::String(kind) if kind.as_bytes().starts_with(b"zombie_dog") => "zombie_dog_",
        _ => "zombie_",
    };
    let now = super::players::now_ms(world);
    let init = format!("animscripts/{prefix}init::main");
    run_now(world, &init, Value::Object(actor), Vec::new(), now)
        .map_err(|fault| format!("{init}: {fault:?}"))?;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.actor_brains.insert(
        actor,
        ActorBrain {
            prefix,
            script: None,
            dead: false,
            death_thread: None,
        },
    );
    runtime.actor_moves.entry(actor).or_default();
    Ok(())
}

/// Switches each actor to the animscript its situation calls for, ending
/// the previous one with `killanimscript`.
pub(crate) fn think(world: &mut World) {
    let actors: Vec<(u64, ActorBrain)> = world
        .resource::<Runtime>()
        .actor_brains
        .iter()
        .map(|(actor, brain)| (*actor, brain.clone()))
        .collect();
    for (actor, brain) in actors {
        if brain.dead {
            match brain.death_thread {
                Some(thread) if !crate::script::runtime::thread_alive(world, thread) => {
                    let mut runtime = world.resource_mut::<Runtime>();
                    runtime.actor_brains.remove(&actor);
                    if !runtime.pending_deletes.contains(&actor) {
                        runtime.pending_deletes.push(actor);
                    }
                }
                Some(_) => {}
                None => {
                    if brain.script.is_some() {
                        raise(world, Value::Object(actor), "killanimscript", Vec::new());
                    }
                    let main = format!("animscripts/{}death::main", brain.prefix);
                    let thread =
                        crate::script::start(world, &main, Value::Object(actor), Vec::new()).ok();
                    if let Some(brain) =
                        world.resource_mut::<Runtime>().actor_brains.get_mut(&actor)
                    {
                        brain.script = Some(AnimScript::Death);
                        brain.death_thread = Some(thread.unwrap_or(0));
                    }
                }
            }
            continue;
        }
        super::actor_nav::choose_enemy(world, actor);
        let scripted = world
            .resource::<Runtime>()
            .actor_moves
            .get(&actor)
            .is_some_and(super::actor_nav::ActorMove::is_scripted);
        let traversal = world
            .resource::<Runtime>()
            .actor_moves
            .get(&actor)
            .and_then(|movement| movement.traversal.clone());
        if let Some(traversal) = &traversal
            && brain.script == Some(AnimScript::Traverse)
            && traversal
                .thread
                .is_some_and(|thread| !crate::script::runtime::thread_alive(world, thread))
        {
            if let Some(movement) = world.resource_mut::<Runtime>().actor_moves.get_mut(&actor) {
                movement.finish_traversal();
            }
            continue;
        }
        let wanted = if traversal.is_some() {
            AnimScript::Traverse
        } else if scripted {
            AnimScript::Scripted
        } else if super::actor_nav::melee_range_enemy(world, actor, MELEE_RANGE) {
            AnimScript::Combat
        } else if world
            .resource::<Runtime>()
            .actor_moves
            .get(&actor)
            .is_some_and(super::actor_nav::ActorMove::has_path)
        {
            AnimScript::Move
        } else {
            AnimScript::Stop
        };
        let restart = wanted == AnimScript::Scripted
            && world
                .resource::<Runtime>()
                .actor_moves
                .get(&actor)
                .is_some_and(super::actor_nav::ActorMove::scripted_waiting);
        if brain.script == Some(wanted) && !restart {
            continue;
        }
        if brain.script.is_some() {
            raise(world, Value::Object(actor), "killanimscript", Vec::new());
        }
        // A new animscript starts from the engine's default movement modes.
        if let Some(movement) = world.resource_mut::<Runtime>().actor_moves.get_mut(&actor) {
            movement.anim_mode = super::actor_nav::AnimMode::Normal;
            movement.orient = super::actor_nav::Orient::Motion;
        }
        if let Value::Vector(at) = world
            .resource_mut::<Runtime>()
            .object_field(actor, "origin")
        {
            diag::debug!(
                Sim,
                "actor {actor}: {:?} -> {wanted:?} at ({:.0} {:.0} {:.0})",
                brain.script,
                at[0],
                at[1],
                at[2]
            );
        }
        let main = match (&traversal, wanted) {
            (Some(traversal), AnimScript::Traverse) => {
                format!("animscripts/traverse/{}::main", traversal.script)
            }
            _ => format!("animscripts/{}{}::main", brain.prefix, wanted.module()),
        };
        match crate::script::start(world, &main, Value::Object(actor), Vec::new()) {
            Ok(thread) if wanted == AnimScript::Traverse => {
                if let Some(traversal) = world
                    .resource_mut::<Runtime>()
                    .actor_moves
                    .get_mut(&actor)
                    .and_then(|movement| movement.traversal.as_mut())
                {
                    traversal.thread = Some(thread);
                }
            }
            Ok(_) => {}
            Err(fault) => {
                diag::warn!(Sim, "actor animscript {main}: {fault:?}");
                if wanted == AnimScript::Traverse
                    && let Some(movement) =
                        world.resource_mut::<Runtime>().actor_moves.get_mut(&actor)
                {
                    movement.finish_traversal();
                }
            }
        }
        if let Some(brain) = world.resource_mut::<Runtime>().actor_brains.get_mut(&actor) {
            brain.script = Some(wanted);
        }
    }
}
