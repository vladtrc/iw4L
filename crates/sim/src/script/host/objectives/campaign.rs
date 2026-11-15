use bevy_ecs::prelude::World;

use super::super::args::{int, string, vector};
use super::{ScriptObjective, index, objective};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use crate::{CompassObjective, ObjectiveListRow, ObjectiveState};

const POSITIONS: usize = 8;

#[derive(Clone, Debug, Default)]
pub(crate) struct SpObjective {
    text: Option<(String, String)>,
    positions: [Option<[f32; 3]>; POSITIONS - 1],
    offset: [f32; 3],
    pointer: Option<String>,
}

fn active(world: &World) -> bool {
    world.resource::<RoundScript>().campaign.active
}

fn announce(world: &mut World, template: String, arg: String) {
    FrameWorld::from_world(world).push_print(crate::PendingPrint {
        recipient: None,
        bold: true,
        chat: false,
        template,
        arg,
    });
}

fn announce_state(world: &mut World, state: ObjectiveState, text: Option<(String, String)>) {
    let parts = match state {
        ObjectiveState::Active | ObjectiveState::Current => {
            text.unwrap_or_else(|| ("GAME_OBJECTIVESUPDATED".into(), String::new()))
        }
        ObjectiveState::Done => ("GAME_OBJECTIVECOMPLETED".into(), String::new()),
        ObjectiveState::Failed => ("GAME_OBJECTIVEFAILED".into(), String::new()),
        _ => return,
    };
    announce(world, parts.0, parts.1);
}

pub(super) fn add(world: &mut World, index: u8, args: &[Value]) -> Result<Value, String> {
    let text = super::super::hud::print_parts(world, &args[2..3])?;
    let state = objective(world, index).state;
    objective(world, index).sp = SpObjective {
        text: Some(text.clone()),
        ..Default::default()
    };
    if let Some(target) = args.get(3) {
        super::place(world, index, target)?;
    }
    if args.len() > 4 {
        objective(world, index).icon = string(args, 4)?;
    }
    announce_state(world, state, Some(text));
    Ok(Value::Undefined)
}

pub(super) fn state_changed(world: &mut World, index: u8) {
    if active(world) {
        let state = objective(world, index).state;
        announce_state(world, state, None);
    }
}

pub(super) fn entity_offset(world: &mut World, index: u8, args: &[Value]) -> Result<(), String> {
    if args.len() > 2 {
        objective(world, index).sp.offset = vector(args, 2)?;
    }
    Ok(())
}

fn set_text(world: &mut World, args: &[Value], message: bool) -> Result<Value, String> {
    let index = index(args)?;
    let text = super::super::hud::print_parts(world, &args[1..])?;
    let state = {
        let mut row = objective(world, index);
        row.sp.text = Some(text.clone());
        row.state
    };
    if message && state != ObjectiveState::Done {
        announce_state(world, state, Some(text));
    }
    Ok(Value::Undefined)
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Function, "objective_string", |world, _, args| {
        set_text(world, args, true)
    });
    registry.register(Function, "objective_string_nomessage", |world, _, args| {
        set_text(world, args, false)
    });
    registry.register(Function, "objective_current", |world, _, args| {
        let current = (0..args.len())
            .map(|k| index(&args[k..]))
            .collect::<Result<Vec<u8>, String>>()?;
        let mut runtime = world.resource_mut::<RoundScript>();
        for (number, row) in runtime.engine.objectives.iter_mut() {
            if row.state == ObjectiveState::Current && !current.contains(number) {
                row.state = ObjectiveState::Active;
            }
        }
        for number in current {
            runtime.engine.objectives.entry(number).or_default().state = ObjectiveState::Current;
        }
        drop(runtime);
        announce_state(world, ObjectiveState::Current, None);
        Ok(Value::Undefined)
    });
    registry.register(
        Function,
        "objective_additionalposition",
        |world, _, args| {
            let index = index(args)?;
            let slot = int(args, 1)?;
            if !(0..POSITIONS as i32).contains(&slot) {
                return Err(format!(
                    "index {slot} is an illegal position index. Valid indexes are 0 to {}",
                    POSITIONS - 1
                ));
            }
            let at = vector(args, 2)?;
            match slot {
                0 => super::place(world, index, &Value::Vector(at))?,
                _ => objective(world, index).sp.positions[slot as usize - 1] = Some(at),
            }
            Ok(Value::Undefined)
        },
    );
    registry.register(
        Function,
        "objective_setpointertextoverride",
        |world, _, args| {
            let index = index(args)?;
            let label = match args.get(1) {
                Some(_) => Some(super::super::hud::print_parts(world, &args[1..2])?.0),
                None => None,
            };
            objective(world, index).sp.pointer = label;
            Ok(Value::Undefined)
        },
    );
}

pub(super) fn publish(
    world: &mut World,
    rows: &[(u8, ScriptObjective)],
) -> Option<(Vec<CompassObjective>, Vec<ObjectiveListRow>)> {
    if !active(world) {
        return None;
    }
    let mut out = Vec::new();
    let mut list = Vec::new();
    for (index, row) in rows {
        if !matches!(row.state, ObjectiveState::Empty | ObjectiveState::Invisible) {
            let (template, arg) = row.sp.text.clone().unwrap_or_default();
            list.push(ObjectiveListRow {
                index: *index,
                state: row.state,
                template,
                arg,
            });
        }
        if row.state != ObjectiveState::Current {
            continue;
        }
        let first = match &row.entity {
            Some(Value::Object(id)) if world.resource::<RoundScript>().live(id) => {
                match super::super::players::entity_field(world, *id, "origin") {
                    Value::Vector(at) => Some(std::array::from_fn(|k| at[k] + row.sp.offset[k])),
                    _ => None,
                }
            }
            Some(_) => None,
            None => Some(row.origin),
        };
        for origin in first
            .into_iter()
            .chain(row.sp.positions.iter().flatten().copied())
        {
            out.push(CompassObjective {
                index: *index,
                state: row.state,
                origin,
                team: row.team,
                icon: match row.icon.as_str() {
                    "" => "objective".into(),
                    icon => icon.into(),
                },
                viewer: row.viewer,
                label: row.sp.pointer.clone().unwrap_or_default(),
                follows: row.entity.is_some(),
            });
        }
    }
    Some((out, list))
}
