use assets::{MapFacts, MapScriptFamily, ScriptSources};
pub const POLICY: MapScriptFamily = MapScriptFamily {
    compile: Some(compile),
    project_factions: true,
    builtins: BUILTINS,
};
const BUILTINS: &[(&str, &str)] = &[];
fn compile(map_scripts: &ScriptSources, zone_name: &str, facts: &MapFacts) -> ScriptSources {
    let module = format!("maps/mp/{zone_name}");
    let main = map_scripts.text(&module).unwrap_or("");
    let mut declarations = declarations(main);
    declarations.set_teams(
        facts.script_sound.attackers.as_deref().unwrap_or("allies"),
        facts.script_sound.defenders.as_deref().unwrap_or("axis"),
    );
    let entities = map_scripts
        .entities()
        .map_or_else(String::new, entities_for_reference_rules);
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

pub fn entities_for_reference_rules(text: &str) -> String {
    type Block = Vec<(String, String)>;
    fn get<'a>(block: &'a Block, key: &str) -> Option<&'a str> {
        block
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    fn set(block: &mut Block, key: &str, value: String) {
        match block.iter_mut().find(|(k, _)| k == key) {
            Some((_, v)) => *v = value,
            None => block.push((key.to_owned(), value)),
        }
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut open: Option<Block> = None;
    for line in text.lines() {
        match (line.trim(), open.as_mut()) {
            ("{", None) => open = Some(Vec::new()),
            ("}", Some(_)) => blocks.extend(open.take()),
            (trimmed, Some(block)) => {
                let mut quoted = trimmed.split('"').skip(1).step_by(2);
                if let (Some(key), Some(value)) = (quoted.next(), quoted.next()) {
                    block.push((key.to_owned(), value.to_owned()));
                }
            }
            _ => {}
        }
    }

    let mut flag_visuals: Vec<(String, String)> = Vec::new();
    let mut spawned: Vec<Block> = Vec::new();
    for block in &mut blocks {
        let team = get(block, "script_team").map(str::to_owned);
        if let Some(suffix) = get(block, "classname").and_then(|c| c.strip_prefix("mp_dem_spawn_"))
        {
            set(block, "classname", format!("mp_dd_spawn_{suffix}"));
        }
        match (get(block, "targetname"), team.as_deref()) {
            (Some("bombzone_dem"), _) => set(block, "targetname", "dd_bombzone".to_owned()),
            (Some("ctf_flag_zone_trig"), Some(team)) => {
                set(block, "targetname", format!("ctf_zone_{team}"));
                if get(block, "height").is_none() {
                    set(block, "height", "128".to_owned());
                }
            }
            (Some("ctf_flag_pickup_trig"), Some(team)) => match get(block, "target") {
                Some(target) => flag_visuals.push((target.to_owned(), format!("ctf_flag_{team}"))),
                None => spawned.push(
                    [("classname", "script_model".to_owned())]
                        .into_iter()
                        .chain(
                            ["origin", "angles"]
                                .into_iter()
                                .filter_map(|key| Some((key, get(block, key)?.to_owned()))),
                        )
                        .chain([("targetname", format!("ctf_flag_{team}"))])
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect(),
                ),
            },
            _ => {}
        }
        if let Some(names) = get(block, "script_gameobjectname") {
            let mut tokens: Vec<&str> = Vec::new();
            for token in names.split_whitespace() {
                let token = match token {
                    "bombzone_dem" => "dd_bombzone",
                    token => token,
                };
                tokens.push(token);
                tokens.extend(match token {
                    "dem" => Some("dd"),
                    "tdm" => Some("war"),
                    "koth" => Some("hq"),
                    _ => None,
                });
            }
            let mut seen = std::collections::BTreeSet::new();
            tokens.retain(|token| seen.insert(*token));
            let names = tokens.join(" ");
            set(block, "script_gameobjectname", names);
        }
    }
    for block in &mut blocks {
        if let Some((_, name)) = get(block, "targetname")
            .and_then(|own| flag_visuals.iter().find(|(target, _)| target == own))
        {
            let name = name.clone();
            set(block, "targetname", name);
        }
    }

    blocks.extend(spawned);
    if !blocks
        .iter()
        .any(|block| get(block, "targetname") == Some("care_package"))
    {
        blocks.push(vec![
            ("classname".into(), "script_model".into()),
            ("targetname".into(), "care_package".into()),
            ("target".into(), "care_package_collision".into()),
        ]);
        blocks.push(vec![
            ("classname".into(), "script_brushmodel".into()),
            ("targetname".into(), "care_package_collision".into()),
        ]);
    }

    let mut out = String::with_capacity(text.len() + text.len() / 16);
    for block in blocks {
        out.push_str("{\n");
        for (key, value) in block {
            out.push_str(&format!("\"{key}\" \"{value}\"\n"));
        }
        out.push_str("}\n");
    }
    out
}
