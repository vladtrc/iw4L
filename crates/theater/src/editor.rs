use std::path::PathBuf;

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, futures_lite::future};
use frame::{AppScreen, ClientSet, RuntimeRole};
use net::{LocalPresentClient, PresentedSnapshot};
use render_scene::{CameraView, FlyCamera, SimCamera};
use replay::{Playback, PlaybackControl, ReplayPlayback, ReplayPump};

use crate::browser::Browser;
use crate::project::{CameraKey, CameraMode, FilmInfo, Project, Segment, inspect};

#[derive(Clone, Debug)]
pub(crate) enum EditorAction {
    Open(PathBuf),
    Pause,
    Rate(f32),
    Step,
    Seek(u64),
    Skip(i64),
    Event(bool),
    Perspective,
    CameraMode(CameraMode),
    Player(bool),
    Hud,
    Controls,
    Tools,
    Record,
    Screenshot,
    Bookmark,
    CameraKey,
    Dolly,
    Attach(i32),
    Highlights,
    Save,
    Load,
    Export(String, usize),
    Delete(usize),
    Trim(usize, bool),
    Fov(f32),
    AttachNext,
    DeleteCamera,
    EditCamera(u64),
    UpdateCamera(u64),
    RetimeCamera(u64),
    RemoveCamera(u64),
    CameraPage(bool),
    CutPage(bool),
    Move(usize, usize),
    Preview,
    Exit,
}

struct CleanCapture {
    paths: Vec<PathBuf>,
    controls: bool,
    hud: bool,
    frames_left: u8,
    queued: bool,
}

#[derive(Resource)]
pub(crate) struct TheaterState {
    pub actions: Vec<EditorAction>,
    active: bool,
    closing: bool,
    path: Option<PathBuf>,
    info: FilmInfo,
    task: Option<Task<Result<FilmInfo, String>>>,
    seek_task: Option<Task<Result<(Playback, PlaybackControl), String>>>,
    export_task: Option<Task<Result<String, String>>>,
    project: Project,
    capture_start: Option<u64>,
    cut_page: usize,
    camera_page: usize,
    show_controls: bool,
    show_tools: bool,
    show_hud: bool,
    free: Option<Transform>,
    fov: f32,
    status: String,
    seek_loading: bool,
    seek_fade: f32,
    preview_segment: Option<usize>,
    screenshot_hold: f32,
    screenshot_taken: bool,
    capture: Option<CleanCapture>,
}

impl Default for TheaterState {
    fn default() -> Self {
        Self {
            actions: Vec::new(),
            active: false,
            closing: false,
            path: None,
            info: FilmInfo::default(),
            task: None,
            seek_task: None,
            export_task: None,
            project: Project::default(),
            capture_start: None,
            cut_page: 0,
            camera_page: 0,
            show_controls: true,
            show_tools: false,
            show_hud: true,
            free: None,
            fov: 80.0,
            status: String::new(),
            seek_loading: false,
            seek_fade: 0.0,
            preview_segment: None,
            screenshot_hold: 0.0,
            screenshot_taken: false,
            capture: None,
        }
    }
}

pub struct TheaterPlugin;

impl Plugin for TheaterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TheaterState>()
            .add_systems(
                PreUpdate,
                attach_editor.in_set(ClientSet::Receive).after(ReplayPump),
            )
            .add_systems(
                Update,
                (route_commands, hotkeys, apply_actions, seek_cover)
                    .chain()
                    .in_set(ClientSet::Diag)
                    .after(console::ConsoleDispatchSet),
            )
            .add_observer(button_actions)
            .add_observer(timeline_actions)
            .add_systems(Startup, register_command)
            .add_systems(Update, select_player.in_set(ClientSet::Input))
            .add_systems(
                Update,
                update_camera
                    .after(frame::PresentedPublished)
                    .before(render_anim::occupancy::view_kick::sync_camera_from_presented)
                    .before(render_scene::GfxSceneAdd)
                    .in_set(ClientSet::Present)
                    .in_set(frame::InMatch)
                    .run_if(resource_exists::<ReplayPlayback>),
            )
            .add_systems(Update, draw_editor.in_set(ClientSet::Ui))
            .add_systems(OnEnter(frame::MatchScope::Absent), release_editor)
            .add_systems(First, (sync_hud, clear_camera_without_replay).chain())
            .add_systems(PostUpdate, capture_previews);
        crate::browser::register(app);
        crate::navigation::register(app);
    }
}

fn release_editor(
    mut state: ResMut<TheaterState>,
    mut pending: ResMut<replay::PendingReplayArm>,
    mut browser: ResMut<Browser>,
    mut menus: MessageWriter<frame::UiMenuRequest>,
    (mut overlay, mut pointer, mut hud, mut camera): (
        ResMut<frame::UiOverlayInput>,
        ResMut<frame::UiPointerFree>,
        ResMut<frame::HudDraw>,
        ResMut<SimCamera>,
    ),
) {
    if !state.active && !state.closing {
        return;
    }
    *state = TheaterState::default();
    *pending = replay::PendingReplayArm::default();
    browser.open = false;
    browser.actions.clear();
    overlay.0 = false;
    pointer.0 = false;
    hud.0 = true;
    camera.view_override = None;
    menus.write(frame::UiMenuRequest::Close(
        "iw4l_theater_background".into(),
    ));
    menus.write(frame::UiMenuRequest::Open("iw4l_main".into()));
}

fn attach_editor(
    mut state: ResMut<TheaterState>,
    mut playback: Option<ResMut<ReplayPlayback>>,
    pending: Res<replay::PendingReplayArm>,
) {
    if state.closing {
        return;
    }
    if pending.playback.is_some() {
        return;
    }
    let Some(playback) = playback.as_deref_mut() else {
        return;
    };
    if state.seek_loading {
        state.status = "Ready".into();
    }
    state.seek_loading = false;
    if playback.quit_on_end && !state.active {
        return;
    }
    state.active = true;
    playback.retain_at_end = true;
    if state.path.as_deref() != Some(playback.path()) {
        let path = playback.path().to_path_buf();
        *state = TheaterState {
            active: true,
            path: Some(path.clone()),
            ..default()
        };
        state.task = Some(AsyncComputeTaskPool::get().spawn(async move { inspect(&path) }));
    }
    if let Some(task) = state.task.as_mut()
        && let Some(result) = future::block_on(future::poll_once(task))
    {
        state.task = None;
        match result {
            Ok(info) => {
                state.project.source_frames = info.frames;
                state.project.source_sha256 = info.source_sha256.clone();
                state.project.player = info
                    .killcam_viewer
                    .or_else(|| info.players.first().copied())
                    .unwrap_or(0);
                state.info = info;
                state.status = "Space pause · F3 camera · F4 controls · F2 HUD".into();
            }
            Err(error) => {
                warn!("theater recording index failed: {error}");
                state.status = error;
            }
        }
    }
}

fn register_command(mut registry: ResMut<console::ConsoleRegistry>) {
    registry.register(
        console::CommandSpec::new("ui_theater").usage("ui_theater — open the saved film library"),
    );
    registry.register(console::CommandSpec::new("theater").usage("theater [pause|speed N|step|seek SECONDS|camera|player|record|shot|bookmark|key|dolly|attach ENTITY|highlights|save|load|export NAME [SEGMENT]|delete SEGMENT|move FROM TO|preview|exit]"));
}

fn route_commands(
    mut events: MessageReader<console::ConsoleCommand>,
    mut state: ResMut<TheaterState>,
    mut browser: ResMut<Browser>,
    screen: Res<AppScreen>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut menus: MessageWriter<frame::UiMenuRequest>,
) {
    for command in events.read() {
        if command.name == "ui_theater"
            || (command.name == "theater"
                && command.args.is_empty()
                && *screen == AppScreen::MainMenu)
        {
            if *screen != AppScreen::MainMenu {
                continue;
            }
            browser.show();
            menus.write(frame::UiMenuRequest::Close("iw4l_main".into()));
            continue;
        }
        if command.name != "theater" {
            continue;
        }
        let args: Vec<_> = command.args.iter().map(String::as_str).collect();
        let action = match args.as_slice() {
            [] | ["pause"] => Some(EditorAction::Pause),
            ["play", name] => identity.as_ref().map(|identity| {
                EditorAction::Open(replay::resolve_playback_path(&identity.artifacts, name))
            }),
            ["speed", value] => value
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && (0.05..=16.0).contains(v))
                .map(EditorAction::Rate),
            ["step"] => Some(EditorAction::Step),
            ["seek", value] => value
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map(|v| EditorAction::Seek((v * 1000.0 / net::AUTHORITY_MS as f64) as u64)),
            ["camera"] => Some(EditorAction::Perspective),
            ["player"] => Some(EditorAction::Player(true)),
            ["record"] => Some(EditorAction::Record),
            ["shot"] => Some(EditorAction::Screenshot),
            ["bookmark"] => Some(EditorAction::Bookmark),
            ["key"] => Some(EditorAction::CameraKey),
            ["dolly"] => Some(EditorAction::Dolly),
            ["attach", value] => value.parse().ok().map(EditorAction::Attach),
            ["hud"] => Some(EditorAction::Hud),
            ["controls"] => Some(EditorAction::Controls),
            ["highlights"] => Some(EditorAction::Highlights),
            ["save"] => Some(EditorAction::Save),
            ["load"] => Some(EditorAction::Load),
            ["export", name] => Some(EditorAction::Export((*name).into(), 0)),
            ["export", name, index] => index
                .parse()
                .ok()
                .map(|i| EditorAction::Export((*name).into(), i)),
            ["delete", index] => index.parse().ok().map(EditorAction::Delete),
            ["move", from, to] => from
                .parse()
                .ok()
                .zip(to.parse().ok())
                .map(|(f, t)| EditorAction::Move(f, t)),
            ["preview"] => Some(EditorAction::Preview),
            ["exit"] => Some(EditorAction::Exit),
            _ => None,
        };
        match action {
            Some(action) => state.actions.push(action),
            None => state.status = "Invalid theater command. Use help theater.".into(),
        }
    }
}

fn hotkeys(
    mut state: ResMut<TheaterState>,
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    console: Res<console::ConsoleState>,
    screen: Res<AppScreen>,
    time: Res<Time>,
) {
    if !state.active || console.open || *screen != AppScreen::InGame {
        return;
    }
    for (key, action) in [
        (KeyCode::Space, EditorAction::Pause),
        (KeyCode::F2, EditorAction::Hud),
        (KeyCode::F3, EditorAction::Perspective),
        (KeyCode::F4, EditorAction::Controls),
        (KeyCode::ArrowLeft, EditorAction::Skip(-100)),
        (KeyCode::ArrowRight, EditorAction::Skip(100)),
        (KeyCode::ArrowDown, EditorAction::Rate(0.5)),
        (KeyCode::ArrowUp, EditorAction::Rate(2.0)),
        (KeyCode::Period, EditorAction::Step),
        (KeyCode::Comma, EditorAction::Skip(-1)),
        (KeyCode::Home, EditorAction::Seek(0)),
        (KeyCode::KeyB, EditorAction::Bookmark),
        (KeyCode::PageUp, EditorAction::Player(false)),
        (KeyCode::PageDown, EditorAction::Player(true)),
        (KeyCode::KeyK, EditorAction::CameraKey),
    ] {
        if keys.just_pressed(key) {
            state.actions.push(action);
        }
    }
    if !state.show_controls && state.project.camera_mode != CameraMode::Free {
        if buttons.just_pressed(MouseButton::Left) {
            state.actions.push(EditorAction::Player(false));
        }
        if buttons.just_pressed(MouseButton::Right) {
            state.actions.push(EditorAction::Player(true));
        }
    }
    if keys.just_pressed(KeyCode::KeyR) {
        state.screenshot_hold = 0.0;
        state.screenshot_taken = false;
    }
    if keys.pressed(KeyCode::KeyR) {
        state.screenshot_hold += time.delta_secs();
        if state.screenshot_hold >= 0.5 && !state.screenshot_taken {
            state.actions.push(EditorAction::Screenshot);
            state.screenshot_taken = true;
        }
    }
    if keys.just_released(KeyCode::KeyR) && !state.screenshot_taken {
        state.actions.push(EditorAction::Record);
    }
}

fn seek(state: &mut TheaterState, frame: u64, control: PlaybackControl) -> Result<(), String> {
    if state.info.frames == 0 {
        return Err("recording index is still loading".into());
    }
    let path = state.path.as_ref().ok_or("no recording")?.clone();
    let target = frame.min(state.info.frames.saturating_sub(1));
    let index = state.info.seek.clone();
    state.seek_task = Some(AsyncComputeTaskPool::get().spawn(async move {
        let mut playback = Playback::open_path(&path).map_err(|e| e.to_string())?;
        playback.seek(target, &index).map_err(|e| e.to_string())?;
        Ok((playback, control))
    }));
    state.preview_segment = None;
    state.status = "Seeking…".into();
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
fn apply_actions(
    mut state: ResMut<TheaterState>,
    mut playback: Option<ResMut<ReplayPlayback>>,
    mut pending: ResMut<replay::PendingReplayArm>,
    mut swap: ResMut<session::SessionSwapRequest>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut browser: ResMut<Browser>,
    camera: Query<&Transform, With<FlyCamera>>,
    presented: Option<Res<PresentedSnapshot>>,
    view: Option<Res<render_anim::occupancy::view_kick::SessionViewKick>>,
) {
    let actions = std::mem::take(&mut state.actions);
    for action in actions {
        if let EditorAction::Open(path) = action {
            let result = Playback::open_path(&path)
                .map_err(|e| e.to_string())
                .and_then(|film| {
                    let zone = film
                        .identity()
                        .zone_name()
                        .ok_or("recording has no map identity")?
                        .to_owned();
                    swap.request_demo(path.to_string_lossy().into_owned(), zone, false)?;
                    state.seek_task = None;
                    state.seek_loading = false;
                    state.preview_segment = None;
                    pending.control = None;
                    pending.playback = Some(film);
                    pending.quit_on_end = false;
                    pending.in_place = false;
                    browser.open = false;
                    state.active = true;
                    state.closing = false;
                    state.path = None;
                    Ok(())
                });
            if let Err(error) = result {
                state.status = error;
            }
            continue;
        }
        if state.closing {
            continue;
        }
        let Some(playback) = playback.as_deref_mut() else {
            state.status = "Open a saved recording from Theater first.".into();
            continue;
        };
        state.active = true;
        playback.quit_on_end = false;
        playback.retain_at_end = true;
        if matches!(
            action,
            EditorAction::Pause
                | EditorAction::Rate(_)
                | EditorAction::Step
                | EditorAction::Seek(_)
                | EditorAction::Skip(_)
                | EditorAction::Event(_)
                | EditorAction::EditCamera(_)
        ) {
            state.preview_segment = None;
            playback.control.stop_at = None;
        }
        let frame = playback.ticks().saturating_sub(1);
        let result: Result<(), String> = (|| {
            match action {
                EditorAction::Pause => {
                    if playback.ended {
                        seek(&mut state, 0, PlaybackControl::default())?;
                    } else {
                        playback.control.paused = !playback.control.paused;
                    }
                }
                EditorAction::Rate(rate) => {
                    playback.control.rate = rate;
                    playback.control.paused = false;
                }
                EditorAction::Step => playback.step(),
                EditorAction::Seek(target) => {
                    let control = playback.control;
                    seek(&mut state, target, control)?;
                }
                EditorAction::Skip(delta) => {
                    let target = frame.saturating_add_signed(delta);
                    let mut control = playback.control;
                    control.steps = 0;
                    control.paused = true;
                    seek(&mut state, target, control)?;
                }
                EditorAction::Event(next) => {
                    let target = if next {
                        state.info.events.iter().find(|e| e.frame > frame)
                    } else {
                        state.info.events.iter().rev().find(|e| e.frame < frame)
                    }
                    .map(|e| e.frame)
                    .ok_or("no more events")?;
                    seek(&mut state, target, playback.control)?;
                }
                EditorAction::CameraMode(mode) => {
                    state.fov = view
                        .as_ref()
                        .map_or(state.fov, |view| view.horiz_fov_deg.clamp(10.0, 150.0));
                    state.project.camera_mode = mode;
                    state.free = camera.iter().next().copied();
                }
                EditorAction::Perspective => {
                    state.fov = view
                        .as_ref()
                        .map_or(state.fov, |view| view.horiz_fov_deg.clamp(10.0, 150.0));
                    state.project.camera_mode = state.project.camera_mode.next();
                    state.free = camera.iter().next().copied();
                }
                EditorAction::Player(next) => {
                    let players = &state.info.players;
                    if !players.is_empty() {
                        let current = players
                            .iter()
                            .position(|&p| p == state.project.player)
                            .unwrap_or(0);
                        let index = if next {
                            (current + 1) % players.len()
                        } else {
                            (current + players.len() - 1) % players.len()
                        };
                        state.project.player = players[index];
                    }
                }
                EditorAction::Hud => state.show_hud = !state.show_hud,
                EditorAction::Controls => state.show_controls = !state.show_controls,
                EditorAction::Record => {
                    if let Some(start) = state.capture_start.take() {
                        let end = playback.ticks().min(state.info.frames);
                        if end <= start {
                            return Err("segment end must follow its start".into());
                        }
                        state.project.segments.push(Segment { start, end });
                    } else {
                        state.capture_start = Some(frame);
                    }
                }
                EditorAction::Screenshot => {
                    let preview = state
                        .path
                        .as_ref()
                        .ok_or("no recording")?
                        .with_extension("png");
                    let identity = identity.as_ref().ok_or("recording directory unavailable")?;
                    let id = replay::new_ulid().map_err(|e| e.to_string())?;
                    let screenshot = identity
                        .artifacts
                        .join("screenshots")
                        .join(format!("theater-{id}.png"));
                    arm_capture(&mut state, vec![preview, screenshot])?;
                    state.status = "Clean screenshot queued.".into();
                }
                EditorAction::Bookmark => {
                    if !state.project.bookmarks.contains(&frame) {
                        state.project.bookmarks.push(frame);
                        state.project.bookmarks.sort_unstable();
                    }
                }
                EditorAction::CameraKey => {
                    let transform = camera.iter().next().ok_or("camera unavailable")?;
                    let fov = view
                        .as_ref()
                        .map_or(state.fov, |view| view.horiz_fov_deg.clamp(10.0, 150.0));
                    state.project.add_camera(CameraKey {
                        frame,
                        origin: transform.translation.to_array(),
                        rotation: transform.rotation.to_array(),
                        fov,
                    });
                }
                EditorAction::Dolly => {
                    if state.project.cameras.is_empty() {
                        return Err("place camera keys first (K)".into());
                    }
                    state.project.camera_mode = CameraMode::Dolly;
                }
                EditorAction::Attach(entnum) => {
                    state.project.attached_entity = Some(entnum);
                    state.project.camera_mode = CameraMode::Attached;
                    state.free = camera.iter().next().copied();
                }
                EditorAction::Highlights => {
                    let events = state.info.events.clone();
                    let player = state.project.player;
                    let frames = state.info.frames;
                    state.project.highlights(&events, player, frames);
                }
                EditorAction::Save => {
                    state
                        .project
                        .validate(state.info.frames, &state.info.source_sha256)?;
                    let bytes =
                        serde_json::to_vec_pretty(&state.project).map_err(|e| e.to_string())?;
                    let path = state
                        .path
                        .as_ref()
                        .ok_or("no recording")?
                        .with_extension("theater.json");
                    let temporary = path.with_extension("json.part");
                    std::fs::write(&temporary, bytes).map_err(|e| e.to_string())?;
                    std::fs::rename(temporary, path).map_err(|e| e.to_string())?;
                    state.status = "Project saved.".into();
                }
                EditorAction::Load => {
                    let bytes = std::fs::read(
                        state
                            .path
                            .as_ref()
                            .ok_or("no recording")?
                            .with_extension("theater.json"),
                    )
                    .map_err(|e| e.to_string())?;
                    let project: Project =
                        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    project.validate(state.info.frames, &state.info.source_sha256)?;
                    if !state.info.players.is_empty()
                        && !state.info.players.contains(&project.player)
                    {
                        return Err("project selects a player absent from this recording".into());
                    }
                    state.project = project;
                }
                EditorAction::Export(name, index) => {
                    if state.export_task.is_some() {
                        return Err("an export is already running".into());
                    }
                    let name = replay::sanitize_demo_name(&name).ok_or("export name is empty")?;
                    let identity = identity.as_ref().ok_or("recording directory unavailable")?;
                    let destination = replay::demo_path(&identity.artifacts, &name);
                    let segment = *state
                        .project
                        .segments
                        .get(index)
                        .ok_or("segment index is out of range")?;
                    let project = Project {
                        source_frames: state.info.frames,
                        source_sha256: state.info.source_sha256.clone(),
                        segments: vec![segment],
                        ..default()
                    };
                    let source = state.path.clone().ok_or("no recording")?;
                    state.export_task = Some(AsyncComputeTaskPool::get().spawn(async move {
                        let count = crate::project::export(&source, &destination, &project)?;
                        Ok(format!("Exported {count} frames to {name}."))
                    }));
                    state.status = "Exporting demo…".into();
                }
                EditorAction::Trim(index, start) => {
                    let total = state.info.frames;
                    let segment = state
                        .project
                        .segments
                        .get_mut(index)
                        .ok_or("segment index is out of range")?;
                    if start {
                        if frame >= segment.end {
                            return Err("start must precede end".into());
                        }
                        segment.start = frame;
                    } else {
                        let end = (frame + 1).min(total);
                        if end <= segment.start {
                            return Err("end must follow start".into());
                        }
                        segment.end = end;
                    }
                }
                EditorAction::Fov(delta) => {
                    if matches!(
                        state.project.camera_mode,
                        CameraMode::FirstPerson | CameraMode::Dolly
                    ) {
                        return Err(
                            "Edit FOV in Free camera; Dolly uses saved waypoint FOVs.".into()
                        );
                    }
                    state.fov = (state.fov + delta).clamp(10.0, 150.0);
                }
                EditorAction::AttachNext => {
                    let snapshot = presented
                        .as_ref()
                        .and_then(|p| p.snapshot())
                        .ok_or("no recorded objects")?;
                    let targets: Vec<i32> = snapshot.projectiles.iter().map(|p| p.entnum).collect();
                    if targets.is_empty() {
                        return Err("no recorded projectiles in this frame".into());
                    }
                    let next = state
                        .project
                        .attached_entity
                        .and_then(|n| targets.iter().position(|&p| p == n))
                        .map_or(0, |i| (i + 1) % targets.len());
                    state.project.attached_entity = Some(targets[next]);
                    state.project.camera_mode = CameraMode::Attached;
                    state.free = camera.iter().next().copied();
                }
                EditorAction::DeleteCamera => state.project.cameras.retain(|c| c.frame != frame),
                EditorAction::EditCamera(key_frame) => {
                    let key = *state
                        .project
                        .cameras
                        .iter()
                        .find(|key| key.frame == key_frame)
                        .ok_or("waypoint no longer exists")?;
                    state.free = Some(
                        Transform::from_translation(Vec3::from_array(key.origin))
                            .with_rotation(Quat::from_array(key.rotation)),
                    );
                    state.fov = key.fov;
                    state.project.camera_mode = CameraMode::Free;
                    let control = PlaybackControl {
                        paused: true,
                        steps: 0,
                        ..playback.control
                    };
                    seek(&mut state, key_frame, control)?;
                }
                EditorAction::UpdateCamera(key_frame) => {
                    let pose = camera.iter().next().ok_or("camera unavailable")?;
                    let fov = view
                        .as_ref()
                        .map_or(state.fov, |view| view.horiz_fov_deg.clamp(10.0, 150.0));
                    let key = state
                        .project
                        .cameras
                        .iter_mut()
                        .find(|key| key.frame == key_frame)
                        .ok_or("waypoint no longer exists")?;
                    key.origin = pose.translation.to_array();
                    key.rotation = pose.rotation.to_array();
                    key.fov = fov;
                }
                EditorAction::RetimeCamera(key_frame) => {
                    if state
                        .project
                        .cameras
                        .iter()
                        .any(|key| key.frame == frame && key.frame != key_frame)
                    {
                        return Err("another waypoint already uses this time".into());
                    }
                    let key = state
                        .project
                        .cameras
                        .iter_mut()
                        .find(|key| key.frame == key_frame)
                        .ok_or("waypoint no longer exists")?;
                    key.frame = frame;
                    state.project.cameras.sort_by_key(|key| key.frame);
                }
                EditorAction::RemoveCamera(key_frame) => {
                    state.project.cameras.retain(|key| key.frame != key_frame)
                }
                EditorAction::CameraPage(next) => {
                    let max = state.project.cameras.len().saturating_sub(1) / 3;
                    state.camera_page = if next {
                        (state.camera_page + 1).min(max)
                    } else {
                        state.camera_page.saturating_sub(1)
                    };
                }

                EditorAction::CutPage(next) => {
                    let max = state.project.segments.len().saturating_sub(1) / 3;
                    state.cut_page = if next {
                        (state.cut_page + 1).min(max)
                    } else {
                        state.cut_page.saturating_sub(1)
                    };
                }
                EditorAction::Delete(index) => {
                    if index >= state.project.segments.len() {
                        return Err("segment index is out of range".into());
                    }
                    state.project.segments.remove(index);
                }
                EditorAction::Move(from, to) => {
                    if from >= state.project.segments.len() || to >= state.project.segments.len() {
                        return Err("segment index is out of range".into());
                    }
                    let segment = state.project.segments.remove(from);
                    state.project.segments.insert(to, segment);
                }
                EditorAction::Preview => {
                    let segment = state
                        .project
                        .segments
                        .first()
                        .ok_or("record a segment first")?;
                    let start = segment.start;
                    let end = segment.end;
                    seek(
                        &mut state,
                        start,
                        PlaybackControl {
                            stop_at: Some(end),
                            ..default()
                        },
                    )?;
                    state.preview_segment = Some(0);
                }
                EditorAction::Tools => state.show_tools = !state.show_tools,
                EditorAction::Exit => {
                    swap.request_menu()?;
                    state.seek_task = None;
                    state.seek_loading = false;
                    state.preview_segment = None;
                    state.active = false;
                    state.closing = true;
                    state.task = None;
                    state.capture = None;
                    state.capture_start = None;
                    state.free = None;
                    pending.playback = None;
                    pending.control = None;
                    pending.in_place = false;
                    browser.open = false;
                }
                EditorAction::Open(_) => unreachable!(),
            }
            Ok(())
        })();
        if let Err(error) = result {
            warn!("theater command refused: {error}");
            state.status = error;
        }
    }
    state.cut_page = state
        .cut_page
        .min(state.project.segments.len().saturating_sub(1) / 3);
    state.camera_page = state
        .camera_page
        .min(state.project.cameras.len().saturating_sub(1) / 3);
    if let Some(task) = state.export_task.as_mut()
        && let Some(result) = future::block_on(future::poll_once(task))
    {
        state.export_task = None;
        state.status = result.unwrap_or_else(|error| error);
    }
    if let Some(task) = state.seek_task.as_mut()
        && let Some(result) = future::block_on(future::poll_once(task))
    {
        state.seek_task = None;
        let result = result.map(|(film, control)| {
            pending.control = Some(control);
            pending.playback = Some(film);
            pending.quit_on_end = false;
            pending.in_place = true;
            state.seek_loading = true;
        });
        if let Err(error) = result {
            state.status = error;
            state.preview_segment = None;
        }
    }
    if state.seek_task.is_some() {
        if let Some(playback) = playback.as_deref_mut() {
            playback.control.paused = true;
        }
        return;
    }
    if state.seek_loading {
        return;
    }
    if let Some(playback) = playback.as_deref_mut()
        && let Some(index) = state.preview_segment
        && let Some(segment) = state.project.segments.get(index)
        && playback.ticks() >= segment.end
    {
        if let Some(next) = state.project.segments.get(index + 1) {
            let start = next.start;
            let end = next.end;
            if let Err(error) = seek(
                &mut state,
                start,
                PlaybackControl {
                    stop_at: Some(end),
                    ..default()
                },
            ) {
                state.status = error;
                state.preview_segment = None;
            } else {
                state.preview_segment = Some(index + 1);
            }
        } else {
            playback.control.paused = true;
            state.preview_segment = None;
        }
    }
}

fn select_player(
    state: Res<TheaterState>,
    playback: Option<Res<ReplayPlayback>>,
    mut local: Option<ResMut<LocalPresentClient>>,
    role: Res<RuntimeRole>,
    active: Option<Res<net::GameActive>>,
    mut screen: ResMut<AppScreen>,
    mut camera: ResMut<SimCamera>,
) {
    if state.active
        && playback.is_some()
        && *role == RuntimeRole::Replay
        && let Some(local) = local.as_deref_mut()
    {
        local.0 = sim::ClientId(state.project.player);
        if active.is_some_and(|active| active.0) {
            *screen = AppScreen::InGame;
            camera.enabled = true;
            camera.freeze_fly = true;
        }
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct EditorCameraInput<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    buttons: Res<'w, ButtonInput<MouseButton>>,
    motion: Res<'w, AccumulatedMouseMotion>,
    time: Res<'w, Time>,
    console: Res<'w, console::ConsoleState>,
    world_camera: Query<'w, 's, &'static Transform, With<FlyCamera>>,
}

fn update_camera(
    mut state: ResMut<TheaterState>,
    playback: Option<Res<ReplayPlayback>>,
    mut presented: ResMut<PresentedSnapshot>,
    mut camera: ResMut<SimCamera>,
    input: EditorCameraInput,
    collision: Res<render_anim::occupancy::dyn_ent_phys::DynEntPhysClip>,
    clock: Res<net::FrameClock>,
) {
    camera.view_override = None;
    presented.set_external_camera(false);
    let Some(playback) = playback else {
        return;
    };
    if !state.active {
        return;
    }
    let mode = state.project.camera_mode;
    if mode == CameraMode::FirstPerson {
        if presented.snapshot().is_some_and(|snapshot| {
            snapshot
                .meta
                .for_client(sim::ClientId(state.project.player))
                .is_some_and(|meta| meta.killcam_hud.is_some())
        }) {
            return;
        }
        presented.set_first_person_camera();
        return;
    }
    let Some(current) = input.world_camera.iter().next().copied() else {
        return;
    };
    let transform = match mode {
        CameraMode::Free => {
            let mut transform = state.free.unwrap_or(current);
            if !input.console.open {
                if input.buttons.pressed(MouseButton::Right) {
                    transform.rotate_z(-input.motion.delta.x * 0.003);
                    transform.rotate_local_x(-input.motion.delta.y * 0.003);
                }
                let mut movement = Vec3::ZERO;
                for (key, direction) in [
                    (KeyCode::KeyW, *transform.forward()),
                    (KeyCode::KeyS, *transform.back()),
                    (KeyCode::KeyA, *transform.left()),
                    (KeyCode::KeyD, *transform.right()),
                    (KeyCode::KeyQ, -Vec3::Z),
                    (KeyCode::KeyE, Vec3::Z),
                ] {
                    if input.keys.pressed(key) {
                        movement += direction;
                    }
                }
                let speed = if input.keys.pressed(KeyCode::ShiftLeft) {
                    1200.0
                } else {
                    200.0
                };
                transform.translation +=
                    movement.normalize_or_zero() * speed * input.time.delta_secs();
            }
            state.free = Some(transform);
            transform
        }
        CameraMode::Dolly => {
            let frame = playback.presented_frame();
            if let Some((transform, fov)) = state.project.camera_at(frame) {
                state.fov = fov;
                transform
            } else {
                current
            }
        }
        CameraMode::Chase => {
            let Some(pose) = render_anim::occupancy::third_person::third_person_camera(
                &presented,
                sim::ClientId(state.project.player),
                collision.0.as_deref(),
            ) else {
                return;
            };
            render_scene::transform_from_iw_view(pose)
        }
        CameraMode::Attached => {
            let mut transform = state.free.unwrap_or(current);
            let target = state.project.attached_entity;
            if let Some(projectile) = presented.snapshot().and_then(|snapshot| {
                snapshot
                    .projectiles
                    .iter()
                    .find(|p| target == Some(p.entnum))
            }) {
                transform.translation = Vec3::from_array(projectile.origin_at(clock.time()));
                state.free = Some(transform);
            } else {
                state.status = "Attached object unavailable; holding camera.".into();
            }
            transform
        }
        CameraMode::FirstPerson => return,
    };
    camera.view_override = Some(CameraView {
        transform,
        horizontal_fov: state.fov,
    });
    presented.set_external_camera(true);
}

fn sync_hud(
    browser: Res<Browser>,
    screen: Res<AppScreen>,
    mut overlay: ResMut<frame::UiOverlayInput>,
    state: Res<TheaterState>,
    playback: Option<Res<ReplayPlayback>>,
    mut hud: ResMut<frame::HudDraw>,
    mut pointer: ResMut<frame::UiPointerFree>,
) {
    overlay.0 = (browser.open && *screen == AppScreen::MainMenu)
        || (state.active && playback.is_some() && *screen == AppScreen::InGame);
    hud.0 = !state.active || playback.is_none() || state.show_hud;
    pointer.0 = state.active && playback.is_some() && state.show_controls;
}

fn clear_camera_without_replay(
    playback: Option<Res<ReplayPlayback>>,
    mut camera: ResMut<SimCamera>,
) {
    if playback.is_none() {
        camera.view_override = None;
    }
}

#[derive(Component)]
pub(crate) struct EditorRoot;
#[derive(Component)]
struct EditorLabel;
#[derive(Component)]
pub(crate) struct TimelineBar;
#[derive(Component)]
struct TimelineCursor;
#[derive(Component)]
struct PlayheadTime;
#[derive(Component, Clone)]
pub(crate) struct EditorButton(pub(crate) EditorAction);

fn button_actions(
    activation: On<bevy::ui_widgets::Activate>,
    mut state: ResMut<TheaterState>,
    buttons: Query<&EditorButton>,
) {
    if let Ok(button) = buttons.get(activation.entity) {
        state.actions.push(button.0.clone());
    }
}

fn timeline_actions(
    activation: On<bevy::ui_widgets::Activate>,
    mut state: ResMut<TheaterState>,
    bars: Query<&bevy::ui::RelativeCursorPosition, With<TimelineBar>>,
) {
    if let Ok(cursor) = bars.get(activation.entity)
        && let Some(position) = cursor.normalized
    {
        let target = ((position.x + 0.5).clamp(0.0, 1.0) as f64
            * state.info.frames.saturating_sub(1) as f64)
            .round() as u64;
        state.actions.push(EditorAction::Seek(target));
    }
}

type EditorSignature = (
    Vec<Segment>,
    Vec<CameraKey>,
    Vec<u64>,
    usize,
    u32,
    usize,
    usize,
    bool,
);

const SEEK_BLUR: f32 = 6.0;
const SEEK_FADE_SECS: f32 = 0.5;

#[derive(Component)]
struct SeekCover;

fn seek_cover(
    mut commands: Commands,
    mut state: ResMut<TheaterState>,
    pending: Res<replay::PendingReplayArm>,
    time: Res<Time>,
    blur: Option<ResMut<frame::ScreenBlurRequest>>,
    font: Res<ui::GameUiFont>,
    covers: Query<Entity, With<SeekCover>>,
) {
    let seeking = state.seek_task.is_some() || state.seek_loading || pending.in_place;
    state.seek_fade = if seeking {
        1.0
    } else {
        (state.seek_fade - time.delta_secs() / SEEK_FADE_SECS).max(0.0)
    };
    if let Some(mut blur) = blur {
        blur.0 = SEEK_BLUR * state.seek_fade;
    }
    if !seeking {
        for cover in &covers {
            commands.entity(cover).despawn();
        }
        return;
    }
    if !covers.is_empty() {
        return;
    }
    commands
        .spawn((
            SeekCover,
            ui::UiLayer::Overlay,
            ui::UiLayerVisibility,
            GlobalZIndex(49),
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.25)),
            bevy::picking::Pickable::IGNORE,
        ))
        .with_children(|cover| {
            cover.spawn((
                Text::new("Seeking…"),
                ui::game_text_font(&font.0, 28.0),
                TextColor(Color::WHITE),
                bevy::picking::Pickable::IGNORE,
            ));
        });
}

fn editor_clock_text(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

#[derive(bevy::ecs::system::SystemParam)]
struct EditorView<'w, 's> {
    roots: Query<'w, 's, Entity, With<EditorRoot>>,
    labels: Query<'w, 's, &'static mut Text, (With<EditorLabel>, Without<PlayheadTime>)>,
    cursors: Query<'w, 's, &'static mut Node, With<TimelineCursor>>,
    playhead_labels: Query<'w, 's, &'static mut Text, (With<PlayheadTime>, Without<EditorLabel>)>,
}

impl EditorView<'_, '_> {
    fn clear(&self, commands: &mut Commands) {
        for root in &self.roots {
            commands.entity(root).despawn();
        }
    }

    fn is_spawned(&self) -> bool {
        !self.roots.is_empty()
    }

    fn update_playhead(&mut self, label: &str, seconds: f64, frame: u64, total_frames: u64) {
        for mut text in &mut self.labels {
            text.0 = label.to_owned();
        }
        for mut text in &mut self.playhead_labels {
            text.0 = editor_clock_text(seconds);
        }
        for mut cursor in &mut self.cursors {
            cursor.left =
                Val::Percent(100.0 * frame as f32 / total_frames.saturating_sub(1).max(1) as f32);
        }
    }
}

fn draw_editor(
    mut commands: Commands,
    state: Res<TheaterState>,
    playback: Option<Res<ReplayPlayback>>,
    screen: Res<AppScreen>,
    font: Res<ui::GameUiFont>,
    mut view: EditorView,
    mut signature: Local<Option<EditorSignature>>,
) {
    let show =
        state.active && state.show_controls && playback.is_some() && *screen == AppScreen::InGame;
    if !show {
        view.clear(&mut commands);
        return;
    }
    let playback = playback.unwrap();
    let frame = playback.ticks().saturating_sub(1);
    let seconds = frame as f64 * net::AUTHORITY_MS as f64 / 1000.0;
    let duration = state.info.frames as f64 * net::AUTHORITY_MS as f64 / 1000.0;
    let camera_name = match state.project.camera_mode {
        CameraMode::FirstPerson => "1ST PERSON",
        CameraMode::Chase => "CHASE",
        CameraMode::Free => "FREE CAMERA",
        CameraMode::Dolly => "DOLLY",
        CameraMode::Attached => "ATTACHED",
    };
    let label = format!(
        "{}   ·   PLAYER {}                      MODE: Film Editor   ·   {} {:.2}×{}",
        camera_name,
        state.project.player,
        if playback.ended {
            "END"
        } else if playback.control.paused {
            "PAUSED"
        } else {
            "PLAY"
        },
        playback.control.rate,
        if state.capture_start.is_some() {
            "   ·   RECORDING"
        } else {
            ""
        },
    );
    view.update_playhead(&label, seconds, frame, state.info.frames);
    let wanted = (
        state.project.segments.clone(),
        state.project.cameras.clone(),
        state.project.bookmarks.clone(),
        state.info.events.len(),
        state.project.player,
        state.cut_page,
        state.camera_page,
        state.show_tools,
    );
    if view.is_spawned() && signature.as_ref() == Some(&wanted) {
        return;
    }
    *signature = Some(wanted);
    view.clear(&mut commands);
    commands
        .spawn((
            EditorRoot,
            ui::UiLayer::Overlay,
            ui::UiLayerVisibility,
            GlobalZIndex(50),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(25.0),
                left: Val::Percent(12.0),
                width: Val::Percent(76.0),
                padding: UiRect::all(Val::Px(12.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(8.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.58)),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("F4 / Start  Toggle Controls                                   F2 / North  Toggle HUD"),
                ui::game_text_font(&font.0, 18.0),
                TextColor(Color::srgb(0.85, 0.87, 0.45)),
                bevy::picking::Pickable::IGNORE,
            ));
            root.spawn((
                EditorLabel,
                Text::new(label),
                ui::game_text_font(&font.0, 17.0),
                TextColor(Color::WHITE),
                bevy::picking::Pickable::IGNORE,
            ));
            root.spawn((
                bevy::ui_widgets::Button,
                bevy::picking::hover::Hovered::default(),
                TimelineBar,
                bevy::ui::RelativeCursorPosition::default(),
                Node {
                    width: Val::Percent(100.0),
                    height: Val::Px(18.0),
                    margin: UiRect::top(Val::Px(28.0)),
                    border: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.45, 0.45, 0.42)),
                BackgroundColor(Color::srgba(0.02, 0.025, 0.02, 0.9)),
            ))
            .with_children(|bar| {
                for event in &state.info.events {
                    let color = if event.victim == state.project.player {
                        Color::srgb(0.95, 0.2, 0.15)
                    } else if event.attacker == Some(state.project.player) {
                        Color::srgb(0.4, 0.95, 0.2)
                    } else {
                        continue;
                    };
                    bar.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(
                                100.0 * event.frame as f32
                                    / state.info.frames.saturating_sub(1).max(1) as f32,
                            ),
                            width: Val::Px(2.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(color),
                        bevy::ui::FocusPolicy::Pass,
                        bevy::picking::Pickable::IGNORE,
                    ));
                }
                for key in &state.project.cameras {
                    bar.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(
                                100.0 * key.frame as f32
                                    / state.info.frames.saturating_sub(1).max(1) as f32,
                            ),
                            width: Val::Px(4.0),
                            height: Val::Percent(100.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.3, 0.7, 1.0)),
                        bevy::ui::FocusPolicy::Pass,
                        bevy::picking::Pickable::IGNORE,
                    ));
                }
                for &frame in &state.project.bookmarks {
                    bar.spawn((
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Percent(
                                100.0 * frame as f32
                                    / state.info.frames.saturating_sub(1).max(1) as f32,
                            ),
                            width: Val::Px(3.0),
                            height: Val::Percent(55.0),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.9, 0.8, 0.1)),
                        bevy::ui::FocusPolicy::Pass,
                        bevy::picking::Pickable::IGNORE,
                    ));
                }
                bar.spawn((
                    TimelineCursor,
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(
                            100.0 * frame as f32
                                / state.info.frames.saturating_sub(1).max(1) as f32,
                        ),
                        width: Val::Px(2.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    BackgroundColor(Color::WHITE),
                    bevy::ui::FocusPolicy::Pass,
                    bevy::picking::Pickable::IGNORE,
                )).with_children(|cursor| {
                    cursor.spawn((
                        PlayheadTime,
                        Text::new(editor_clock_text(seconds)),
                        ui::game_text_font(&font.0, 20.0),
                        TextColor(Color::WHITE),
                        Node { position_type: PositionType::Absolute, top: Val::Px(-28.0), left: Val::Px(-20.0), ..default() },
                        bevy::picking::Pickable::IGNORE,
                    ));
                });
            });
            root.spawn((
                Text::new(format!("{}   ·   Green: kills   Red: deaths   ·   D-pad: navigate   A / Enter: select   B / Esc: exit", editor_clock_text(duration))),
                ui::game_text_font(&font.0, 14.0),
                TextColor(Color::srgb(0.85, 0.87, 0.45)),
                bevy::picking::Pickable::IGNORE,
            ));
            root.spawn(Node { column_gap: Val::Px(3.0), ..default() }).with_children(|row| {
                for (title, action) in [
                    ("CAMERA", EditorAction::Perspective),
                    ("|<", EditorAction::Seek(0)),
                    ("<|", EditorAction::Event(false)),
                    ("PLAY / PAUSE", EditorAction::Pause),
                    ("STEP >", EditorAction::Step),
                    (">>", EditorAction::Rate(4.0)),
                    ("|>", EditorAction::Event(true)),
                    ("RECORD", EditorAction::Record),
                    ("SCREENSHOT", EditorAction::Screenshot),
                    ("TOOLS", EditorAction::Tools),
                    ("EXIT", EditorAction::Exit),
                ] {
                    row.spawn((
                        bevy::ui_widgets::Button,
                        bevy::picking::hover::Hovered::default(),
                        EditorButton(action),
                        Node { flex_grow: 1.0, padding: UiRect::axes(Val::Px(6.0), Val::Px(7.0)), justify_content: JustifyContent::Center, border: UiRect::all(Val::Px(1.0)), ..default() },
                        BorderColor::all(Color::srgb(0.3, 0.3, 0.28)),
                        BackgroundColor(Color::srgba(0.12, 0.12, 0.11, 0.85)),
                    )).with_children(|p| {
                        p.spawn((Text::new(title),
                        TextLayout::no_wrap(), ui::game_text_font(&font.0, 12.0), TextColor(Color::WHITE), bevy::picking::Pickable::IGNORE));
                    });
                }
            });
            if !state.show_tools { return; }
            root.spawn((Text::new(&state.status), ui::game_text_font(&font.0, 14.0), TextColor(Color::WHITE), bevy::picking::Pickable::IGNORE));
            root.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: Val::Px(5.0),
                row_gap: Val::Px(5.0),
                ..default()
            })
            .with_children(|row| {
                for (title, action) in [
                    ("|<", EditorAction::Seek(0)),
                    ("< event", EditorAction::Event(false)),
                    ("Pause / Play", EditorAction::Pause),
                    ("Step", EditorAction::Step),
                    ("event >", EditorAction::Event(true)),
                    (
                        "First person",
                        EditorAction::CameraMode(CameraMode::FirstPerson),
                    ),
                    ("Chase", EditorAction::CameraMode(CameraMode::Chase)),
                    ("Free camera", EditorAction::CameraMode(CameraMode::Free)),
                    ("Camera key", EditorAction::CameraKey),
                    ("Dolly", EditorAction::Dolly),
                    ("Remove key", EditorAction::DeleteCamera),
                    ("Attach object", EditorAction::AttachNext),
                    ("FOV −", EditorAction::Fov(-5.0)),
                    ("FOV +", EditorAction::Fov(5.0)),
                    ("Bookmark", EditorAction::Bookmark),
                    ("0.25×", EditorAction::Rate(0.25)),
                    ("1×", EditorAction::Rate(1.0)),
                    ("4×", EditorAction::Rate(4.0)),
                    ("Player", EditorAction::Player(true)),
                    ("Record", EditorAction::Record),
                    ("Screenshot", EditorAction::Screenshot),
                    ("Highlight reel", EditorAction::Highlights),
                    ("Preview cuts", EditorAction::Preview),
                    ("Save project", EditorAction::Save),
                    ("Load project", EditorAction::Load),
                    ("Exit", EditorAction::Exit),
                ] {
                    row.spawn((
                        bevy::ui_widgets::Button,
                        bevy::picking::hover::Hovered::default(),
                        EditorButton(action),
                        Node {
                            padding: UiRect::all(Val::Px(7.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.14, 0.17, 0.18)),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            Text::new(title),
                            ui::game_text_font(&font.0, 15.0),
                            TextColor(Color::WHITE),
                            bevy::picking::Pickable::IGNORE,
                        ));
                    });
                }
            });
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                ..default()
            })
            .with_children(|list| {
                for (index, segment) in state
                    .project
                    .segments
                    .iter()
                    .enumerate()
                    .skip(state.cut_page * 3)
                    .take(3)
                {
                    list.spawn(Node {
                        column_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format!(
                                "Cut {index}: {:.2}–{:.2}s",
                                segment.start as f64 * 0.05,
                                segment.end as f64 * 0.05
                            )),
                            ui::game_text_font(&font.0, 14.0),
                            TextColor(Color::WHITE),
                            bevy::picking::Pickable::IGNORE,
                        ));
                        let name = format!(
                            "{}-cut-{index}",
                            state
                                .path
                                .as_ref()
                                .and_then(|p| p.file_stem())
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_else(|| "film".into())
                        );
                        for (title, action) in [
                            ("Go", EditorAction::Seek(segment.start)),
                            ("Set start", EditorAction::Trim(index, true)),
                            ("Set end", EditorAction::Trim(index, false)),
                            (
                                "Move up",
                                EditorAction::Move(index, index.saturating_sub(1)),
                            ),
                            ("Delete", EditorAction::Delete(index)),
                            ("Export demo", EditorAction::Export(name, index)),
                        ] {
                            row.spawn((
                                bevy::ui_widgets::Button,
                                bevy::picking::hover::Hovered::default(),
                                EditorButton(action),
                                Node {
                                    padding: UiRect::all(Val::Px(5.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.1, 0.13, 0.15)),
                            ))
                            .with_children(|p| {
                                p.spawn((
                                    Text::new(title),
                                    ui::game_text_font(&font.0, 13.0),
                                    TextColor(Color::WHITE),
                                    bevy::picking::Pickable::IGNORE,
                                ));
                            });
                        }
                    });
                }
                for (index, key) in state
                    .project
                    .cameras
                    .iter()
                    .enumerate()
                    .skip(state.camera_page * 3)
                    .take(3)
                {
                    list.spawn(Node {
                        column_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|row| {
                        row.spawn((
                            Text::new(format!(
                                "Waypoint {index}: {:.2}s · {:.0}°",
                                key.frame as f64 * 0.05,
                                key.fov
                            )),
                            ui::game_text_font(&font.0, 14.0),
                            TextColor(Color::srgb(0.3, 0.7, 1.0)),
                            bevy::picking::Pickable::IGNORE,
                        ));
                        for (title, action) in [
                            ("Edit pose", EditorAction::EditCamera(key.frame)),
                            ("Update pose", EditorAction::UpdateCamera(key.frame)),
                            ("Set time", EditorAction::RetimeCamera(key.frame)),
                            ("Delete waypoint", EditorAction::RemoveCamera(key.frame)),
                        ] {
                            row.spawn((
                                bevy::ui_widgets::Button,
                                bevy::picking::hover::Hovered::default(),
                                EditorButton(action),
                                Node {
                                    padding: UiRect::all(Val::Px(5.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.1, 0.13, 0.15)),
                            ))
                            .with_children(|p| {
                                p.spawn((
                                    Text::new(title),
                                    ui::game_text_font(&font.0, 13.0),
                                    TextColor(Color::WHITE),
                                    bevy::picking::Pickable::IGNORE,
                                ));
                            });
                        }
                    });
                }
                if state.project.cameras.len() > 3 {
                    list.spawn(Node {
                        column_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for (title, action) in [
                            ("Earlier waypoints", EditorAction::CameraPage(false)),
                            ("More waypoints", EditorAction::CameraPage(true)),
                        ] {
                            row.spawn((
                                bevy::ui_widgets::Button,
                                bevy::picking::hover::Hovered::default(),
                                EditorButton(action),
                                Node {
                                    padding: UiRect::all(Val::Px(5.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.1, 0.13, 0.15)),
                            ))
                            .with_children(|p| {
                                p.spawn((
                                    Text::new(title),
                                    ui::game_text_font(&font.0, 13.0),
                                    TextColor(Color::WHITE),
                                    bevy::picking::Pickable::IGNORE,
                                ));
                            });
                        }
                    });
                }
                if state.project.segments.len() > 3 {
                    list.spawn(Node {
                        column_gap: Val::Px(5.0),
                        ..default()
                    })
                    .with_children(|row| {
                        for (title, action) in [
                            ("Earlier cuts", EditorAction::CutPage(false)),
                            ("More cuts", EditorAction::CutPage(true)),
                        ] {
                            row.spawn((
                                bevy::ui_widgets::Button,
                                bevy::picking::hover::Hovered::default(),
                                EditorButton(action),
                                Node {
                                    padding: UiRect::all(Val::Px(5.0)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.1, 0.13, 0.15)),
                            ))
                            .with_children(|p| {
                                p.spawn((
                                    Text::new(title),
                                    ui::game_text_font(&font.0, 13.0),
                                    TextColor(Color::WHITE),
                                    bevy::picking::Pickable::IGNORE,
                                ));
                            });
                        }
                    });
                }
            });
        });
}

fn arm_capture(state: &mut TheaterState, paths: Vec<PathBuf>) -> Result<(), String> {
    if state.capture.is_some() {
        return Err("a screenshot is still being saved".into());
    }
    state.capture = Some(CleanCapture {
        paths,
        controls: state.show_controls,
        hud: state.show_hud,
        frames_left: 2,
        queued: false,
    });
    state.show_controls = false;
    state.show_hud = false;
    Ok(())
}

#[derive(bevy::ecs::system::SystemParam)]
struct PreviewMedia<'w, 's> {
    recorder: Res<'w, replay::ReplaySession>,
    previous: Local<'s, Option<PathBuf>>,
    played: Local<'s, Option<PathBuf>>,
    playback: Option<Res<'w, ReplayPlayback>>,
}

fn capture_previews(
    mut state: ResMut<TheaterState>,
    mut media: PreviewMedia,
    active: Option<Res<net::GameActive>>,
    console: Res<console::ConsoleState>,
    mut events: MessageReader<console::ConsoleCommand>,
    identity: Option<Res<frame::LaunchIdentity>>,
    mut capture: ResMut<render::diag::capture::CaptureQueue>,
) {
    if let Some(clean) = state.capture.as_mut() {
        if clean.frames_left > 0 {
            clean.frames_left -= 1;
        } else if !clean.queued && !console.open {
            for path in &clean.paths {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                capture.push(render::diag::capture::CaptureRequest {
                    path: path.clone(),
                    exit_after_capture: false,
                    any_screen: false,
                });
            }
            clean.queued = true;
        } else if clean.queued && capture.owed_at_exit() == (0, 0) {
            let controls = clean.controls;
            let hud = clean.hud;
            state.show_controls = controls;
            state.show_hud = hud;
            state.capture = None;
        }
    }
    if !console.open
        && state.capture.is_none()
        && let Some(playback) = media.playback.as_ref()
        && state.active
        && active.is_some_and(|active| active.0)
        && media.played.as_deref() != Some(playback.path())
    {
        let path = playback.path().to_path_buf();
        if !path.with_extension("png").is_file() {
            let _ = arm_capture(&mut state, vec![path.with_extension("png")]);
        }
        *media.played = Some(path);
    }
    if media.playback.is_none() {
        *media.played = None;
    }
    if let Some(recording) = media.recorder.0.as_ref() {
        if !console.open && media.previous.as_deref() != Some(recording.path()) {
            let path = recording.path().to_path_buf();
            capture.push(render::diag::capture::CaptureRequest {
                path: path.with_extension("png"),
                exit_after_capture: false,
                any_screen: false,
            });
            *media.previous = Some(path);
        }
    } else {
        *media.previous = None;
    }
    for event in events.read() {
        if event.name == "clip"
            && event.args.is_empty()
            && let Some(identity) = identity.as_ref()
        {
            let path = replay::resolve_playback_path(&identity.artifacts, "LATEST");
            if path.is_file() {
                capture.push(render::diag::capture::CaptureRequest {
                    path: path.with_extension("png"),
                    exit_after_capture: false,
                    any_screen: false,
                });
            }
        }
    }
}
