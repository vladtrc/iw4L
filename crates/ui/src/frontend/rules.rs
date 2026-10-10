use frame::UiMenuDvars;

use crate::MenuMapList;

pub fn selected_game(
    dvars: &UiMenuDvars,
    maps: &MenuMapList,
) -> Result<(String, sim::HostGameModeSelection), String> {
    let map = dvars.get("ui_mapname").ok_or("No map selected")?;
    if !maps.contains(map) {
        return Err(format!("Map `{map}` is not installed"));
    }
    let mode = dvars
        .get("ui_gametype")
        .and_then(sim::HostGameModeSelection::from_token)
        .ok_or("Unsupported game mode")?;
    let zombies_map = map.starts_with("t6:zm_");
    if (mode.token() == "zclassic") != zombies_map {
        return Err(
            "Select Zombies for a Zombies map, or Multiplayer for a multiplayer map".into(),
        );
    }
    Ok((map.to_owned(), mode))
}

pub const MATCH_CONFIG: &str = "default_xboxlive.cfg";

fn is_rule(name: &str) -> bool {
    name.starts_with("scr_")
        || name.starts_with("koth_")
        || matches!(name, "g_hardcore" | "camera_thirdperson")
}

pub fn seed_rules(dvars: &mut UiMenuDvars, config: &str) {
    for line in config.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        let Some(rest) = line.strip_prefix("set ") else {
            continue;
        };
        let mut words = rest.split_whitespace();
        let (Some(name), value) = (words.next(), words.next().unwrap_or("")) else {
            continue;
        };
        let name = name.to_ascii_lowercase();
        if is_rule(&name) && dvars.get(&name).is_none() {
            dvars.set(&name, value.trim_matches('"'));
        }
    }
    for dvar in [
        sim::CONSTANT_RADAR_DVAR,
        sim::ENEMY_BOTS_DVAR,
        sim::FRIENDLY_BOTS_DVAR,
    ] {
        if dvars.get(dvar).is_none() {
            dvars.set(dvar, "0");
        }
    }
}

pub fn host_rules(dvars: &UiMenuDvars) -> frame::HostMatchRules {
    frame::HostMatchRules(
        dvars
            .iter()
            .filter(|(name, _)| is_rule(name))
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
            .collect(),
    )
}
