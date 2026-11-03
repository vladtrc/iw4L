use asset_game::*;
pub const POLICY: TeamFamily = TeamFamily {
    bind_objectives: true,
    allow_absent_flag_base_fx: false,
    icon_alias_suffixes: &["_fade"],
    announcer: Some(super::announcer::route),
    objectives: Some(|settings| {
        objective_visuals(
            settings.allies_charset.as_deref(),
            settings.axis_charset.as_deref(),
        )
    }),
    sources: None,
};
fn team_music(script: &str, team: &str) -> TeamMusic {
    let spawn =
        asset_audio::game_nested_string_assignment(script, &["music", &format!("spawn_{team}")]);
    TeamMusic {
        spawn: spawn.map(|state| format!("mus_{}", state.to_ascii_lowercase())),
        victory: Some("mus_victory".to_owned()),
        defeat: Some("mus_loss".to_owned()),
        winning: Some("mus_time_running_out".to_owned()),
        losing: Some("mus_time_running_out".to_owned()),
    }
}

pub fn objective_visuals(allies: Option<&str>, axis: Option<&str>) -> ObjectiveVisuals {
    let allies_flag = match allies {
        Some("marines") => "mp_flag_allies_1",
        Some("rebels") => "mp_flag_allies_3",
        _ => "mp_flag_allies_2",
    };
    let axis_flag = match axis {
        Some("russian") => "mp_flag_axis_1",
        Some("tropas") => "mp_flag_axis_3",
        _ => "mp_flag_axis_2",
    };
    let crates = [
        Some("mp_supplydrop_ally".to_owned()),
        Some("mp_supplydrop_axis".to_owned()),
    ];
    ObjectiveVisuals {
        neutral_flag: Some("mp_flag_neutral".to_owned()),
        bomb: Some("prop_suitcase_bomb".to_owned()),
        plant_weapon: Some("briefcase_bomb_mp".to_owned()),
        defuse_weapon: Some("briefcase_bomb_defuse_mp".to_owned()),
        bomb_explosion_fx: Some("maps/mp_maps/fx_mp_exp_bomb".to_owned()),
        flag: [Some(allies_flag.to_owned()), Some(axis_flag.to_owned())],
        flag_carry: [
            Some(format!("{allies_flag}_carry")),
            Some(format!("{axis_flag}_carry")),
        ],
        flag_base_fx: std::array::from_fn(|_| Some("misc/fx_ui_flagbase_gold_t5".to_owned())),
        crate_model: crates.clone(),
        crate_overlay: crates,
    }
}

#[must_use]
pub fn t5_teamset_key_from_rawfile(name: &str) -> Option<&str> {
    let file = name.rsplit(['/', '\\']).next()?;
    let stem = file.strip_prefix("_teamset_")?.strip_suffix(".gsc")?;
    (!stem.is_empty()).then_some(stem)
}

#[must_use]
pub fn t5_teamset_from_map_gsc(script: &str) -> Option<&str> {
    let lower = script.to_ascii_lowercase();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("_teamset_") {
        let start = from + rel + "_teamset_".len();
        let rest = script.get(start..)?;
        let Some(end) = rest.find("::") else {
            from = start;
            continue;
        };
        let name = rest[..end].trim();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            from = start + end + 2;
            continue;
        }
        if rest[end..].to_ascii_lowercase().starts_with("::level_init") {
            return Some(name);
        }
        from = start + end + 2;
    }
    None
}

fn script_dvar_string<'a>(script: &'a str, key: &str) -> Option<&'a str> {
    for line in script.lines() {
        let line = line.trim();
        let Some((function, args)) = line.split_once('(') else {
            continue;
        };
        if !function.trim().eq_ignore_ascii_case("setdvar") {
            continue;
        }
        let mut quotes = args.split('"');
        let (Some(before), Some(name), Some(between), Some(value)) =
            (quotes.next(), quotes.next(), quotes.next(), quotes.next())
        else {
            continue;
        };
        if !before.trim().is_empty() || !name.eq_ignore_ascii_case(key) {
            continue;
        }
        if !matches!(between.trim(), "," | ",&" | ", &") {
            continue;
        }
        return Some(value);
    }
    None
}

#[must_use]
pub fn t5_settings_from_teamset_gsc(script: &str) -> MapTeamSettings {
    let color = |key| {
        let mut values = script_dvar_string(script, key)?
            .split_whitespace()
            .map(str::parse::<f32>);
        let rgb = [
            values.next()?.ok()?,
            values.next()?.ok()?,
            values.next()?.ok()?,
        ];
        (values.next().is_none() && rgb.iter().all(|v| v.is_finite())).then_some(rgb)
    };
    MapTeamSettings {
        allies_color: color("g_TeamColor_Allies"),
        axis_color: color("g_TeamColor_Axis"),
        allies_name: script_dvar_string(script, "g_TeamName_Allies").and_then(|name| {
            asset_core::AssetKey::new(crate::ID, asset_core::AssetKind::Localize, name).ok()
        }),
        axis_name: script_dvar_string(script, "g_TeamName_Axis").and_then(|name| {
            asset_core::AssetKey::new(crate::ID, asset_core::AssetKind::Localize, name).ok()
        }),
        attackers: asset_audio::game_string_assignment(script, "attackers").map(str::to_owned),
        defenders: asset_audio::game_string_assignment(script, "defenders").map(str::to_owned),
        allies: asset_audio::game_nested_string_assignment(script, &["icons", "allies"]).and_then(
            |name| asset_core::AssetKey::new(crate::ID, asset_core::AssetKind::Material, name).ok(),
        ),
        axis: asset_audio::game_nested_string_assignment(script, &["icons", "axis"]).and_then(
            |name| asset_core::AssetKey::new(crate::ID, asset_core::AssetKind::Material, name).ok(),
        ),
        allies_charset: asset_audio::game_string_assignment(script, "allies").map(str::to_owned),
        axis_charset: asset_audio::game_string_assignment(script, "axis").map(str::to_owned),
        allies_strings: teamset_strings(script, "allies"),
        axis_strings: teamset_strings(script, "axis"),
        allies_voice: asset_audio::game_nested_string_assignment(script, &["voice", "allies"])
            .map(str::to_owned),
        axis_voice: asset_audio::game_nested_string_assignment(script, &["voice", "axis"])
            .map(str::to_owned),
        allies_music: team_music(script, "allies"),
        axis_music: team_music(script, "axis"),
    }
}

fn teamset_strings(script: &str, team: &str) -> FactionStrings {
    let key = |suffix| {
        asset_audio::game_nested_string_assignment(
            script,
            &["strings", &format!("{team}_{suffix}")],
        )
        .map(str::to_owned)
    };
    FactionStrings {
        name: key("name"),
        eliminated: key("eliminated"),
        forfeited: key("forfeited"),
    }
}

#[must_use]
pub fn t5_teamset_from_rawfile(name: &str, data: &[u8], zlib_compressed: bool) -> Option<String> {
    if !asset_audio::is_map_main_script(name) {
        return None;
    }
    let text = asset_world::decode_rawfile_text(data, zlib_compressed)?;
    t5_teamset_from_map_gsc(&text).map(str::to_owned)
}

#[must_use]
pub fn t5_settings_from_teamset_rawfile(
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Option<(String, MapTeamSettings)> {
    let key = t5_teamset_key_from_rawfile(name)?.to_owned();
    let text = asset_world::decode_rawfile_text(data, zlib_compressed)?;
    let icons = t5_settings_from_teamset_gsc(&text);
    (icons.allies.is_some() || icons.axis.is_some()).then_some((key, icons))
}
