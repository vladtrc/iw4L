use assets::PreparedWorld;

pub(crate) fn prepare(
    zone: &str,
    world: &PreparedWorld,
    triggers: &[assets::MapUseTrigger],
) -> Result<Option<sim::MapDoors>, String> {
    if zone != "mp_radiation" {
        return Ok(None);
    }
    let mut doors = sim::MapDoors::default();
    for i in 0..2 {
        let switch_name = format!("switch_trigger{}", i + 1);
        let trigger = triggers
            .iter()
            .find(|t| t.targetname == switch_name)
            .ok_or_else(|| format!("Radiation missing {switch_name}"))?;

        let handle: u32 = trigger
            .model
            .strip_prefix('*')
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("Radiation invalid switch brush {}", trigger.model))?;
        doors.switches[i] = sim::DoorSwitch {
            origin: trigger.origin,
            half: [0.0; 3],
            cmodel: handle,
        };
        let name = format!("big_door{}", i + 1);
        let model = world
            .script_model_instances
            .iter()
            .find(|m| m.metadata.targetname == name)
            .ok_or_else(|| format!("Radiation missing {name}"))?;
        let brush = world
            .script_brush_models
            .iter()
            .find(|b| b.targetname == format!("{name}_clip"))
            .ok_or_else(|| format!("Radiation missing {name}_clip"))?;
        if model.transform.translation.to_array() != brush.origin || brush.angles != [0.0; 3] {
            return Err(format!(
                "Radiation {name} changed its authored shared pivot / parent orientation"
            ));
        }
        let (yaw, pitch, roll) = model.transform.rotation.to_euler(bevy::math::EulerRot::ZYX);
        doors.leaves[i] = sim::DoorLeaf {
            model: model.id.source_ordinal(),
            brush: brush.source_ordinal,
            cmodel: brush.cmodel_handle,
            origin: brush.origin,
            angles: brush.angles,
            model_angles: [pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees()],
        };
    }

    doors.sound_origin = triggers
        .iter()
        .find(|t| t.targetname == "center_death_trig")
        .ok_or("Radiation missing center_death_trig")?
        .origin;
    Ok(Some(doors))
}

pub(crate) fn install(world: &mut sim::SimWorld, mut doors: sim::MapDoors) -> Result<(), String> {
    for switch in &mut doors.switches {
        let cmodel = world
            .clip_cmodels()
            .models
            .get(switch.cmodel as usize)
            .ok_or_else(|| format!("Radiation missing switch cmodel {}", switch.cmodel))?;
        for i in 0..3 {
            switch.origin[i] += (cmodel.mins[i] + cmodel.maxs[i]) * 0.5;
            switch.half[i] = (cmodel.maxs[i] - cmodel.mins[i]) * 0.5;
        }
    }
    for leaf in &doors.leaves {
        world
            .spawn_script_mover(
                sim::ScriptModelId::from_authored_source_ordinal(leaf.brush),
                leaf.origin,
                leaf.angles,
            )
            .map_err(|e| format!("Radiation door allocation: {e:?}"))?;
    }
    diag::info!(
        Sim,
        "Radiation door script installed: switches={:?}, leaves={:?}",
        doors.switches,
        doors.leaves
    );
    world.map_doors = Some(doors);
    Ok(())
}
