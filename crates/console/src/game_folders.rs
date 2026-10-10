use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use bevy::prelude::*;
use frame::OtherGame;

fn settings_key(game: OtherGame) -> String {
    format!("game_folder_{}", game.key())
}

pub(crate) fn parse_game_folder(
    key: &str,
    value: &str,
    settings: &mut frame::GameSettings,
) -> bool {
    let Some(game) = OtherGame::ALL
        .into_iter()
        .find(|&game| settings_key(game) == key)
    else {
        return false;
    };
    settings.set_game_folder(game, value.trim().to_owned());
    true
}

pub(crate) fn serialize_game_folders(settings: &frame::GameSettings) -> Vec<String> {
    OtherGame::ALL
        .into_iter()
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
    OtherGame::ALL
        .into_iter()
        .map(|game| settings.game_folder(game))
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn zone_game(game: OtherGame) -> Option<asset_transport::ZoneGame> {
    Some(match game {
        OtherGame::BlackOps => asset_transport::ZoneGame::T5,
        OtherGame::BlackOps2 => asset_transport::ZoneGame::T6,
        OtherGame::ModernWarfare3 => asset_transport::ZoneGame::Iw5,
        OtherGame::ModernWarfare2 => asset_transport::ZoneGame::Iw4,
        OtherGame::ModernWarfare => return None,
    })
}

fn holds(folder: &Path, game: OtherGame) -> bool {
    zone_game(game).map_or_else(
        || asset_transport::folder_holds_modern_warfare(folder),
        |kind| asset_transport::folder_holds_game(folder, kind),
    )
}

/// The installation of each game found under `root`, breadth first, a few
/// levels deep: a Steam library, a games folder, or the installation itself.
fn scan(root: &Path) -> Vec<(OtherGame, PathBuf)> {
    const DEPTH: usize = 4;
    const MAX_FOLDERS: usize = 5000;
    const BUDGET: std::time::Duration = std::time::Duration::from_secs(5);
    let started = std::time::Instant::now();
    let mut found: Vec<(OtherGame, PathBuf)> = Vec::new();
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0)]);
    let mut visited = 0;
    while let Some((folder, depth)) = queue.pop_front() {
        visited += 1;
        if visited > MAX_FOLDERS
            || started.elapsed() > BUDGET
            || found.len() == OtherGame::ALL.len()
        {
            break;
        }
        if folder.join("zone").is_dir() {
            for game in OtherGame::ALL {
                if !found.iter().any(|(known, _)| *known == game) && holds(&folder, game) {
                    found.push((game, folder.clone()));
                }
            }
        }
        if depth == DEPTH {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| !name.starts_with('.'))
            })
            .collect();
        children.sort();
        queue.extend(children.into_iter().map(|child| (child, depth + 1)));
    }
    found.sort_by_key(|(game, _)| OtherGame::ALL.iter().position(|known| known == game));
    found
}

enum Picked {
    Game(OtherGame, PathBuf),
    Scan(PathBuf, Vec<(OtherGame, PathBuf)>),
}

#[derive(Resource)]
pub(crate) struct FolderPicks {
    sender: mpsc::Sender<Picked>,
    receiver: Mutex<mpsc::Receiver<Picked>>,
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
    /// `game` picks that game's folder; `None` picks a folder to scan for every game.
    fn open_dialog(&self, game: Option<OtherGame>, start: Option<PathBuf>) {
        if self.dialog_open.swap(true, Ordering::AcqRel) {
            return;
        }
        let sender = self.sender.clone();
        let dialog_open = self.dialog_open.clone();
        let spawned = std::thread::Builder::new()
            .name("game folder dialog".into())
            .spawn(move || {
                let title = game.map_or_else(
                    || "Choose a folder to scan for games".to_owned(),
                    |game| format!("Choose the {} folder", game.title()),
                );
                let mut dialog = rfd::FileDialog::new().set_title(title);
                if let Some(start) = start.filter(|start| start.is_dir()) {
                    dialog = dialog.set_directory(start);
                }
                if let Some(folder) = dialog.pick_folder() {
                    let picked = match game {
                        Some(game) => Picked::Game(game, folder),
                        None => {
                            let found = scan(&folder);
                            Picked::Scan(folder, found)
                        }
                    };
                    let _ = sender.send(picked);
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

fn folder_line(games_root: Option<&Path>, game: OtherGame, folder: &str) -> String {
    const WIDTH: usize = 40;
    if !folder.is_empty() {
        let path = Path::new(folder);
        return if zone_game(game).map_or_else(
            || asset_transport::folder_holds_modern_warfare(path),
            |kind| asset_transport::folder_holds_game(path, kind),
        ) {
            tail(folder, WIDTH)
        } else {
            format!("^1Not this game:^7 {}", tail(folder, WIDTH - 15))
        };
    }
    match games_root.and_then(|root| {
        zone_game(game)
            .and_then(|kind| asset_transport::find_game_install(root, kind))
            .or_else(|| {
                (game == OtherGame::ModernWarfare)
                    .then(|| asset_transport::find_modern_warfare_install(root))
                    .flatten()
            })
    }) {
        Some(found) => format!("Found: {}", tail(&found.display().to_string(), WIDTH - 7)),
        None => "^3Not found^7 - choose its folder".to_owned(),
    }
}

pub(crate) fn game_folder_menu(
    mut events: MessageReader<crate::ConsoleCommand>,
    picks: Res<FolderPicks>,
    mut settings: ResMut<frame::GameSettings>,
    mut dvars: ResMut<frame::UiMenuDvars>,
    mut at_launch: Local<Option<[String; 5]>>,
    mut shown: Local<Option<[String; 5]>>,
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
                if let Some(game) = OtherGame::from_key(value) {
                    let current = settings.game_folder(game);
                    let start = (!current.is_empty()).then(|| PathBuf::from(current));
                    picks.open_dialog(Some(game), start);
                }
            }
            "ui_scan_game_folders" if value == "1" => picks.open_dialog(None, None),
            "ui_reset_game_folders" if value == "1" => {
                for game in OtherGame::ALL {
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
    for picked in received {
        let (game, folder) = match picked {
            Picked::Game(game, folder) => (game, folder),
            Picked::Scan(root, found) => {
                let summary = if found.is_empty() {
                    format!(
                        "No games found in {}.",
                        tail(&root.display().to_string(), 48)
                    )
                } else {
                    let names: Vec<_> = found.iter().map(|(game, _)| game.title()).collect();
                    format!("Found {}.", names.join(", "))
                };
                for (game, folder) in found {
                    settings.set_game_folder(game, folder.display().to_string());
                }
                settings.touch();
                dvars.set("ui_folder_error", "");
                dvars.set("ui_folder_scan", summary);
                continue;
            }
        };
        let root = asset_transport::game_install_root(&folder);
        if !holds(&root, game) {
            dvars.set(
                "ui_folder_error",
                format!("This folder does not contain {} assets.", game.title()),
            );
            continue;
        }
        dvars.set("ui_folder_error", "");
        settings.set_game_folder(game, root.display().to_string());
        settings.touch();
    }

    if shown.as_ref() == Some(&settings.game_folders) {
        return;
    }
    let games_root = asset_transport::games_root_from_env().ok();
    for game in OtherGame::ALL {
        let line = folder_line(
            games_root.as_ref().map(|root| root.0.as_path()),
            game,
            settings.game_folder(game),
        );
        dvars.set(&format!("ui_game_folder_{}", game.key()), line);
    }
    let hint = if settings.game_folders == *launched {
        "Empty folders are detected in the configured games directory."
    } else {
        "Installation folders updated."
    };
    dvars.set("ui_game_folders_hint", hint);
    *shown = Some(settings.game_folders.clone());
}
