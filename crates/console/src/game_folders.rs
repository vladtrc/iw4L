use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use asset_core::{FamilyId, game_folder_families, game_folder_family};
use bevy::prelude::*;

fn settings_key(game: FamilyId) -> String {
    format!(
        "game_folder_{}",
        game_folder_family(game)
            .expect("registered folder family")
            .key
    )
}

pub(crate) fn parse_game_folder(
    key: &str,
    value: &str,
    settings: &mut frame::GameSettings,
) -> bool {
    let Some(game) = game_folder_families()
        .into_iter()
        .map(|(id, _)| id)
        .find(|&game| settings_key(game) == key)
    else {
        return false;
    };
    settings.set_game_folder(game, value.trim().to_owned());
    true
}

pub(crate) fn serialize_game_folders(settings: &frame::GameSettings) -> Vec<String> {
    game_folder_families()
        .into_iter()
        .map(|(id, _)| id)
        .map(|game| {
            let folder = settings.game_folder(game).replace(['\n', '\r'], "");
            format!("{}={folder}", settings_key(game))
        })
        .collect()
}

pub fn stored_game_folders(artifacts: &Path) -> Vec<PathBuf> {
    let Some(path) = crate::user_settings::settings_path(artifacts) else {
        return Vec::new();
    };
    let Ok(source) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let mut settings = frame::GameSettings::default();
    for line in source.lines() {
        if let Some((key, value)) = line.trim().split_once('=') {
            parse_game_folder(key, value, &mut settings);
        }
    }
    game_folder_families()
        .into_iter()
        .map(|(id, _)| id)
        .map(|game| settings.game_folder(game))
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[derive(Resource)]
pub(crate) struct FolderPicks {
    sender: mpsc::Sender<(FamilyId, PathBuf)>,
    receiver: Mutex<mpsc::Receiver<(FamilyId, PathBuf)>>,
    dialog_open: Arc<AtomicBool>,
}

impl Default for FolderPicks {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            sender,
            receiver: Mutex::new(receiver),
            dialog_open: Arc::default(),
        }
    }
}

impl FolderPicks {
    fn open_dialog(&self, game: FamilyId, start: Option<PathBuf>) {
        if self.dialog_open.swap(true, Ordering::AcqRel) {
            return;
        }
        let sender = self.sender.clone();
        let dialog_open = self.dialog_open.clone();
        let spawned = std::thread::Builder::new()
            .name("game folder dialog".into())
            .spawn(move || {
                let mut dialog = rfd::FileDialog::new().set_title(format!(
                    "Choose the {} folder",
                    game_folder_family(game)
                        .expect("registered folder family")
                        .title
                ));
                if let Some(start) = start.filter(|start| start.is_dir()) {
                    dialog = dialog.set_directory(start);
                }
                if let Some(folder) = dialog.pick_folder() {
                    let _ = sender.send((game, folder));
                }
                dialog_open.store(false, Ordering::Release);
            });
        if let Err(error) = spawned {
            self.dialog_open.store(false, Ordering::Release);
            warn!("could not open the folder dialog: {error}");
        }
    }
}

fn tail(text: &str, chars: usize) -> String {
    let count = text.chars().count();
    if count <= chars {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(count - (chars - 3)).collect();
    format!("...{kept}")
}

fn folder_line(games_root: Option<&Path>, game: FamilyId, folder: &str) -> String {
    const WIDTH: usize = 40;
    if !folder.is_empty() {
        let path = Path::new(folder);
        return if asset_transport::folder_holds_game(path, game) {
            tail(folder, WIDTH)
        } else {
            format!("^1Not this game:^7 {}", tail(folder, WIDTH - 15))
        };
    }
    match games_root.and_then(|root| asset_transport::find_game_install(root, game)) {
        Some(found) => format!("Found: {}", tail(&found.display().to_string(), WIDTH - 7)),
        None => "^3Not found^7 - choose its folder".to_owned(),
    }
}

pub(crate) fn game_folder_menu(
    mut events: MessageReader<crate::ConsoleCommand>,
    picks: Res<FolderPicks>,
    mut settings: ResMut<frame::GameSettings>,
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut at_launch: Local<Option<std::collections::BTreeMap<FamilyId, String>>>,
    mut shown: Local<Option<std::collections::BTreeMap<FamilyId, String>>>,
) {
    let launched = at_launch.get_or_insert_with(|| settings.game_folders.clone());
    for command in events.read() {
        if !matches!(command.name.as_str(), "set" | "seta") {
            continue;
        }
        let [name, value, ..] = command.args.as_slice() else {
            continue;
        };
        match name.as_str() {
            "ui_pick_game_folder" => {
                if let Some(game) = game_folder_families()
                    .into_iter()
                    .find_map(|(id, policy)| (policy.key == value).then_some(id))
                {
                    let current = settings.game_folder(game);
                    let start = (!current.is_empty()).then(|| PathBuf::from(current));
                    picks.open_dialog(game, start);
                }
            }
            "ui_reset_game_folders" if value == "1" => {
                for (game, _) in game_folder_families() {
                    settings.set_game_folder(game, String::new());
                }
                settings.touch();
            }
            _ => {}
        }
    }
    let received: Vec<_> = match picks.receiver.lock() {
        Ok(receiver) => receiver.try_iter().collect(),
        Err(poisoned) => poisoned.into_inner().try_iter().collect(),
    };
    for (game, folder) in received {
        let root = asset_transport::game_install_root(&folder);
        settings.set_game_folder(game, root.display().to_string());
        settings.touch();
    }

    if shown.as_ref() == Some(&settings.game_folders) {
        return;
    }
    let games_root = asset_transport::games_root().ok();
    for (game, _) in game_folder_families() {
        let line = folder_line(
            games_root.as_ref().map(|root| root.0.as_path()),
            game,
            settings.game_folder(game),
        );
        dvars.set(
            &format!(
                "ui_game_folder_{}",
                game_folder_family(game)
                    .expect("registered folder family")
                    .key
            ),
            line,
        );
    }
    let hint = if settings.game_folders == *launched {
        "Folders left empty are looked for next to the MW2 folder."
    } else {
        "^3Restart the game to load content from the new folders."
    };
    dvars.set("ui_game_folders_hint", hint);
    *shown = Some(settings.game_folders.clone());
}
