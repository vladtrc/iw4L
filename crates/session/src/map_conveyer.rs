use assets::MapUseTrigger;

pub(crate) fn prepare(zone: &str, triggers: &[MapUseTrigger]) -> Option<sim::RadiationConveyer> {
    if zone != "mp_radiation" {
        return None;
    }
    let trigger = triggers
        .iter()
        .find(|t| gamemode_iw4::is_conveyer_trigger(&t.targetname))?;
    let cmodel: u32 = trigger
        .model
        .strip_prefix('*')
        .and_then(|s| s.parse().ok())?;
    let angles = trigger.target_struct_angles.unwrap_or(trigger.angles);
    Some(sim::RadiationConveyer {
        origin: trigger.origin,
        half: [0.0; 3],
        vector: gamemode_iw4::radiation_conveyer_force_vector(angles),
        cmodel,
    })
}

pub(crate) fn install(world: &mut sim::SimWorld, mut belt: sim::RadiationConveyer) {
    let Some(cmodel) = world.clip_cmodels().models.get(belt.cmodel as usize) else {
        return;
    };
    for i in 0..3 {
        belt.origin[i] += (cmodel.mins[i] + cmodel.maxs[i]) * 0.5;
        belt.half[i] = (cmodel.maxs[i] - cmodel.mins[i]) * 0.5;
    }
    diag::info!(
        Sim,
        "Radiation conveyer installed: origin={:?} half={:?} vector={:?}",
        belt.origin,
        belt.half,
        belt.vector
    );
    world.radiation_conveyer = Some(belt);
}
