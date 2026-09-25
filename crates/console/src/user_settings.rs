use std::{fs, path::PathBuf};

use bevy::{
    audio::{AudioSink, AudioSinkPlayback, GlobalVolume, Volume},
    input::{ButtonInput, keyboard::KeyCode, mouse::MouseButton},
    prelude::*,
};

use crate::{BindButton, KeyBinds, display_button};

#[derive(Resource, Default)]
pub(crate) struct PendingMenuBinding {
    id: Option<u32>,
    armed: bool,
}

#[derive(Resource, Default)]
pub(crate) struct UserSettingsPersistence {
    path: Option<PathBuf>,
    last_payload: Option<String>,
}

#[derive(Resource)]
pub(crate) struct AppliedMasterVolume(f32);

pub(crate) fn load_user_settings(
    identity: Res<ui::LaunchIdentity>,
    mut settings: ResMut<frame::GameSettings>,
    mut binds: ResMut<KeyBinds>,
    mut persistence: ResMut<UserSettingsPersistence>,
) {
    let Some(path) = settings_path(&identity.artifacts) else {
        warn!("no HOME or XDG_CONFIG_HOME; user settings are session-only");
        return;
    };
    persistence.path = Some(path.clone());
    match fs::read_to_string(&path) {
        Ok(source) => parse_settings(&source, &mut settings, &mut binds),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!("could not read {}: {error}", path.display()),
    }
    settings.sanitize();
    persistence.last_payload = Some(serialize_settings(&settings, &binds));
}

pub(crate) fn consume_menu_binding(
    mut intents: MessageReader<ui::UiIntent>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut pending: ResMut<PendingMenuBinding>,
    mut binds: ResMut<KeyBinds>,
    mut view: ResMut<ui::BindingView>,
) {
    let mut began = false;
    for intent in intents.read() {
        if let ui::UiIntent::BeginBinding { id } = intent {
            pending.id = Some(*id);
            pending.armed = false;
            began = true;
        }
    }
    let Some(id) = pending.id else { return };
    if began || !pending.armed {
        pending.armed = true;
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        pending.id = None;
        pending.armed = false;
        view.listening = None;
        view.revision = view.revision.wrapping_add(1);
        return;
    }
    let button = keys
        .get_just_pressed()
        .copied()
        .map(BindButton::Key)
        .next()
        .or_else(|| {
            mouse
                .get_just_pressed()
                .copied()
                .map(BindButton::Mouse)
                .next()
        });
    let Some(button) = button else { return };
    binds.clear_command(id);
    binds.set(button, id);
    pending.id = None;
    pending.armed = false;
    view.listening = None;
    view.revision = view.revision.wrapping_add(1);
}

pub(crate) fn sync_binding_view(binds: Res<KeyBinds>, mut view: ResMut<ui::BindingView>) {
    if !binds.is_changed() {
        return;
    }
    let mut chords = std::collections::BTreeMap::<u32, Vec<String>>::new();
    for (button, id) in binds.iter() {
        chords.entry(id).or_default().push(display_button(button));
    }
    view.chords.clear();
    for (id, mut names) in chords {
        names.sort();
        names.dedup();
        view.chords.insert(id, names.join(" OR "));
    }
    view.revision = view.revision.wrapping_add(1);
}

pub(crate) fn apply_master_volume(
    settings: Res<frame::GameSettings>,
    mut global: ResMut<GlobalVolume>,
    mut sinks: Query<&mut AudioSink>,
    applied: Option<ResMut<AppliedMasterVolume>>,
    mut commands: Commands,
) {
    if !settings.is_changed() {
        return;
    }
    let previous = applied.as_ref().map_or(1.0, |value| value.0);
    let next = settings.master_volume;
    global.volume = Volume::Linear(next);
    for mut sink in &mut sinks {
        let base = if previous > f32::EPSILON {
            sink.volume().to_linear() / previous
        } else {
            sink.volume().to_linear()
        };
        sink.set_volume(Volume::Linear(base * next));
    }
    if let Some(mut applied) = applied {
        applied.0 = next;
    } else {
        commands.insert_resource(AppliedMasterVolume(next));
    }
}

pub(crate) fn sync_player_name(
    settings: Res<frame::GameSettings>,
    mut installed: MessageReader<frame::MatchInstalled>,
    local: Option<Res<net::LocalPresentClient>>,
    mut inbox: Option<ResMut<net::ClientActionInbox>>,
    mut seq: ResMut<net::ActionRequestIds>,
) {
    let installed_now = installed.read().next().is_some();
    if !settings.is_changed() && !installed_now {
        return;
    }
    let (Some(local), Some(inbox)) = (local, inbox.as_deref_mut()) else {
        return;
    };
    let request_id = seq.allocate();
    if let Err(error) = inbox.push(
        local.0,
        sim::ClientAction::SetName {
            request_id,
            name: entity_iw4::pack_client_state_name(&settings.player_name),
        },
    ) {
        diag::warn!(
            Console,
            "name: request_id={request_id} not queued — {error}"
        );
    }
}

pub(crate) fn save_user_settings(
    settings: Res<frame::GameSettings>,
    binds: Res<KeyBinds>,
    mut persistence: ResMut<UserSettingsPersistence>,
) {
    if !settings.is_changed() && !binds.is_changed() {
        return;
    }
    let payload = serialize_settings(&settings, &binds);
    if persistence.last_payload.as_deref() == Some(payload.as_str()) {
        return;
    }
    let Some(path) = persistence.path.clone() else {
        return;
    };
    let Some(parent) = path.parent() else { return };
    if let Err(error) = fs::create_dir_all(parent) {
        warn!("could not create {}: {error}", parent.display());
        return;
    }
    let temporary = path.with_extension("cfg.tmp");
    if let Err(error) =
        fs::write(&temporary, payload.as_bytes()).and_then(|()| fs::rename(&temporary, &path))
    {
        warn!("could not atomically save {}: {error}", path.display());
        return;
    }
    persistence.last_payload = Some(payload);
}

fn settings_path(artifacts: &std::path::Path) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("IW4L_SETTINGS_PATH") {
        return Some(PathBuf::from(path));
    }
    if cfg!(windows) {
        return Some(artifacts.join("settings.cfg"));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".config")))
        .map(|path| path.join("iw4l/settings.cfg"))
}

fn serialize_settings(settings: &frame::GameSettings, binds: &KeyBinds) -> String {
    let safe_name = settings.player_name.replace(['\n', '\r', '='], " ");
    let mut lines = vec![
        "// IW4L user settings v2".to_owned(),
        format!(
            "resolution={}x{}",
            settings.resolution.width, settings.resolution.height
        ),
        format!("fullscreen={}", settings.fullscreen),
        format!("vsync={}", settings.vsync),
        format!("fov={:.0}", settings.fov),
        format!("master_volume={:.3}", settings.master_volume),
        format!("sensitivity={:.3}", settings.sensitivity),
        format!("invert_mouse={}", settings.invert_mouse),
        format!("player_name={safe_name}"),
        "unbindall".to_owned(),
    ];
    lines.extend(binds.list_lines());
    lines.push(String::new());
    lines.join("\n")
}

fn parse_settings(source: &str, settings: &mut frame::GameSettings, binds: &mut KeyBinds) {
    let mut bind_script = String::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if line.starts_with("bind ") || line == "unbindall" {
            bind_script.push_str(line);
            bind_script.push('\n');
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            warn!("ignored malformed setting line: {line}");
            continue;
        };
        match key {
            "resolution" => {
                if let Some((w, h)) = value.split_once('x')
                    && let (Ok(width), Ok(height)) = (w.parse(), h.parse())
                {
                    settings.resolution = frame::DisplayResolution::new(width, height);
                }
            }
            "fullscreen" => {
                if let Ok(value) = value.parse() {
                    settings.fullscreen = value;
                }
            }
            "vsync" => {
                if let Ok(value) = value.parse() {
                    settings.vsync = value;
                }
            }
            "master_volume" => {
                if let Ok(value) = value.parse() {
                    settings.master_volume = value;
                }
            }
            "fov" => {
                if let Ok(value) = value.parse() {
                    settings.fov = value;
                }
            }
            "sensitivity" => {
                if let Ok(value) = value.parse() {
                    settings.sensitivity = value;
                }
            }
            "invert_mouse" => {
                if let Ok(value) = value.parse() {
                    settings.invert_mouse = value;
                }
            }
            "player_name" => settings.player_name = value.to_owned(),
            _ => warn!("ignored unknown setting `{key}`"),
        }
    }
    if !bind_script.is_empty() {
        let warnings = binds.apply_config_script(&bind_script);
        for warning in warnings {
            warn!("settings bind: {warning}");
        }
    }
    if source
        .lines()
        .next()
        .is_some_and(|line| line.trim() == "// IW4L user settings v1")
        && binds.get(BindButton::Key(KeyCode::Digit4)).is_none()
        && !binds.iter().any(|(_, id)| id == 21)
    {
        binds.set(BindButton::Key(KeyCode::Digit4), 21);
    }
}
