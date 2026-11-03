use std::collections::HashMap;

use asset_core::FamilyId;
use bevy::prelude::Resource;

pub(crate) fn voice_prefixes_for_zone(
    catalog: Option<&asset_game::MenuCatalog>,
    bank: Option<&crate::SoundBank>,
    identity: &frame::LaunchIdentity,
) -> (Option<String>, Option<String>) {
    let Some(catalog) = catalog else {
        return (None, None);
    };
    let Some(table) = catalog.string_table(gamemode_iw4::FACTION_TABLE) else {
        return (None, None);
    };
    let (allies_cs, axis_cs) = arena_faction_charsets(catalog, bank, identity);
    let allies = table.lookup_col(&allies_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    let axis = table.lookup_col(&axis_cs, gamemode_iw4::FACTION_VOICE_PREFIX_COL);
    (
        (!allies.is_empty()).then(|| allies.to_owned()),
        (!axis.is_empty()).then(|| axis.to_owned()),
    )
}

fn arena_faction_charsets(
    catalog: &asset_game::MenuCatalog,
    bank: Option<&crate::SoundBank>,
    identity: &frame::LaunchIdentity,
) -> (String, String) {
    let from_ui = catalog.rawfile_text("mp/basemaps.arena").map(str::to_owned);
    let from_bank = bank.and_then(|b| b.0.rawfile_text("mp/basemaps.arena").map(str::to_owned));
    let arena_text = from_ui
        .or(from_bank)
        .or_else(|| asset_game::read_basemaps_arena(&identity.games_root));
    let row = arena_text
        .as_deref()
        .and_then(|text| asset_game::arena_charsets(text, &identity.zone));
    let (allies, axis) = row.map_or((None, None), |row| (row.allieschar, row.axischar));
    (
        allies
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| gamemode_iw4::DEFAULT_ALLIES_CHARSET.to_owned()),
        axis.filter(|s| !s.is_empty())
            .unwrap_or_else(|| gamemode_iw4::DEFAULT_AXIS_CHARSET.to_owned()),
    )
}

pub(crate) fn team_voice_aliases(
    bank: &asset_audio::SoundCatalog,
    allies: Option<&str>,
    axis: Option<&str>,
) -> Vec<String> {
    let prefixes: Vec<&str> = [allies, axis]
        .into_iter()
        .flatten()
        .chain(["generic_death_"])
        .collect();
    let mut aliases: Vec<String> = (0..bank.sounds().len())
        .filter(|&index| bank.namespace_of_alias(index) == Some(FamilyId::IW4))
        .filter_map(|index| bank.name_at(index))
        .filter(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .map(str::to_owned)
        .collect();
    for prefix in [allies, axis].into_iter().flatten() {
        for line in TEAM_MUSIC {
            let alias = format!("{prefix}{line}");
            if !aliases.contains(&alias) {
                aliases.push(alias);
            }
        }
    }
    aliases
}

const TEAM_MUSIC: [&str; 5] = [
    "spawn_music",
    "victory_music",
    "defeat_music",
    "winning_music",
    "losing_music",
];

const IW4_LINE: &str = "1mc_";

#[derive(Resource, Default)]
pub(crate) struct AnnouncerRoutes(HashMap<String, (FamilyId, String)>);

impl AnnouncerRoutes {
    pub(crate) fn route<'a>(&'a self, alias: &'a str) -> (FamilyId, &'a str) {
        self.0
            .get(alias)
            .map_or((FamilyId::IW4, alias), |(ns, routed)| (*ns, routed))
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn build(
        bank: &asset_audio::SoundCatalog,
        namespace: FamilyId,
        iw4: [Option<&str>; 2],
        native: Option<&asset_game::MapTeamSettings>,
        aliases: &[String],
    ) -> Self {
        let Some(route_line) =
            asset_game::team_family(namespace).and_then(|policy| policy.announcer)
        else {
            return Self::default();
        };
        let Some(native) = native else {
            return Self::default();
        };
        let teams: Vec<(&str, Option<&str>, &asset_game::TeamMusic)> = iw4
            .into_iter()
            .zip([
                (native.allies_voice.as_deref(), &native.allies_music),
                (native.axis_voice.as_deref(), &native.axis_music),
            ])
            .filter_map(|(iw4, (voice, music))| Some((iw4?, voice, music)))
            .collect();
        let routes = aliases
            .iter()
            .filter_map(|alias| {
                teams.iter().find_map(|&(iw4, voice, music)| {
                    let line = alias.strip_prefix(iw4)?;
                    let routed = match line.strip_prefix(IW4_LINE) {
                        Some(line) => route_line(voice?, line),
                        None => match line {
                            "spawn_music" => music.spawn.clone(),
                            "victory_music" => music.victory.clone(),
                            "defeat_music" => music.defeat.clone(),
                            "winning_music" => music.winning.clone(),
                            "losing_music" => music.losing.clone(),
                            _ => None,
                        }?,
                    };
                    bank.index_in(namespace, &routed)?;
                    Some((alias.clone(), (namespace, routed)))
                })
            })
            .collect();
        Self(routes)
    }
}
