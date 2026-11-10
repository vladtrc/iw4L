use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::script::host;
use crate::script::{
    Fault, LevelData, Location, NativeRegistry, Program, RoundScript, ThreadState, Value,
};

use super::{INSTRUCTION_BUDGET, execute, new_thread};

/// Runs during install, before the map's `script_struct` blocks are appended to `level.struct`.
pub const STRUCT_INIT: &str = "codescripts/struct::initstructs";

pub(crate) fn reset_round(world: &mut World) {
    world.remove_resource::<crate::campaign_checkpoint::SaveMemory>();
    world.remove_resource::<crate::campaign_reload::CampaignReload>();
    let round = RoundScript::new(world.resource::<crate::script::MatchScript>());
    world.insert_resource(round);
    world.insert_resource(host::mechanics::Mechanics::default());
}

pub(crate) fn reset(world: &mut World) {
    world.insert_resource(crate::script::MatchScript::default());
    reset_round(world);
}

pub(crate) fn install(
    world: &mut World,
    program: Program,
    natives: NativeRegistry,
    level: LevelData,
) -> Result<(), Fault> {
    let plan = host::restart::RestartPlan {
        anim_trees: level.anim_trees,
        profile: level.profile,
        campaign_player: level.campaign_player,
        path_nodes: level.path_nodes,
        absent_effects: level.absent_effects,
        natives: natives.clone(),
        entities: Arc::new(level.entities),
        tables: Arc::new(level.tables),
        keys: Arc::new(level.keys),
        entries: Vec::new(),
        schemas: level.schemas,
        player_data_defaults: level.player_data_defaults.map(Arc::new),
    };
    install_level(world, Arc::new(program), plan)
}

pub(crate) fn install_level(
    world: &mut World,
    program: Arc<Program>,
    plan: host::restart::RestartPlan,
) -> Result<(), Fault> {
    let mut natives = plan.natives.clone();
    let location = Location {
        module: "<runtime>".into(),
        function: "install".into(),
        line: 0,
        column: 0,
    };
    let family = crate::script::families::script_family(program.rules())
        .map_err(|message| Fault::at(&location, message))?;
    (family.register_natives)(&mut natives);
    if world.resource::<RoundScript>().program.is_some() {
        return Err(Fault::at(
            &location,
            "program already installed; reset the match before replacing scripts",
        ));
    }
    if plan.profile != asset_core::RuntimeProfile::Campaign {
        natives.validate_program(&program)?;
    }
    world
        .resource_mut::<crate::PersistentDataStore>()
        .install_schemas(plan.schemas.clone())
        .map_err(|error| Fault::at(&location, format!("persistent data schema: {error:?}")))?;
    world
        .resource_mut::<crate::PersistentDataStore>()
        .install_defaults(plan.player_data_defaults.clone());
    let bound = program
        .natives
        .iter()
        .map(|b| natives.get(b.namespace, b.name))
        .collect();
    let mut runtime = world.resource_mut::<RoundScript>();
    (family.initialize_round)(&mut runtime.extensions);
    runtime.anim_trees = plan.anim_trees.clone();
    runtime.profile = plan.profile;
    runtime.program = Some(program.clone());
    runtime.natives = bound;
    runtime.objects.insert(0, BTreeMap::new());
    runtime.objects.insert(1, BTreeMap::new());
    runtime.objects.insert(2, BTreeMap::new());
    runtime.next_object = 3;
    runtime.next_entity_number = playerstate_iw4::GENTITY_SPAWN_BASE;
    runtime.tables = plan.tables.clone();
    runtime.rng = u32::from_le_bytes(program.fingerprint()[..4].try_into().unwrap()) | 1;
    runtime.loading = true;
    world.insert_resource(natives);
    if world.resource::<RoundScript>().profile == asset_core::RuntimeProfile::Campaign {
        host::natives::iw4::saved::install(world);
        world.init_resource::<crate::campaign_checkpoint::SaveMemory>();
        crate::campaign_reload::reset(world);
    }
    if let Some(&function) = program.names.get(STRUCT_INIT) {
        let mut thread = new_thread(world, &program, function, Value::Object(0), Vec::new())
            .map_err(|m| Fault::at(&location, m))?;
        world.resource_mut::<RoundScript>().budget = INSTRUCTION_BUDGET;
        thread.state = ThreadState::Runnable;
        execute(world, &program, &mut thread, 0);
        let mut runtime = world.resource_mut::<RoundScript>();
        if let Some(fault) = runtime.fault.clone() {
            return Err(fault);
        }
        if thread.state != ThreadState::Complete {
            return Err(Fault::at(&location, format!("{STRUCT_INIT} must not wait")));
        }
        runtime.started = false;
    }
    let entities = plan.entities.clone();
    let keys = plan.keys.clone();
    let path_nodes = plan.path_nodes.clone();
    let campaign_player = plan.campaign_player;
    world.resource_mut::<RoundScript>().restart = Some(Arc::new(plan));
    world
        .resource_mut::<RoundScript>()
        .spawn_map_entities(&entities, &keys)
        .map_err(|m| Fault::at(&location, m))?;
    host::weapons::items::initialize_map(world).map_err(|m| Fault::at(&location, m))?;
    host::path_nodes::install(world.resource_mut::<RoundScript>().into_inner(), path_nodes)
        .map_err(|message| Fault::at(&location, message))?;
    host::spawners::finish_spawning(world)?;
    host::presence::initialize_map_models(world);
    host::path_nodes::blockers::initialize(world)
        .map_err(|message| Fault::at(&location, message))?;
    if let Some(client) = campaign_player {
        host::players::campaign::initialize(world, client)
            .map_err(|message| Fault::at(&location, message))?;
    }
    Ok(())
}
