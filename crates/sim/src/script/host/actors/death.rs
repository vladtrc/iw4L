use super::*;
use crate::script::host::entity_damage::ObjectHit;

pub(super) const FIELDS: [&str; 7] = [
    "damagetaken",
    "damageyaw",
    "damagedir",
    "damagelocation",
    "damageweapon",
    "damagemod",
    "forceragdollimmediate",
];

#[derive(Clone, Debug)]
pub(crate) struct Damage {
    amount: i32,
    yaw: i32,
    direction: [f32; 3],
    location: &'static str,
    weapon: String,
    means: &'static str,
    force_ragdoll: bool,
}
impl Default for Damage {
    fn default() -> Self {
        Self {
            amount: 0,
            yaw: 0,
            direction: [0.; 3],
            location: "none",
            weapon: String::new(),
            means: "MOD_UNKNOWN",
            force_ragdoll: false,
        }
    }
}
impl Damage {
    pub(super) fn load(&self, field: &str) -> Option<Value> {
        Some(match field {
            "damagetaken" => Value::Int(self.amount),
            "damageyaw" => Value::Int(self.yaw),
            "damagedir" => Value::Vector(self.direction),
            "damagelocation" => Value::string(self.location),
            "damageweapon" => Value::string(&self.weapon),
            "damagemod" => Value::string(self.means),
            "forceragdollimmediate" => Value::Int(i32::from(self.force_ragdoll)),
            _ => return None,
        })
    }
    pub(super) fn store(&mut self, field: &str, value: &Value) -> Result<bool, String> {
        if field == "forceragdollimmediate" {
            self.force_ragdoll = super::super::args::int(std::slice::from_ref(value), 0)? != 0;
            return Ok(true);
        }
        if FIELDS.contains(&field) {
            return Err(format!("actor {field} is read-only"));
        }
        Ok(false)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Death {
    started_at: i32,
    script_started: bool,
    cleaned: bool,
    corpse_contents: bool,
}

pub(crate) fn begin(
    world: &mut World,
    id: u64,
    hit: &ObjectHit,
    weapon: &str,
    attacker: &Value,
) -> Result<(), String> {
    let now = super::super::players::now_ms(world) as i32;
    {
        let mut runtime = world.resource_mut::<RoundScript>();
        if runtime.actors[&id].death.is_some() {
            return Ok(());
        }
        let Value::Vector(angles) = runtime.object_field(id, "angles") else {
            return Err("actor angles are not a vector".into());
        };
        let actor = runtime.actors.get_mut(&id).unwrap();
        actor.damage.amount = hit.amount;
        let turns = (hit.dir[1].atan2(hit.dir[0]).to_degrees() - angles[1]) * (1. / 360.);
        actor.damage.yaw = ((turns - (turns + 0.5).floor()) * 360.) as i32;
        actor.damage.direction = hit.dir;
        actor.damage.location = weapon_iw4::HITLOC_NAMES
            .get(usize::from(hit.hitloc))
            .copied()
            .ok_or("invalid actor hit location")?;
        actor.damage.weapon = weapon.into();
        actor.damage.means = hit.means;
        actor.death = Some(Death {
            started_at: now,
            script_started: false,
            cleaned: false,
            corpse_contents: false,
        });
        actor.alive = false;
        super::bad_places::release(actor);
        super::reacquire::clear(actor, false);
        super::cover::clear(actor);
        super::arrival::clear(actor);
        super::traversal::clear(actor);
        actor.delayed_death = false;
        actor.scripted_control = None;
        actor.goals.keep_claimed = false;
        actor.goals.keep_claimed_if_valid = false;
        actor.claimed_node = None;
        actor.at_claim_node = false;
        actor.motion.trace_mask &= !4;
        actor.orientation.select_stop();
        if let Some(state) = &mut actor.initial_state {
            state.state = 6;
            state.substate = 200;
            state.next_think_at = now;
        }
        runtime.sentients.get_mut(&id).unwrap().last_attacker = match attacker {
            Value::Object(id) => Some(*id),
            _ => None,
        };
        runtime.set_object_field(id, "targetname", Value::Undefined);
    }
    super::turret::stop(world, id)?;
    super::grenades::release(world, id)?;
    scripted::kill_anim_script(world, id);
    super::super::animations::stop_actor_motion(world, id);
    let team = world.resource::<RoundScript>().sentients[&id].team;
    if (1..=3).contains(&team)
        && let Some(node) = super::super::sentients::nearest::query(world, id)?
    {
        world
            .resource_mut::<RoundScript>()
            .path_nodes
            .mark_connected_dangerous(node, team, now)?;
    }
    Ok(())
}

fn cleanup(world: &mut World, id: u64) -> Result<(), String> {
    let (team, origin) = {
        let mut runtime = world.resource_mut::<RoundScript>();
        let team = runtime.sentients[&id].team;
        let Value::Vector(origin) = runtime.object_field(id, "origin") else {
            return Err("actor origin is not a vector".into());
        };
        (team, origin)
    };
    super::super::ai_events::point(
        world,
        super::super::ai_events::Event::Death,
        Some(id),
        origin,
        1 << team,
    );
    let mut runtime = world.resource_mut::<RoundScript>();
    suppression::dissociate(&mut runtime, id);
    targeting::dissociate(&mut runtime, id);
    let actor = runtime.actors.get_mut(&id).unwrap();
    actor.navigation = Default::default();
    actor.death.as_mut().unwrap().cleaned = true;
    if let Some(state) = &mut actor.initial_state {
        state.substate = 201;
    }
    Ok(())
}

pub(crate) fn advance(world: &mut World, now: i32) -> Result<(), String> {
    let _span = perf::span_handle!("actor_death", "iw4l.sim").enter();
    let ids: Vec<_> = world
        .resource::<RoundScript>()
        .actors
        .iter()
        .filter_map(|(&id, actor)| actor.death.as_ref().map(|_| (actor.slot, id)))
        .collect();
    let mut ids = ids;
    ids.sort_unstable();
    for (_, id) in ids {
        let death = world.resource::<RoundScript>().actors[&id]
            .death
            .clone()
            .unwrap();
        if !death.script_started {
            crate::script::runtime::deliver_external(world, i64::from(now));
            crate::script::runtime::run_current_threads(world, i64::from(now));
            scripted::finish_previous(world, id, i64::from(now))?;
            let species = {
                let mut runtime = world.resource_mut::<RoundScript>();
                let Some(actor) = runtime.actors.get_mut(&id) else {
                    continue;
                };
                actor.anim_script_kind = Some(scripted::ControlKind::Death);
                actor.script_anim_mode = anim_mode::Mode::None;
                actor.move_mode = 0;
                actor.orientation.clear_script();
                actor.death.as_mut().unwrap().script_started = true;
                actor.species
            };
            let module = match species {
                Species::Human => "animscripts/death::main",
                Species::Dog => "animscripts/dog/dog_death::main",
                Species::Civilian => "animscripts/civilian/civilian_death::main",
            };
            let serial = crate::script::runtime::run_now_owned(
                world,
                module,
                Value::Object(id),
                vec![],
                i64::from(now),
            )
            .map_err(|e| e.to_string())?;
            if let Some(actor) = world.resource_mut::<RoundScript>().actors.get_mut(&id) {
                actor.anim_script_thread = Some(serial);
            }
            continue;
        }
        if now.wrapping_sub(death.started_at) >= 800 && !death.cleaned {
            cleanup(world, id)?;
        }
        if now.wrapping_sub(death.started_at) >= 500 && !death.corpse_contents {
            let mut runtime = world.resource_mut::<RoundScript>();
            runtime.entities.get_mut(&id).unwrap().contents = 0x04002000;
            runtime
                .actors
                .get_mut(&id)
                .unwrap()
                .death
                .as_mut()
                .unwrap()
                .corpse_contents = true;
        }
        let thread = world.resource::<RoundScript>().actors[&id].anim_script_thread;
        if !thread.is_some_and(|serial| crate::script::runtime::engine_thread_alive(world, serial))
        {
            if !world.resource::<RoundScript>().actors[&id]
                .death
                .as_ref()
                .unwrap()
                .cleaned
            {
                cleanup(world, id)?;
            }
            corpses::handoff(world, id, now)?;
        }
    }
    Ok(())
}
