use assets::{MapFacts, MapScriptFamily, ScriptSources};
pub const POLICY: MapScriptFamily = MapScriptFamily {
    compile: Some(compile),
    project_factions: true,
    builtins: BUILTINS,
};
const BUILTINS: &[(&str, &str)] = &[("iw4l_t6/equipment", include_str!("t6_equipment.gsc.txt"))];
fn compile(map_scripts: &ScriptSources, zone_name: &str, facts: &MapFacts) -> ScriptSources {
    let module = format!("maps/mp/{zone_name}");
    let main = map_scripts.text(&module).unwrap_or("");
    let mut declarations = declarations(main);
    declarations.set_teams(
        facts.script_sound.attackers.as_deref().unwrap_or("allies"),
        facts.script_sound.defenders.as_deref().unwrap_or("axis"),
    );
    let entities = map_scripts.entities().unwrap_or("").to_owned();
    let mut scripts = ScriptSources::default();
    let mut map_main = declarations.map_script(&entities);
    let end = map_main.rfind('}').expect("generated map main");
    map_main.insert_str(end, "\tthread iw4l_maps\\destructibles::main();\n");
    scripts.insert_source(
        "iw4l_maps/destructibles",
        include_str!("map_scripts/destructibles.gsc.txt").to_owned(),
    );
    if zone_name == "mp_radiation" {
        let end = map_main.rfind('}').expect("generated map main");
        map_main.insert_str(end, "\tthread iw4l_maps\\radiation::main();\n");
        scripts.insert_source(
            "iw4l_maps/radiation",
            include_str!("map_scripts/radiation.gsc.txt").to_owned(),
        );
    }
    scripts.insert_source(&module, map_main);
    scripts.set_entities(entities);
    scripts
}

fn declarations(source: &str) -> asset_world::MapDeclarations {
    let compass = source
        .split("setupMiniMap")
        .skip(1)
        .filter_map(|call| call.split('"').nth(1))
        .find(|name| name.starts_with("compass_map_") && !name.ends_with("_wager"))
        .map(str::to_owned);
    let mut declarations = asset_world::MapDeclarations::default();
    declarations.set_compass(compass);
    declarations
}
