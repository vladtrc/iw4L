use movement_iw4::SURFACE_TYPE_NAMES;

pub fn reference_explosion_surface_aliases(
    impact_type: i32,
    surface: u8,
) -> Option<(String, String)> {
    let prefix = match impact_type {
        8 => "grenade_explode",
        9 => "rocket_explode",
        _ => return None,
    };
    let surface = SURFACE_TYPE_NAMES.get(usize::from(surface))?;
    Some((format!("{prefix}_{surface}"), format!("{prefix}_default")))
}
