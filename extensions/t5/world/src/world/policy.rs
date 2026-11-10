use asset_world::*;

pub const POLICY: WorldFamily = WorldFamily {
    draw: WorldDrawPolicy {
        family: crate::ID,
        resolve_specular_env: false,
        lightmap_requires_image: true,
        decode_color_at_convert: true,
        sun_sample_size_near: 0.25,
    },
    complete_objective_models: None,
    normalize_entities,
    spawn_points,
};

fn normalize_entities(text: &str) -> String {
    normalize_map_entities(
        text,
        &EntityRules {
            overtime: None,
            gametype_aliases: &[("dem", "dd"), ("tdm", "war"), ("koth", "hq")],
            augment: |_, _| {},
        },
    )
}

pub fn spawn_points(text: &str) -> Vec<SpawnPoint> {
    let names: Vec<_> = REFERENCE_MP_SPAWN_CLASSNAMES
        .iter()
        .map(|name| name.replace("mp_dd_", "mp_dem_"))
        .collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    let mut spawns = parse_spawn_points(text, &names);
    for spawn in &mut spawns {
        if let Some(suffix) = spawn.classname.strip_prefix("mp_dem_") {
            spawn.classname = format!("mp_dd_{suffix}");
        }
    }
    spawns
}
