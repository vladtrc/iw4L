use crate::{CommandSpec, ConsoleCommand, ConsoleRegistry};
use bevy::prelude::*;
use frame::UiMenuDvars;
use ui::frontend::maps::map_label;

const OBJECTIVE_ROWS: usize = 8;

const ROWS: usize = 20;

const MISSIONS: &[(&str, &str)] = &[
    ("trainer", "S.S.D.D."),
    ("roadkill", "Team Player"),
    ("cliffhanger", "Cliffhanger"),
    ("airport", "No Russian"),
    ("favela", "Takedown"),
    ("invasion", "Wolverines!"),
    ("favela_escape", "The Hornet's Nest"),
    ("arcadia", "Exodus"),
    ("oilrig", "The Only Easy Day... Was Yesterday"),
    ("gulag", "The Gulag"),
    ("dcburning", "Of Their Own Accord"),
    ("contingency", "Contingency"),
    ("dcemp", "Second Sun"),
    ("dc_whitehouse", "Whiskey Hotel"),
    ("estate", "Loose Ends"),
    ("boneyard", "The Enemy of My Enemy"),
    ("af_caves", "Just Like Old Times"),
    ("af_chase", "Endgame"),
    ("ending", "Museum"),
];

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    for name in [
        "ui_campaign",
        "ui_campaign_start",
        "ui_campaign_resume",
        "ui_campaign_new",
        "ui_campaign_skill",
        "ui_campaign_objectives",
    ] {
        registry.register(CommandSpec::new(name));
    }
}

fn mission_name(level: &str) -> String {
    MISSIONS
        .iter()
        .find(|(stem, _)| *stem == level)
        .map_or_else(|| map_label(level), |(_, name)| name.to_uppercase())
}

fn save_path(identity: &frame::LaunchIdentity) -> Option<std::path::PathBuf> {
    crate::user_settings::settings_path(&identity.artifacts)
        .map(|path| path.with_file_name("campaign.txt"))
}

fn saved_level(identity: &frame::LaunchIdentity) -> Option<String> {
    let text = std::fs::read_to_string(save_path(identity)?).ok()?;
    let mut lines = text.lines();
    let level = lines.next()?.trim().to_owned();
    if let Some(skill) = lines.next().and_then(|line| line.trim().parse().ok()) {
        sim::script::set_game_skill(skill);
    }
    Some(level).filter(|level| !level.is_empty())
}

fn save_level(identity: &frame::LaunchIdentity, level: &str) {
    let Some(path) = save_path(identity) else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = format!("{level}\n{}\n", sim::script::game_skill());
    if let Err(error) = std::fs::write(&path, text) {
        warn!("could not write {}: {error}", path.display());
    }
}

fn campaign_levels(root: &asset_transport::GamesRoot) -> Vec<String> {
    let mut levels: Vec<String> = asset_transport::list_sp_maps(root)
        .into_iter()
        .filter(|stem| !stem.starts_with("so_"))
        .collect();
    sort_story(&mut levels);
    levels
}

fn sort_story(levels: &mut [String]) {
    levels.sort_by_key(|stem| {
        (
            MISSIONS
                .iter()
                .position(|(known, _)| known == stem)
                .unwrap_or(MISSIONS.len()),
            stem.clone(),
        )
    });
}

pub(crate) fn route(
    mut commands: MessageReader<ConsoleCommand>,
    mut called: MessageReader<frame::CampaignLevelCalled>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut transition: ResMut<session::SessionSwapRequest>,
    mut dvars: ResMut<UiMenuDvars>,
    mut levels: Local<Vec<String>>,
    mut echo: crate::feature_dispatch::ConsoleEcho,
    presented: Res<net::PresentedSnapshot>,
    strings: Option<Res<assets::PreparedLocalizedStrings>>,
) {
    for frame::CampaignLevelCalled(level) in called.read() {
        if !level.is_empty()
            && let Some(identity) = identity.as_deref()
        {
            save_level(identity, level);
        }
    }
    for command in commands.read() {
        match command.name.as_str() {
            "ui_campaign" => {
                asset_transport::set_campaign_zone_set(true);
                assets::forget_common();
                assets::clear_resident_map();
                *levels = identity
                    .as_ref()
                    .map(|identity| {
                        campaign_levels(&asset_transport::GamesRoot(identity.games_root.clone()))
                    })
                    .unwrap_or_default();
                for row in 0..ROWS {
                    dvars.set(
                        &format!("ui_sp_{row}"),
                        levels
                            .get(row)
                            .map(|level| mission_name(level))
                            .unwrap_or_default(),
                    );
                }
                let resume = identity
                    .as_deref()
                    .and_then(saved_level)
                    .filter(|level| levels.contains(level));
                dvars.set(
                    "ui_sp_current",
                    resume
                        .as_deref()
                        .map(|level| format!("\"{}\"", mission_name(level)))
                        .unwrap_or_default(),
                );
                dvars.set("ui_sp_resume", resume.unwrap_or_default());
                dvars.set(
                    "ui_sp_status",
                    if levels.is_empty() {
                        "No campaign levels installed"
                    } else {
                        ""
                    },
                );
            }
            "ui_campaign_objectives" => {
                let rows = presented
                    .snapshot()
                    .map(|snapshot| snapshot.meta.objectives.list.clone())
                    .unwrap_or_default();
                for row in 0..OBJECTIVE_ROWS {
                    let text = rows
                        .get(row)
                        .map(|row| objective_line(row, strings.as_deref()))
                        .unwrap_or_default();
                    dvars.set(&format!("ui_obj_{row}"), text);
                }
            }
            "ui_campaign_start" | "ui_campaign_resume" | "ui_campaign_new"
            | "ui_campaign_skill" => {
                let row = |arg: Option<&str>| {
                    arg.and_then(|arg| arg.parse::<usize>().ok())
                        .and_then(|row| levels.get(row))
                        .cloned()
                };
                let level = match command.name.as_str() {
                    "ui_campaign_start" => row(command.args.first().map(String::as_str)),
                    "ui_campaign_resume" => identity
                        .as_deref()
                        .and_then(saved_level)
                        .filter(|level| levels.contains(level)),
                    "ui_campaign_skill" => {
                        if let Some(skill) = command.args.first().and_then(|arg| arg.parse().ok()) {
                            sim::script::set_game_skill(skill);
                        }
                        match dvars.get("ui_sp_pending") {
                            Some("new") => levels.first().cloned(),
                            pending => row(pending),
                        }
                    }
                    _ => levels.first().cloned(),
                };
                let Some(level) = level else {
                    echo.write("menu: Level is unavailable");
                    continue;
                };
                if let Some(identity) = identity.as_deref() {
                    save_level(identity, &level);
                }
                match transition.request_zone(level.clone()) {
                    Ok(id) => echo.write(format!("menu: starting campaign {level} (swap #{id})")),
                    Err(error) => echo.write(format!("menu: {error}")),
                }
            }
            _ => {}
        }
    }
}

fn objective_line(
    row: &sim::ObjectiveListRow,
    strings: Option<&assets::PreparedLocalizedStrings>,
) -> String {
    let localize = |raw: &str| match raw.strip_prefix(sim::HUD_STRING_PLAIN) {
        Some(plain) => plain.to_owned(),
        None => strings
            .and_then(|strings| strings.0.text(raw))
            .map_or_else(|| raw.to_owned(), str::to_owned),
    };
    let mut text = localize(&row.template);
    if !row.arg.is_empty() {
        for (k, arg) in row.arg.split(sim::HUD_PRINT_ARG_SEPARATOR).enumerate() {
            text = text.replace(&format!("&&{}", k + 1), &localize(arg));
        }
    }
    match row.state {
        sim::ObjectiveState::Done => format!("{text} - Complete"),
        sim::ObjectiveState::Failed => format!("{text} - Failed"),
        _ => text,
    }
}
