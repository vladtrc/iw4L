use std::path::Path;

use bevy::prelude::Resource;

use crate::CapturedStringTable;

pub const FACTION_ICON_COL: i32 = 5;

const FACTION_VOICE_PREFIX_COL: i32 = 7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArenaCharsets {
    pub map: String,
    pub allieschar: Option<String>,
    pub axischar: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapTeamSettings {
    pub allies_color: Option<[f32; 3]>,
    pub axis_color: Option<[f32; 3]>,
    pub allies_name: Option<asset_core::AssetKey>,
    pub axis_name: Option<asset_core::AssetKey>,
    pub attackers: Option<String>,
    pub defenders: Option<String>,
    pub allies: Option<asset_core::AssetKey>,
    pub axis: Option<asset_core::AssetKey>,
    pub allies_charset: Option<String>,
    pub axis_charset: Option<String>,
    pub allies_strings: FactionStrings,
    pub axis_strings: FactionStrings,
    pub allies_voice: Option<String>,
    pub axis_voice: Option<String>,
    pub allies_music: TeamMusic,
    pub axis_music: TeamMusic,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamMusic {
    pub spawn: Option<String>,
    pub victory: Option<String>,
    pub defeat: Option<String>,
    pub winning: Option<String>,
    pub losing: Option<String>,
}

impl TeamMusic {
    fn prefixed(prefix: &str) -> Self {
        let music = |line: &str| Some(format!("{prefix}{line}_music"));
        Self {
            spawn: music("spawn"),
            victory: music("victory"),
            defeat: music("defeat"),
            winning: music("winning"),
            losing: music("losing"),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FactionStrings {
    pub name: Option<String>,
    pub eliminated: Option<String>,
    pub forfeited: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ObjectiveVisuals {
    pub neutral_flag: Option<String>,
    pub bomb: Option<String>,
    pub bomb_explosion_fx: Option<String>,
    pub plant_weapon: Option<String>,
    pub defuse_weapon: Option<String>,
    pub flag: [Option<String>; 2],
    pub flag_carry: [Option<String>; 2],
    pub flag_base_fx: [Option<String>; 2],
    pub crate_model: [Option<String>; 2],
    pub crate_overlay: [Option<String>; 2],
}

impl ObjectiveVisuals {
    #[must_use]
    pub fn from_faction_table(
        table: &CapturedStringTable,
        allieschar: Option<&str>,
        axischar: Option<&str>,
    ) -> Self {
        let cell = |charset: Option<&str>, column| {
            let charset = charset.filter(|s| !s.is_empty())?;
            let value = table.lookup_col(charset, column);
            (!value.is_empty()).then(|| value.to_owned())
        };
        let pair = |column| [cell(allieschar, column), cell(axischar, column)];
        Self {
            neutral_flag: Some("prop_flag_neutral".to_owned()),
            bomb: Some("prop_suitcase_bomb".to_owned()),
            plant_weapon: Some("briefcase_bomb_mp".to_owned()),
            defuse_weapon: Some("briefcase_bomb_defuse_mp".to_owned()),
            bomb_explosion_fx: Some("explosions/tanker_explosion".to_owned()),
            flag: pair(10),
            flag_carry: pair(11),
            flag_base_fx: pair(13),
            crate_model: pair(18),
            crate_overlay: [
                Some("com_plasticcase_friendly".to_owned()),
                Some("com_plasticcase_enemy".to_owned()),
            ],
        }
    }

    fn models(&self) -> impl Iterator<Item = &Option<String>> {
        [&self.neutral_flag, &self.bomb]
            .into_iter()
            .chain(&self.flag)
            .chain(&self.flag_carry)
            .chain(&self.crate_model)
            .chain(&self.crate_overlay)
    }

    pub fn model_names(&self) -> impl Iterator<Item = &str> {
        self.models().filter_map(|name| name.as_deref())
    }

    pub fn is_complete(&self) -> bool {
        self.models()
            .chain(&self.flag_base_fx)
            .chain([
                &self.bomb_explosion_fx,
                &self.plant_weapon,
                &self.defuse_weapon,
            ])
            .all(|name| name.as_deref().is_some_and(|s| !s.is_empty()))
    }

    pub fn model_pairs<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.models()
            .zip(other.models())
            .filter_map(|(a, b)| Some((a.as_deref()?, b.as_deref()?)))
    }

    pub fn weapon_pairs<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = (&'a str, &'a str)> {
        [&self.plant_weapon, &self.defuse_weapon]
            .into_iter()
            .zip([&other.plant_weapon, &other.defuse_weapon])
            .filter_map(|(a, b)| Some((a.as_deref()?, b.as_deref()?)))
    }

    pub fn fx_pairs<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = (&'a str, &'a str)> {
        self.flag_base_fx
            .iter()
            .zip(&other.flag_base_fx)
            .chain(std::iter::once((
                &self.bomb_explosion_fx,
                &other.bomb_explosion_fx,
            )))
            .filter_map(|(a, b)| Some((a.as_deref()?, b.as_deref()?)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Resource)]
pub struct SessionTeamSettings(pub MapTeamSettings);

fn table_icon(
    table: &CapturedStringTable,
    namespace: asset_core::FamilyId,
    charset: Option<&str>,
) -> Option<asset_core::AssetKey> {
    let charset = charset.filter(|s| !s.is_empty())?;
    let cell = table.lookup_col(charset, FACTION_ICON_COL);
    asset_core::AssetKey::new(namespace, asset_core::AssetKind::Material, cell).ok()
}

#[must_use]
pub fn team_settings(
    table: &CapturedStringTable,
    namespace: asset_core::FamilyId,
    allieschar: Option<&str>,
    axischar: Option<&str>,
) -> MapTeamSettings {
    let color = |charset: &str| {
        let rgb = [14, 15, 16].map(|column| table.lookup_col(charset, column).parse::<f32>().ok());
        let rgb = [rgb[0]?, rgb[1]?, rgb[2]?];
        rgb.iter().all(|v| v.is_finite()).then_some(rgb)
    };
    let prefix = |charset: &str| {
        let prefix = table.lookup_col(charset, FACTION_VOICE_PREFIX_COL);
        (!prefix.is_empty()).then_some(prefix)
    };
    let voice = |charset: &str| prefix(charset).map(|prefix| format!("{prefix}1mc_"));
    let strings = |charset: &str| {
        let key = |column| {
            let key = table.lookup_col(charset, column);
            (!key.is_empty()).then(|| key.to_owned())
        };
        FactionStrings {
            name: key(1),
            eliminated: key(3),
            forfeited: key(4),
        }
    };
    let music = |charset: Option<&str>| {
        charset
            .and_then(prefix)
            .map_or_else(TeamMusic::default, TeamMusic::prefixed)
    };
    MapTeamSettings {
        allies_color: allieschar.and_then(color),
        axis_color: axischar.and_then(color),
        allies: table_icon(table, namespace, allieschar),
        axis: table_icon(table, namespace, axischar),
        allies_name: allieschar.and_then(|name| {
            asset_core::AssetKey::new(
                namespace,
                asset_core::AssetKind::Localize,
                table.lookup_col(name, 2),
            )
            .ok()
        }),
        axis_name: axischar.and_then(|name| {
            asset_core::AssetKey::new(
                namespace,
                asset_core::AssetKind::Localize,
                table.lookup_col(name, 2),
            )
            .ok()
        }),
        allies_charset: allieschar.map(str::to_owned),
        axis_charset: axischar.map(str::to_owned),
        allies_voice: allieschar.and_then(voice),
        axis_voice: axischar.and_then(voice),
        allies_music: music(allieschar),
        axis_music: music(axischar),
        allies_strings: allieschar.map_or_else(FactionStrings::default, strings),
        axis_strings: axischar.map_or_else(FactionStrings::default, strings),
        ..Default::default()
    }
}

#[must_use]
pub fn team_settings_for_zone(
    table: &CapturedStringTable,
    namespace: asset_core::FamilyId,
    arena_text: Option<&str>,
    zone: &str,
) -> MapTeamSettings {
    if zone.is_empty() {
        return MapTeamSettings::default();
    }
    let Some(row) = arena_text.and_then(|text| arena_charsets(text, zone)) else {
        return MapTeamSettings::default();
    };
    team_settings(
        table,
        namespace,
        row.allieschar.as_deref(),
        row.axischar.as_deref(),
    )
}

#[must_use]
pub fn arena_charsets(text: &str, map: &str) -> Option<ArenaCharsets> {
    parse_arena(text)
        .into_iter()
        .find(|row| row.map.eq_ignore_ascii_case(map))
}

#[must_use]
pub fn arena_entry(text: &str, map: &str) -> Option<std::collections::BTreeMap<String, String>> {
    for block in text.split('{').skip(1) {
        let body = block.split('}').next().unwrap_or(block);
        let mut entry = std::collections::BTreeMap::new();
        for raw in body.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(key) = parts.next() else {
                continue;
            };
            let value = unquote(parts.collect::<Vec<_>>().join(" ").as_str());
            entry.insert(key.to_ascii_lowercase(), value);
        }
        if entry
            .get("map")
            .is_some_and(|name| name.eq_ignore_ascii_case(map))
        {
            return Some(entry);
        }
    }
    None
}

#[must_use]
pub fn parse_arena(text: &str) -> Vec<ArenaCharsets> {
    let mut rows = Vec::new();
    for block in text.split('{').skip(1) {
        let body = block.split('}').next().unwrap_or(block);
        let mut map = None;
        let mut allieschar = None;
        let mut axischar = None;
        for raw in body.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(key) = parts.next() else {
                continue;
            };
            let value = unquote(parts.collect::<Vec<_>>().join(" ").as_str());
            if value.is_empty() {
                continue;
            }
            match key {
                "map" => map = Some(value),
                "allieschar" => allieschar = Some(value),
                "axischar" => axischar = Some(value),
                _ => {}
            }
        }
        if let Some(map) = map {
            rows.push(ArenaCharsets {
                map,
                allieschar,
                axischar,
            });
        }
    }
    rows
}

fn unquote(value: &str) -> String {
    value.trim().trim_matches('"').to_owned()
}

pub fn read_basemaps_arena(games_root: &Path) -> Option<String> {
    read_iwd_named(games_root, "mp/basemaps.arena").and_then(|bytes| String::from_utf8(bytes).ok())
}

pub fn read_iwd_named(games_root: &Path, want: &str) -> Option<Vec<u8>> {
    asset_transport::read_iwd_named(games_root, want)
}

pub type TeamSourcesLoader = fn(&Path) -> (Option<String>, Option<CapturedStringTable>);

#[derive(Clone, Copy)]
pub struct TeamFamily {
    /// Bind this family’s objective visuals into the reference game mode.
    pub bind_objectives: bool,
    /// Missing native flag-base effects intentionally suppress the host effect.
    pub allow_absent_flag_base_fx: bool,
    pub icon_alias_suffixes: &'static [&'static str],
    /// Native projection of reference announcer lines; None retains reference routes.
    pub announcer: Option<fn(&str, &str) -> String>,
    /// None when normalized faction-table capture already supplies the visuals.
    pub objectives: Option<fn(&MapTeamSettings) -> ObjectiveVisuals>,
    /// A separate source walk beyond the family's normal asset walk, when needed.
    pub sources: Option<TeamSourcesLoader>,
}

pub const REFERENCE_TEAM_FAMILY: TeamFamily = TeamFamily {
    bind_objectives: false,
    allow_absent_flag_base_fx: false,
    icon_alias_suffixes: &[],
    announcer: None,
    objectives: None,
    sources: None,
};

#[derive(Default)]
pub struct TeamFamilies(std::collections::BTreeMap<asset_core::FamilyId, TeamFamily>);
impl TeamFamilies {
    pub fn register(&mut self, family: asset_core::FamilyId, policy: TeamFamily) {
        assert!(
            self.0.insert(family, policy).is_none(),
            "duplicate team family"
        );
    }
}
static TEAM_FAMILIES: std::sync::OnceLock<TeamFamilies> = std::sync::OnceLock::new();
pub fn install_team_families(families: TeamFamilies) -> Result<(), &'static str> {
    TEAM_FAMILIES
        .set(families)
        .map_err(|_| "team families already installed")
}
pub fn team_family(family: asset_core::FamilyId) -> Option<&'static TeamFamily> {
    TEAM_FAMILIES.get()?.0.get(&family)
}

pub fn team_icon_alias(
    namespace: asset_core::FamilyId,
    material: &str,
    icon: &asset_core::AssetKey,
) -> bool {
    namespace == icon.namespace
        && team_family(namespace).is_some_and(|policy| {
            policy
                .icon_alias_suffixes
                .iter()
                .any(|suffix| material.strip_suffix(suffix) == Some(icon.name.as_str()))
        })
}
