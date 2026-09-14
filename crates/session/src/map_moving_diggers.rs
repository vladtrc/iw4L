use assets::{MapScriptStruct, PreparedWorld};

fn model_id(instance: &assets::ScriptModelSceneInstance) -> sim::ScriptModelId {
    sim::ScriptModelId::from_authored_source_ordinal(instance.id.source_ordinal())
}

fn follow_path(structs: &[MapScriptStruct], first: &str) -> Option<Vec<([f32; 3], u32)>> {
    let mut name = first;
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    loop {
        if seen.contains(&name) || seen.len() >= 32 {
            return None;
        }
        seen.push(name);
        let row = structs.iter().find(|s| s.targetname == name)?;
        let seconds = row.script_int.filter(|n| *n >= 0)?;
        let duration_ms = u32::try_from(seconds).ok()?.saturating_mul(1_000);
        out.push((row.origin, duration_ms));
        if row.target.is_empty() {
            return Some(out);
        }
        name = row.target.as_str();
    }
}

pub(crate) fn prepare(zone: &str, world: &PreparedWorld) -> Vec<sim::RadiationMovingDigger> {
    if zone != "mp_radiation" {
        return Vec::new();
    }
    let mut out = Vec::new();
    for instance in world
        .script_model_instances
        .iter()
        .filter(|m| gamemode_iw4::is_moving_digger(&m.metadata.targetname))
    {
        if instance.metadata.target.is_empty() {
            continue;
        }
        let Some(legs) = follow_path(&world.script_structs, &instance.metadata.target) else {
            continue;
        };
        out.push(sim::RadiationMovingDigger {
            id: model_id(instance),
            start: instance.transform.translation.to_array(),
            legs,
        });
    }
    out
}

pub(crate) fn install(world: &mut sim::SimWorld, diggers: Vec<sim::RadiationMovingDigger>) {
    if !diggers.is_empty() {
        diag::info!(Sim, "Radiation moving diggers installed: {}", diggers.len());
    }
    world.radiation_moving_diggers = diggers;
}
