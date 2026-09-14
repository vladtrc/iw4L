use assets::PreparedWorld;

fn model_id(instance: &assets::ScriptModelSceneInstance) -> sim::ScriptModelId {
    sim::ScriptModelId::from_authored_source_ordinal(instance.id.source_ordinal())
}

fn model_angles(instance: &assets::ScriptModelSceneInstance) -> [f32; 3] {
    let (yaw, pitch, roll) = instance
        .transform
        .rotation
        .to_euler(bevy::math::EulerRot::ZYX);
    [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()]
}

pub(crate) fn prepare(zone: &str, world: &PreparedWorld) -> Vec<sim::RadiationDigger> {
    if zone != "mp_radiation" {
        return Vec::new();
    }
    let models = &world.script_model_instances;
    let mut out = Vec::new();
    for body in models
        .iter()
        .filter(|m| gamemode_iw4::is_digger_body(&m.metadata.targetname))
    {
        let Some(arm) = models.iter().find(|m| {
            !body.metadata.target.is_empty() && m.metadata.targetname == body.metadata.target
        }) else {
            continue;
        };
        let Some(blade) = models.iter().find(|m| {
            !arm.metadata.target.is_empty() && m.metadata.targetname == arm.metadata.target
        }) else {
            continue;
        };
        let blade_id = model_id(blade);
        let pieces = models
            .iter()
            .filter(|piece| {
                gamemode_iw4::is_digger_blade(&piece.metadata.targetname)
                    && model_id(piece) != blade_id
                    && (piece.metadata.target == blade.metadata.targetname
                        || piece.metadata.target.is_empty())
            })
            .map(|piece| (model_id(piece), model_angles(piece)))
            .collect();
        out.push(sim::RadiationDigger {
            body: model_id(body),
            arm: model_id(arm),
            blade: blade_id,
            pieces,
            body_angles: model_angles(body),
            arm_angles: model_angles(arm),
            blade_angles: model_angles(blade),
        });
    }
    out
}

pub(crate) fn install(world: &mut sim::SimWorld, diggers: Vec<sim::RadiationDigger>) {
    if !diggers.is_empty() {
        diag::info!(Sim, "Radiation diggers installed: {}", diggers.len());
    }
    world.radiation_diggers = diggers;
}
