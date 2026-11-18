use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DogProneStartup {
    pub orient_pitch: bool,
    pub prone: bool,
    pub started_at: i32,
    pub transition_ms: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InitialState {
    pub state: u8,
    pub substate: u16,
    pub next_think_at: i32,
    pub dog_prone: Option<DogProneStartup>,
}

fn run_entry(world: &mut World, id: u64, entry: &str, now: i64) -> Result<(), String> {
    let errors = world.resource::<RoundScript>().errors.clone();
    crate::script::runtime::run_now(world, entry, Value::Object(id), vec![], now)
        .map_err(|error| error.to_string())?;
    let runtime = world.resource::<RoundScript>();
    if let Some(((site, message), _)) = runtime
        .errors
        .iter()
        .find(|(key, hits)| errors.get(*key) != Some(*hits))
    {
        return Err(format!("{entry} failed at {site}: {message}"));
    }
    if !runtime.can_receive_call(&id) {
        return Err(format!("actor was deleted during {entry}"));
    }
    Ok(())
}

pub(crate) fn start_type(world: &mut World, id: u64, now: i32) -> Result<(), String> {
    let actor = world
        .resource::<RoundScript>()
        .actors
        .get(&id)
        .ok_or("entity is not an actor")?;
    if actor.type_started {
        return Ok(());
    }
    super::super::animations::initialize_actor_tree(
        world.resource_mut::<RoundScript>().into_inner(),
        id,
    );
    if !world.resource::<RoundScript>().actors[&id].physics_initialized {
        super::spatial::initialize_physics(world, id)?;
    }
    let module = super::super::spawners::aitype_module(
        &world.resource::<RoundScript>().entities[&id].classname,
    )
    .ok_or("actor has no authored type module")?;
    run_entry(world, id, &format!("{module}::main"), i64::from(now))?;
    world
        .resource_mut::<RoundScript>()
        .actors
        .get_mut(&id)
        .unwrap()
        .type_started = true;
    Ok(())
}

pub(crate) fn start_animation(world: &mut World, id: u64, now: i32) -> Result<(), String> {
    let actor = world
        .resource::<RoundScript>()
        .actors
        .get(&id)
        .ok_or("entity is not an actor")?;
    if actor.script_initialized {
        return Ok(());
    }
    if !actor.type_started {
        return Err("actor type main must start before its animation hook".into());
    }
    let species = world.resource::<RoundScript>().actors[&id].species;
    let entry = match species {
        Species::Human => "animscripts/init::main",
        Species::Dog => "animscripts/dog/dog_init::main",
        Species::Civilian => "animscripts/civilian/civilian_init::main",
    };
    if species.humanoid() && !world.resource::<RoundScript>().animators.contains_key(&id) {
        return Err("actor slot animation tree generic_human is not loaded".into());
    }
    run_entry(world, id, entry, i64::from(now))?;
    let mut runtime = world.resource_mut::<RoundScript>();
    let actor = runtime
        .actors
        .get_mut(&id)
        .ok_or("actor was deleted during initialization")?;
    actor.initial_state = Some(InitialState {
        state: 1,
        substate: 100,
        next_think_at: now,
        dog_prone: (actor.species == Species::Dog).then_some(DogProneStartup {
            orient_pitch: true,
            prone: true,
            started_at: now,
            transition_ms: 500,
        }),
    });
    actor.script_initialized = true;
    Ok(())
}

pub(crate) fn initialize(world: &mut World, id: u64, now: i32) -> Result<(), String> {
    let result = start_type(world, id, now).and_then(|()| start_animation(world, id, now));
    if result.is_err() {
        let number = world
            .resource::<RoundScript>()
            .entities
            .get(&id)
            .filter(|entity| entity.presence.is_some())
            .map(|entity| entity.number);
        crate::script::runtime::raise(world, Value::Object(id), "death", vec![]);
        world.resource_mut::<RoundScript>().delete_entity(id);
        if let Some(number) = number {
            crate::frame::FrameWorld::from_world(world).remove_script_mover_by_number(number);
        }
    }
    result
}
