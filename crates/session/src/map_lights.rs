use assets::PreparedWorld;

pub(crate) fn prepare(zone: &str, world: &PreparedWorld) -> Option<sim::RadiationLights> {
    if zone != "mp_radiation" {
        return None;
    }
    let switch_count = world
        .script_structs
        .iter()
        .filter(|s| gamemode_iw4::is_switch_struct(&s.targetname))
        .count() as u32;
    let tunnel_count = world
        .script_structs
        .iter()
        .filter(|s| gamemode_iw4::is_tunnel_light(&s.targetname))
        .count() as u32;
    Some(sim::RadiationLights {
        switch_count,
        tunnel_count,
        ..Default::default()
    })
}

pub(crate) fn install(world: &mut sim::SimWorld, lights: sim::RadiationLights) {
    if lights.switch_count > 0 || lights.tunnel_count > 0 {
        diag::info!(
            Sim,
            "Radiation lights installed: switch_structs={} tunnel_structs={}",
            lights.switch_count,
            lights.tunnel_count
        );
    }
    world.radiation_lights = Some(lights);
}
