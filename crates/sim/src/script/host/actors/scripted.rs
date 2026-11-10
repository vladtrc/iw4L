use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlKind {
    Scripted,
    Custom,
}

#[derive(Clone, Debug)]
pub(crate) struct ScriptedControl {
    started: bool,
    kind: ControlKind,
}

pub(super) fn owns_placement(actor: &Actor) -> bool {
    actor
        .scripted_control
        .as_ref()
        .is_some_and(|c| c.kind == ControlKind::Scripted)
}

fn now(world: &World) -> i64 {
    world
        .get_resource::<crate::step::StepRequest>()
        .map_or(0, |r| i64::from(crate::level_time_ms(r.tick)))
}

pub(crate) fn kill_anim_script(world: &mut World, id: u64) {
    let thread = world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .and_then(|actor| {
            if let Some(kind) = actor.anim_script_kind.take() {
                actor.previous_anim_script = Some(kind);
            }
            actor.anim_script_thread.take()
        });
    if thread.is_some() {
        crate::script::runtime::raise(world, Value::Object(id), "killanimscript", vec![]);
    }
}

pub(crate) fn enter(world: &mut World, id: u64, args: &[Value]) -> Result<Value, String> {
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime
            .actors
            .get_mut(&id)
            .ok_or("actor state requires an allocated actor")?;
        if !actor.alive {
            return Err("cannot animscripted a nonliving actor".into());
        }
        actor.goals.arrival_time = 0;
        actor.goals.arrival_duration = 0;
        actor.scripted_control = Some(ScriptedControl {
            started: false,
            kind: ControlKind::Scripted,
        });
    }
    kill_anim_script(world, id);
    let mut parameters = args.to_vec();
    parameters.resize(6, Value::Undefined);
    crate::script::runtime::run_now(
        world,
        "animscripts/scripted::init",
        Value::Object(id),
        parameters,
        now(world),
    )
    .map_err(|e| e.to_string())?;
    Ok(Value::Undefined)
}

fn function(world: &World, args: &[Value], index: usize) -> Result<u32, String> {
    let Value::Function(function) = arg(args, index)? else {
        return Err(format!("parameter {index} must be a script function"));
    };
    if world
        .resource::<RoundScript>()
        .program
        .as_ref()
        .is_none_or(|program| *function as usize >= program.functions.len())
    {
        return Err("invalid script function".into());
    }
    Ok(*function)
}

fn run_function(world: &mut World, id: u64, function: u32, now: i64) -> Result<u64, String> {
    let name = {
        let runtime = world.resource::<RoundScript>();
        let program = runtime
            .program
            .as_ref()
            .ok_or("script program is missing")?;
        let entry = program
            .functions
            .get(function as usize)
            .ok_or("invalid script function")?;
        format!("{}::{}", entry.location.module, entry.location.function)
    };
    crate::script::runtime::run_now_owned(world, &name, Value::Object(id), vec![], now)
        .map_err(|e| e.to_string())
}

fn custom(world: &mut World, receiver: &Value, args: &[Value]) -> Result<Value, String> {
    let id = actor_id(world, receiver)?;
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        let actor = runtime.actors.get_mut(&id).unwrap();
        actor.goals.arrival_time = 0;
        actor.goals.arrival_duration = 0;
        actor.scripted_control = Some(ScriptedControl {
            started: false,
            kind: ControlKind::Custom,
        });
    }
    let main = function(world, args, 0)?;
    let end = if args.len() > 1 {
        Some(function(world, args, 1)?)
    } else {
        None
    };
    kill_anim_script(world, id);
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .custom_anim = Some((main, end));
    Ok(Value::Undefined)
}

fn current(world: &World, id: u64, kind: ControlKind) -> bool {
    world
        .resource::<RoundScript>()
        .actors
        .get(&id)
        .is_some_and(|actor| {
            actor
                .scripted_control
                .as_ref()
                .is_some_and(|control| control.started && control.kind == kind)
        })
}

pub(crate) fn pop(world: &mut World, id: u64) {
    if let Some(actor) = world.resource_mut::<RoundScript>().actors.get_mut(&id) {
        actor.scripted_control = None;
    }
    kill_anim_script(world, id);
}

pub(crate) fn think(world: &mut World, now: i32) -> Result<(), String> {
    let actors: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| actor.scripted_control.as_ref().map(|_| id))
        .collect();
    if actors.is_empty() {
        return Ok(());
    }
    for id in actors {
        let (start, kind, species, alive) = {
            let mut runtime = world.resource_mut::<RoundScript>();
            let Some(actor) = runtime.actors.get_mut(&id) else {
                continue;
            };
            if actor.scripted_control.is_none() {
                continue;
            }
            actor.path_stances = None;
            if actor.scripted_control.as_ref().unwrap().kind == ControlKind::Scripted {
                actor.motion.stop();
            }
            if !actor.goals.keep_claimed {
                actor.goals.keep_claimed_if_valid = false;
                actor.goals.arrival_time = 0;
                actor.goals.arrival_duration = 0;
                actor.claimed_node = None;
                actor.at_claim_node = false;
            }
            let control = actor.scripted_control.as_mut().unwrap();
            let start = !control.started;
            control.started = true;
            (start, control.kind, actor.species, actor.alive)
        };
        if !alive {
            super::super::animations::stop_actor_motion(world, id);
            pop(world, id);
            continue;
        }
        if start {
            if kind == ControlKind::Custom {
                crate::script::runtime::raise(
                    world,
                    Value::Object(id),
                    "begin_custom_anim",
                    vec![],
                );
            }
            kill_anim_script(world, id);
            crate::script::runtime::deliver_external(world, i64::from(now));
            crate::script::runtime::run_current_threads(world, i64::from(now));
            if !current(world, id, kind) {
                continue;
            }
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .orientation
                .clear_script();
            let previous = world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .previous_anim_script
                .take();
            if previous == Some(ControlKind::Custom) {
                let end = world.resource::<RoundScript>().actors[&id]
                    .custom_anim
                    .and_then(|(_, end)| end);
                if let Some(end) = end {
                    run_function(world, id, end, i64::from(now))?;
                }
            }
            if !current(world, id, kind) {
                continue;
            }
            let entry = match species {
                Species::Human => "animscripts/scripted::main",
                Species::Dog => "animscripts/dog/dog_scripted::main",
                Species::Civilian => "animscripts/civilian/civilian_scripted::main",
            };
            world
                .resource_mut::<RoundScript>()
                .actors
                .get_mut(&id)
                .unwrap()
                .anim_script_kind = Some(kind);
            let serial = if kind == ControlKind::Custom {
                let (main, _) = world.resource::<RoundScript>().actors[&id]
                    .custom_anim
                    .ok_or("custom animation has no script function")?;
                super::super::animations::stop_actor_motion(world, id);
                run_function(world, id, main, i64::from(now))?
            } else {
                crate::script::runtime::run_now_owned(
                    world,
                    entry,
                    Value::Object(id),
                    vec![],
                    i64::from(now),
                )
                .map_err(|e| e.to_string())?
            };
            if let Some(actor) = world.resource_mut::<RoundScript>().actors.get_mut(&id) {
                actor.anim_script_thread = Some(serial);
            } else {
                continue;
            }
        }
        if !current(world, id, kind) {
            continue;
        }
        let thread = world.resource::<RoundScript>().actors[&id].anim_script_thread;
        let running =
            thread.is_some_and(|serial| crate::script::runtime::engine_thread_alive(world, serial));
        if !running
            || (kind == ControlKind::Scripted
                && !super::super::animations::actor_motion_active(world, id))
        {
            super::super::animations::stop_actor_motion(world, id);
            pop(world, id);
        }
    }
    super::super::animations::apply_scripted_motion(world)?;
    crate::script::runtime::deliver_external(world, i64::from(now));
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "animcustom", custom);
    registry.register(
        Namespace::Method,
        "isinscriptedstate",
        |world, receiver, _| {
            let id = actor_id(world, receiver)?;
            Ok(Value::Int(i32::from(
                world.resource::<RoundScript>().actors[&id]
                    .scripted_control
                    .is_some(),
            )))
        },
    );
}
