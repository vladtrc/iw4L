use super::iw4_natives::string;
use super::natives_math::{arg, int, vector};
use super::runtime::type_name;
use super::*;
use crate::frame::FrameWorld;
use crate::{CompassObjective, ObjectiveMatch, ObjectiveState};
use bevy_ecs::prelude::World;
use gamemode_iw4::Team;

const MAX_OBJECTIVES: i32 = 32;
const ENGINE_SERVER_INFO: &[&str] = &["ui_bomb_timer", "mapname", "g_gametype"];

#[derive(Clone, Debug, Default)]
pub(super) struct ScriptObjective {
    state: ObjectiveState,
    origin: [f32; 3],
    entity: Option<Value>,
    team: Team,
    icon: String,
}

fn index(args: &[Value]) -> Result<u8, String> {
    let index = int(args, 0)?;
    if !(0..MAX_OBJECTIVES).contains(&index) {
        return Err(format!("index {index} is an illegal objective index"));
    }
    Ok(index as u8)
}

fn state(args: &[Value], at: usize) -> Result<ObjectiveState, String> {
    let name = string(args, at)?;
    ObjectiveState::from_script(&name).ok_or_else(|| format!("Illegal objective state \"{name}\""))
}

fn objective(world: &mut World, index: u8) -> bevy_ecs::world::Mut<'_, ScriptObjective> {
    world
        .resource_mut::<Runtime>()
        .map_unchanged(|runtime| runtime.engine.objectives.entry(index).or_default())
}

fn place(world: &mut World, index: u8, target: &Value) -> Result<(), String> {
    match target {
        Value::Vector(origin) => {
            let mut row = objective(world, index);
            row.origin = *origin;
            row.entity = None;
        }
        Value::Object(_) => objective(world, index).entity = Some(target.clone()),
        other => return Err(format!("objective position cannot be {}", type_name(other))),
    }
    Ok(())
}

pub(super) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    registry.register(Function, "objective_add", |world, _, args| {
        let index = index(args)?;
        let state = state(args, 1)?;
        *objective(world, index) = ScriptObjective {
            state,
            ..Default::default()
        };
        if let Some(target) = args.get(2) {
            place(world, index, target)?;
        }
        if args.len() > 3 {
            objective(world, index).icon = string(args, 3)?;
        }
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_delete", |world, _, args| {
        let index = index(args)?;
        world
            .resource_mut::<Runtime>()
            .engine
            .objectives
            .remove(&index);
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_state", |world, _, args| {
        let index = index(args)?;
        objective(world, index).state = state(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_icon", |world, _, args| {
        let index = index(args)?;
        objective(world, index).icon = string(args, 1)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_position", |world, _, args| {
        let index = index(args)?;
        let origin = vector(args, 1)?;
        place(world, index, &Value::Vector(origin))?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_onentity", |world, _, args| {
        let index = index(args)?;
        let target = arg(args, 1)?.clone();
        if !matches!(target, Value::Object(_)) {
            return Err(format!("{} is not an entity", type_name(&target)));
        }
        place(world, index, &target)?;
        Ok(Value::Undefined)
    });
    registry.register(Function, "objective_team", |world, _, args| {
        let index = index(args)?;
        let team = match string(args, 1)?.as_str() {
            "axis" => Team::Axis,
            "allies" => Team::Allies,
            "none" | "free" | "neutral" => Team::Free,
            other => return Err(format!("'{other}' is an illegal team string")),
        };
        objective(world, index).team = team;
        Ok(Value::Undefined)
    });
}

pub(super) fn publish(world: &mut World) {
    let rows: Vec<(u8, ScriptObjective)> = world
        .resource::<Runtime>()
        .engine
        .objectives
        .iter()
        .map(|(index, row)| (*index, row.clone()))
        .collect();
    let mut compass = Vec::with_capacity(rows.len());
    for (index, row) in rows {
        let origin = match &row.entity {
            Some(Value::Object(id)) if world.resource::<Runtime>().objects.contains_key(id) => {
                match super::players::entity_field(world, *id, "origin") {
                    Value::Vector(origin) => origin,
                    _ => row.origin,
                }
            }
            _ => row.origin,
        };
        compass.push(CompassObjective {
            index,
            state: row.state,
            origin,
            team: row.team,
            icon: row.icon,
        });
    }
    let runtime = world.resource::<Runtime>();
    let score = |team: &str| runtime.engine.team_scores.get(team).copied().unwrap_or(0);
    let scores = [0, score("axis"), score("allies")];
    let engine = ENGINE_SERVER_INFO
        .iter()
        .filter(|name| !runtime.server_info.contains(**name))
        .filter_map(|name| Some((name.to_string(), runtime.dvars.get(*name)?.clone())));
    let server_info = runtime
        .server_info
        .iter()
        .map(|name| {
            let value = runtime.dvars.get(name).cloned().unwrap_or_default();
            (name.clone(), value)
        })
        .chain(engine)
        .collect();
    let game_end_time = runtime.engine.game_end_time;
    FrameWorld::from_world(world).objectives = ObjectiveMatch {
        scores,
        compass,
        server_info,
        game_end_time,
    };
}
