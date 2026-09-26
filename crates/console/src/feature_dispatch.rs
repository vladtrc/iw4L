use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bots::{
    BotAddQueue, BotFireQueue, BotHold, BotRoster, BotTpQueue, BotTpRequest, BotTpTarget,
    BotTpWhere,
};
use frame::{HasWorld, LaunchIdentity, RuntimeRole};
use hud::PendingSplash;
use net::{
    AuthorityClock, AuthorityInputGate, AuthorityWorld, ClientActionInbox, MasterBridge,
    MasterBridgeState, PresentedSnapshot,
};
use render::diag::capture::{CaptureQueue, CaptureRequest};
use render_frontend::adapters::anim::view_kick::PendingViewHurt;
use render_frontend::prepare::scene::camera::SimCamera;
use replay::{
    CLIP_DEMO_FILE, CLIP_DUMP_FILE, CLIP_MANIFEST_FILE, CLIP_MS, ClipRing, MatchRecordIdentity,
    Recording, ReplaySession,
};
use sim::{ClientAction, ClientId};
use ui::{
    Focus, MenuEnabled, MenuMapList, MenuShellCmd, NavDir, RetailMenuStack, UiDraw, play_map_layout,
};

use crate::{ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleSettings, ConsoleState};

#[derive(SystemParam)]
pub(crate) struct ConsoleEcho<'w> {
    console: ResMut<'w, ConsoleState>,
    settings: Res<'w, ConsoleSettings>,
    line: ResMut<'w, ConsoleLine>,
}

impl ConsoleEcho<'_> {
    fn write(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        diag::info!(Console, "{msg}");
        self.line.0 = msg.clone();
        self.console.echo(msg, self.settings.log_capacity);
    }
}

pub(crate) fn route_session_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    mut transition: ResMut<session::SessionSwapRequest>,
    has_world: Res<HasWorld>,
    playback: Option<Res<replay::ReplayPlayback>>,
    bridge: Option<Res<net::MasterBridge>>,
    identity: Option<Res<LaunchIdentity>>,
    mut dispatch: ResMut<ConsoleDispatch>,
) {
    for cmd in events.read() {
        match cmd.name.as_str() {
            "map_restart" => {
                let zone = identity
                    .as_ref()
                    .map(|identity| identity.zone.clone())
                    .filter(|zone| has_world.0 && !zone.is_empty());
                match (cmd.args.is_empty(), zone) {
                    (false, _) => {
                        dispatch.release();
                        echo.write("usage: map_restart");
                    }
                    (true, None) => {
                        dispatch.release();
                        echo.write("map_restart: no map to restart");
                    }
                    (true, Some(zone)) => match transition.request_zone(zone.clone()) {
                        Ok(id) => {
                            echo.write(format!("map_restart: requested `{zone}` (swap #{id})"))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("map_restart: {error}"));
                        }
                    },
                }
            }
            "map" => match cmd.args.as_slice() {
                [zone] => match transition.request_zone(zone.clone()) {
                    Ok(id) => echo.write(format!("map: requested `{zone}` (swap #{id})")),
                    Err(error) => {
                        dispatch.release();
                        echo.write(format!("map: {error}"));
                    }
                },
                _ => {
                    dispatch.release();
                    echo.write("usage: map <zone>");
                }
            },
            "disconnect" => {
                let in_session = has_world.0
                    || playback.is_some()
                    || transition.dump_id().is_some()
                    || bridge.is_some();
                if !cmd.args.is_empty() {
                    dispatch.release();
                    echo.write("usage: disconnect");
                } else if !in_session {
                    dispatch.release();
                    echo.write("disconnect: no session to leave");
                } else {
                    match transition.request_leave() {
                        Ok(id) => {
                            diag::lifecycle_boundary(
                                "disconnect_requested",
                                &format!(" swap={id}"),
                            );
                            echo.write(format!(
                                "disconnect: waiting for session teardown (swap #{id})"
                            ))
                        }
                        Err(error) => {
                            dispatch.release();
                            echo.write(format!("disconnect: {error}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn route_replay_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    identity: Option<Res<LaunchIdentity>>,
    mut recorder: ResMut<ReplaySession>,
    replay_inputs: (
        Option<Res<AuthorityWorld>>,
        Res<SimCamera>,
        Res<AuthorityInputGate>,
    ),
    mut lifecycle: (
        ResMut<replay::PendingReplayArm>,
        ResMut<session::SessionSwapRequest>,
        ResMut<ConsoleDispatch>,
    ),
    clip: (
        Res<ClipRing>,
        Res<RuntimeRole>,
        Res<net::ClientPredictionState>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
    ),
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };
    let (authority, sim_cam, input_gate) = replay_inputs;
    let (theater, map_transition, dispatch) = &mut lifecycle;
    let (ring, role, prediction, clip_clock, clip_presented) = clip;

    for cmd in events.read() {
        match cmd.name.as_str() {
            "demo" | "play" => match cmd.args.as_slice() {
                [name] => {
                    let Some(identity) = identity.as_ref() else {
                        dispatch.release();
                        echo(
                            "demo: launch identity missing (artifacts path unknown)".into(),
                            console,
                            line,
                        );
                        continue;
                    };
                    if let Some(open) = recorder.0.take() {
                        match open.stop() {
                            Ok((ticks, path)) => echo(
                                format!(
                                    "demo: stoprecord {ticks} ticks in {} before playback",
                                    path.display()
                                ),
                                console,
                                line,
                            ),
                            Err(error) => {
                                echo(format!("demo: stoprecord failed: {error}"), console, line)
                            }
                        }
                    }
                    match replay::Playback::open(&identity.artifacts, name) {
                        Ok(playback) => {
                            let Some(zone) = playback.identity().zone_name().map(str::to_owned)
                            else {
                                dispatch.release();
                                echo(
                                    format!(
                                        "demo: `{name}` has no zone in the header — re-record, or: make play {name} ZONE=<map>"
                                    ),
                                    console,
                                    line,
                                );
                                continue;
                            };
                            theater.playback = Some(playback);
                            theater.quit_on_end = false;
                            match map_transition.request_demo(name.clone(), zone.clone(), false) {
                                Ok(id) => echo(
                                    format!("demo: `{name}` zone `{zone}` (swap #{id})"),
                                    console,
                                    line,
                                ),
                                Err(error) => {
                                    theater.playback = None;
                                    dispatch.release();
                                    echo(format!("demo: {error}"), console, line);
                                }
                            }
                        }
                        Err(error) => {
                            dispatch.release();
                            echo(format!("demo: {error}"), console, line);
                        }
                    }
                }
                _ => {
                    dispatch.release();
                    echo("usage: demo <name>".into(), console, line);
                }
            },

            "record" => {
                if cmd.args.len() > 1 {
                    echo("usage: record [name]".into(), console, line);
                    continue;
                }
                if let Some(open) = recorder.0.as_ref() {
                    echo(
                        format!(
                            "record: already recording `{}` ({} ticks) — stoprecord first",
                            open.name(),
                            open.ticks()
                        ),
                        console,
                        line,
                    );
                    continue;
                }
                if !sim_cam.enabled || !input_gate.local_cmds_enabled {
                    echo(
                        "record: the simulation does not own the camera yet \
                         (no clip brushes, or the authored intermission view is active); \
                         there are no ticks to record"
                            .into(),
                        console,
                        line,
                    );
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    echo(
                        "record: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let Some(authority) = authority.as_ref() else {
                    echo(
                        "record: authority world missing (match identity unavailable)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let requested = cmd.args.first().map(String::as_str).unwrap_or("");
                let record_identity =
                    MatchRecordIdentity::from_world_on_zone(&authority.0, &identity.zone);
                match Recording::start_with_identity(
                    &identity.artifacts,
                    requested,
                    record_identity,
                ) {
                    Ok(open) => {
                        echo(
                            format!("record: writing {}", open.path().display()),
                            console,
                            line,
                        );
                        recorder.0 = Some(open);
                    }
                    Err(error) => echo(format!("record: {error}"), console, line),
                }
            }

            "stoprecord" => {
                let Some(open) = recorder.0.take() else {
                    echo("stoprecord: not recording".into(), console, line);
                    continue;
                };
                match open.stop() {
                    Ok((ticks, path)) => echo(
                        format!("stoprecord: {ticks} ticks in {}", path.display()),
                        console,
                        line,
                    ),
                    Err(error) => echo(format!("stoprecord: {error}"), console, line),
                }
            }
            "clip" => {
                if parse_clip_args(&cmd.args).is_err() {
                    echo("usage: clip".into(), console, line);
                    continue;
                }
                if ring.is_empty() {
                    echo("clip: ring empty (0 ticks)".into(), console, line);
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    echo(
                        "clip: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                match save_clip_package(
                    identity,
                    authority.as_deref().filter(|_| role.runs_authority()),
                    prediction.0.world(),
                    ring.as_ref(),
                    clip_clock.as_deref().filter(|_| role.runs_authority()),
                    clip_presented.as_deref(),
                ) {
                    Ok(saved) => {
                        echo(
                            format!(
                                "clip: {}  {:.2}s ({} ticks) of {}s",
                                saved.id,
                                saved.duration_ms as f32 / 1000.0,
                                saved.ticks,
                                CLIP_MS / 1000
                            ),
                            console,
                            line,
                        );
                        echo(
                            format!("clip: demo → {}", saved.demo_path.display()),
                            console,
                            line,
                        );
                        echo(
                            format!("clip: dump → {}", saved.dump_path.display()),
                            console,
                            line,
                        );
                    }
                    Err(error) => echo(format!("clip: {error}"), console, line),
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn route_ui_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    mut ui: (
        ResMut<UiDraw>,
        Option<Res<MenuEnabled>>,
        Option<ResMut<RetailMenuStack>>,
        Option<Res<Focus>>,
        MessageWriter<MenuShellCmd>,
        Option<Res<MenuMapList>>,
    ),
    identity: Option<Res<LaunchIdentity>>,
    (authority, authority_clock, presented): (
        Option<Res<AuthorityWorld>>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
    ),
    mut menu_requests: MessageWriter<frame::UiMenuRequest>,
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "ui" => match cmd.args.as_slice() {
                [] => echo(
                    format!("ui = {}", if ui.0.0 { 1 } else { 0 }),
                    console,
                    line,
                ),
                [value] if value == "0" || value.eq_ignore_ascii_case("off") => {
                    ui.0.0 = false;
                    echo("ui 0".into(), console, line);
                }
                [value] if value == "1" || value.eq_ignore_ascii_case("on") => {
                    ui.0.0 = true;
                    echo("ui 1".into(), console, line);
                }
                _ => echo("usage: ui [0|1]".into(), console, line),
            },

            "togglemenu" => {
                menu_requests.write(frame::UiMenuRequest::Toggle);
            }
            "openmenu" | "closemenu" => match cmd.args.as_slice() {
                [name] => {
                    menu_requests.write(if cmd.name == "openmenu" {
                        frame::UiMenuRequest::Open(name.clone())
                    } else {
                        frame::UiMenuRequest::Close(name.clone())
                    });
                }
                _ => echo(format!("usage: {} <menu>", cmd.name), console, line),
            },
            "menukey" => {
                let key = match cmd.args.first().map(|a| a.to_ascii_lowercase()).as_deref() {
                    Some("escape") => Some(frame::UiMenuKey::Escape),
                    Some("enter") => Some(frame::UiMenuKey::Enter),
                    Some("up") => Some(frame::UiMenuKey::Up),
                    Some("down") => Some(frame::UiMenuKey::Down),
                    _ => None,
                };
                match key {
                    Some(key) => {
                        menu_requests.write(frame::UiMenuRequest::Key(key));
                    }
                    None => echo("usage: menukey escape|enter|up|down".into(), console, line),
                }
            }

            "menu" => match parse_menu_args(&cmd.args) {
                Err(msg) => echo(msg, console, line),
                Ok(MenuVerb::Status) => {
                    let enabled = ui.1.as_ref().map(|e| e.0);
                    let stack = ui.2.as_ref().map(|s| s.names.join(",")).unwrap_or_default();
                    let focus =
                        ui.3.as_ref()
                            .and_then(|f| f.widget.clone())
                            .unwrap_or_else(|| "NULL".into());
                    let maps = ui.5.as_ref().map(|m| play_map_layout(&m.0));
                    echo(
                        format!(
                            "menu: enabled={} stack=[{stack}] focus={focus}{}",
                            enabled.map(|e| if e { "1" } else { "0" }).unwrap_or("NULL"),
                            match maps {
                                Some(l) => format!(
                                    " maps={} iw4={} iw5={} t5={} pages={}",
                                    l.maps_n, l.iw4_n, l.iw5_n, l.t5_n, l.pages_n
                                ),
                                None => " maps=NULL".into(),
                            }
                        ),
                        console,
                        line,
                    );
                }
                Ok(MenuVerb::Dump) => match write_current_state_dump(
                    identity.as_deref(),
                    "menu",
                    authority_clock.as_deref(),
                    authority.as_deref(),
                    presented.as_deref(),
                    None,
                ) {
                    Ok(path) => echo(
                        format!("menu dump: wrote {}", path.display()),
                        console,
                        line,
                    ),
                    Err(error) => echo(format!("menu dump: {error}"), console, line),
                },
                Ok(MenuVerb::Open(name)) => match retail_menu_name(&name) {
                    Some(target) => {
                        if let Some(stack) = ui.2.as_mut() {
                            if target == "main" {
                                stack.names.clear();
                            }
                            if stack.names.last().map(String::as_str) != Some(target) {
                                stack.names.push(target.into());
                            }
                            echo(format!("menu: open {target}"), console, line);
                        } else {
                            echo(
                                "menu open: RetailMenuStack resource missing".into(),
                                console,
                                line,
                            );
                        }
                    }
                    None => echo(
                        format!("menu open: `{name}` is not a screen yet (typed gap)"),
                        console,
                        line,
                    ),
                },
                Ok(MenuVerb::Nav(dir)) => match NavDir::parse(&dir) {
                    Some(nav) => {
                        ui.4.write(MenuShellCmd::Nav(nav));
                        echo(format!("menu: nav {dir}"), console, line);
                    }
                    None => echo("usage: menu nav up|down|left|right".into(), console, line),
                },
                Ok(MenuVerb::Accept) => {
                    ui.4.write(MenuShellCmd::Accept);
                    echo("menu: accept".into(), console, line);
                }
                Ok(MenuVerb::Back) => {
                    ui.4.write(MenuShellCmd::Back);
                    echo("menu: back".into(), console, line);
                }
                Ok(MenuVerb::Device(dev)) => echo(
                    format!("menu device {dev}: InputDevice not wired (S4) — typed gap"),
                    console,
                    line,
                ),
            },
            _ => {}
        }
    }
}

pub(crate) fn route_capture_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    identity: Option<Res<LaunchIdentity>>,
    mut capture: Option<ResMut<CaptureQueue>>,
    mut exit: MessageWriter<AppExit>,
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "screenshot" => {
                if cmd.args.len() > 1 {
                    echo("usage: screenshot [name]".into(), console, line);
                    continue;
                }
                let Some(identity) = identity.as_ref() else {
                    echo(
                        "screenshot: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let path = match screenshot_path(
                    &identity.artifacts,
                    &identity.zone,
                    cmd.args.first().map(String::as_str),
                ) {
                    Ok(path) => path,
                    Err(error) => {
                        echo(format!("screenshot: {error}"), console, line);
                        continue;
                    }
                };
                if let Some(parent) = path.parent()
                    && let Err(error) = std::fs::create_dir_all(parent)
                {
                    echo(
                        format!("screenshot: create {}: {error}", parent.display()),
                        console,
                        line,
                    );
                    continue;
                }
                let Some(queue) = capture.as_deref_mut() else {
                    echo(
                        "screenshot: render capture queue missing (no RenderPlugin)".into(),
                        console,
                        line,
                    );
                    continue;
                };

                queue.push(CaptureRequest {
                    path: path.clone(),
                    exit_after_capture: false,
                });
                echo(
                    format!(
                        "screenshot: queued {} ({} waiting)",
                        path.display(),
                        queue.pending()
                    ),
                    console,
                    line,
                );
            }

            "exit" | "quit" => {
                let (queued, writing) = capture
                    .as_deref()
                    .map(CaptureQueue::owed_at_exit)
                    .unwrap_or((0, 0));
                render::diag::capture::exit_is_user_quit();
                diag::lifecycle_boundary("quit_requested", "");
                if queued + writing > 0 {
                    echo(
                        format!(
                            "quit: leaving {queued} queued and {writing} unfinished screenshot(s) behind"
                        ),
                        console,
                        line,
                    );
                }
                exit.write(AppExit::Success);
            }

            "finish_run" => {
                diag::lifecycle_boundary("quit_requested", " via=finish_run");
                let owed_shots = capture
                    .as_deref_mut()
                    .is_some_and(CaptureQueue::exit_after_drained);
                if owed_shots {
                    echo(
                        "finish_run: waiting for queued screenshots".into(),
                        console,
                        line,
                    );
                } else {
                    exit.write(AppExit::Success);
                }
            }
            _ => {}
        }
    }
}

const LEAVE_BUDGET: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) fn exit_process(mut exit: MessageReader<AppExit>, bridge: Option<Res<MasterBridge>>) {
    let Some(code) = exit.read().last().map(|exit| match exit {
        AppExit::Success => 0,
        AppExit::Error(code) => i32::from(code.get()),
    }) else {
        return;
    };
    if let Some(bridge) = bridge {
        leave_master(&bridge);
    }
    diag::lifecycle_boundary("process_exit", &format!(" code={code}"));
    diag::flush();
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}

fn leave_master(bridge: &MasterBridge) {
    if !matches!(
        bridge.state(),
        MasterBridgeState::Hosting { .. }
            | MasterBridgeState::Joining { .. }
            | MasterBridgeState::Joined { .. }
    ) {
        return;
    }
    bridge.leave();
    let until = std::time::Instant::now() + LEAVE_BUDGET;
    while std::time::Instant::now() < until {
        if bridge.state().is_terminal() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    diag::warn!(
        Console,
        "quit: master had not confirmed the leave after {}ms",
        LEAVE_BUDGET.as_millis()
    );
}

pub(crate) fn route_state_dump_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    identity: Option<Res<LaunchIdentity>>,
    (authority, authority_clock, presented): (
        Option<Res<AuthorityWorld>>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
    ),
    (audio_ready, decisions, gaps, clips): (
        Option<Res<audio::AudioReady>>,
        Option<Res<audio::StartDecisions>>,
        Option<Res<audio::MissingAliasGaps>>,
        Option<Res<audio::ClipStore>>,
    ),
) {
    for cmd in events.read() {
        if cmd.name != "dump" {
            continue;
        }
        let name = match parse_state_dump_name(&cmd.args) {
            Ok(name) => name,
            Err(error) => {
                echo.write(format!("dump: {error}"));
                continue;
            }
        };
        let audio = audio_dump_section(
            audio_ready.as_deref(),
            decisions.as_deref(),
            gaps.as_deref(),
            clips.as_deref(),
        );
        match write_current_state_dump(
            identity.as_deref(),
            &name,
            authority_clock.as_deref(),
            authority.as_deref(),
            presented.as_deref(),
            Some(&audio),
        ) {
            Ok(path) => echo.write(format!("dump: wrote {}", path.display())),
            Err(error) => echo.write(format!("dump: {error}")),
        }
    }
}

pub(crate) fn route_hitvol_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut echo: ConsoleEcho,
    authority: Option<Res<AuthorityWorld>>,
) {
    for cmd in events.read() {
        if cmd.name != "hitvol" {
            continue;
        }
        let Some(authority) = authority.as_deref() else {
            echo.write("hitvol: no authority world on this client");
            continue;
        };
        for line in hitvol_report(&authority.0) {
            echo.write(line);
        }
    }
}

fn hitvol_report(world: &sim::SimWorld) -> Vec<String> {
    let census = world.collision_census();
    let w = &census.world;
    let p = &census.players;
    let e = &census.entities;
    let mut out = vec![
        format!(
            "hitvol world: brushes {} leaves {} leafbrushes {} meshtris {} cmodels {} smodels {} (with tris {}) pen_table {}",
            w.brushes,
            w.bsp_leaves,
            w.leafbrushes,
            w.mesh_tris,
            w.cmodels,
            w.static_models,
            w.static_models_with_tris,
            w.pen_table_loaded
        ),
        format!(
            "hitvol players: poses {} bones {} aabb-only {} bone-count {}..{} with-head-bone {}",
            p.poses, p.with_bones, p.aabb_only, p.min_bones, p.max_bones, p.with_head_bone
        ),
        format!(
            "hitvol entities: rows {} colltris {} boxes {} brush {} authored-no-collision {} not-bullet-solid {} no-dobj {} no-capability {} materialize-failed {} linked-brushes {}",
            e.rows,
            e.colltris,
            e.boxes_only,
            e.brush_only,
            e.no_collision_authored,
            e.not_bullet_solid,
            e.no_dobj,
            e.no_capability,
            e.materialize_failed,
            e.linked_brushes
        ),
    ];
    if !e.no_clip_sample.is_empty() {
        out.push(format!(
            "hitvol entities with no model clip: {}",
            e.no_clip_sample.join(", ")
        ));
    }
    if let Some(error) = &p.materialize_error {
        out.push(format!("hitvol players: materialize error {error}"));
    }
    for kit in &census.kits {
        out.push(format!(
            "hitvol kit `{}`: {} bones {} boxes {} collsurfs {} colltris {} lod {}",
            kit.key,
            kit.clip(),
            kit.bones,
            kit.bone_boxes,
            kit.coll_surfs,
            kit.coll_tris,
            kit.coll_lod
        ));
    }
    for row in world.hitvol_dump() {
        out.push(format!(
            "hitvol client {:?}: geom {} bones {} pose {} body `{}` head `{}` controller {}",
            row.client.map(|c| c.0),
            row.geom,
            row.bone_count,
            row.pose_kind,
            row.body_key,
            row.head_key,
            row.controller
        ));
    }
    out
}

pub(crate) fn route_debug_feature_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    mut bot_add: ResMut<BotAddQueue>,
    (mut bot_hold, mut bot_tp, mut bot_fire): (
        ResMut<BotHold>,
        ResMut<BotTpQueue>,
        ResMut<BotFireQueue>,
    ),
    (weapons, mut inbox, mut give_seq, roster): (
        Option<Res<assets::PreparedWeapons>>,
        Option<ResMut<ClientActionInbox>>,
        ResMut<net::ActionRequestIds>,
        Option<Res<BotRoster>>,
    ),
    (mut hurt, mut pending_splash): (ResMut<PendingViewHurt>, ResMut<PendingSplash>),
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    for cmd in events.read() {
        match cmd.name.as_str() {
            "bot" => match parse_bot_args(&cmd.args) {
                Err(msg) => echo(msg, console, line),
                Ok(BotVerb::Add(n)) => {
                    bot_add.push(n);
                    echo(format!("bot: queued add {n}"), console, line);
                }
                Ok(BotVerb::Dummy(n)) => {
                    bot_add.push_dummy(n);
                    echo(format!("bot: queued dummy {n}"), console, line);
                }
                Ok(BotVerb::Hold(on)) => {
                    bot_hold.0 = on;
                    echo(
                        format!("bot: hold {}", if on { "on" } else { "off" }),
                        console,
                        line,
                    );
                }
                Ok(BotVerb::Tp(request)) => {
                    bot_tp.push(request);
                    echo("bot: queued tp".into(), console, line);
                }
                Ok(BotVerb::Fire(target)) => {
                    bot_fire.push(target);
                    echo("bot: queued fire".into(), console, line);
                }
                Ok(BotVerb::Give {
                    id,
                    weapon,
                    attachments,
                }) => {
                    let Some(weapons) = weapons.as_ref() else {
                        echo("bot give: weapon catalog not loaded".into(), console, line);
                        continue;
                    };
                    let Some(inbox) = inbox.as_mut() else {
                        echo("bot give: no action inbox".into(), console, line);
                        continue;
                    };
                    if !roster.as_ref().is_some_and(|r| r.is_bot(id)) {
                        echo(format!("bot give: {id:?} is not a bot"), console, line);
                        continue;
                    }
                    match crate::weapon_dispatch::resolve_give_id(&weapons.0, &weapon, &attachments) {
                        Ok(weapon_id) => {
                            let request_id = give_seq.allocate();
                            if let Err(error) = inbox.push(
                                id,
                                ClientAction::GiveWeapon {
                                    request_id,
                                    weapon: weapon_id,
                                },
                            ) {
                                echo(format!("bot give: {error}"), console, line);
                                continue;
                            }
                            echo(
                                format!(
                                    "bot give: queued {} id={weapon_id} on {} request_id={request_id}",
                                    weapons.0.configuration_label(weapon_id),
                                    id.0
                                ),
                                console,
                                line,
                            );
                        }
                        Err(msg) => echo(format!("bot give: {msg}"), console, line),
                    }
                }
            },
            "hurt" => {
                hurt.0 = hurt.0.saturating_add(1);
                echo(
                    "hurt: queued undirected view punch (255/255 count=1)".into(),
                    console,
                    line,
                );
            }
            "splash" => match cmd.args.as_slice() {
                [] => echo(
                    "usage: splash <key> [optionalNumber] — CG_ActivateSplash slot 0 (mp/splashTable.csv)".into(),
                    console,
                    line,
                ),
                [key, rest @ ..] => {
                    let optional = rest
                        .first()
                        .and_then(|s| s.parse::<i32>().ok())
                        .unwrap_or(0);
                    pending_splash.key = Some(key.clone());
                    pending_splash.optional_number = optional;
                    echo(
                        format!("splash: queued `{key}` optional={optional} (CG_ActivateSplash slot 0)"),
                        console,
                        line,
                    );
                }
            },

            _ => {}
        }
    }
}

pub(crate) fn resume_lifecycle_commands(
    mut dispatch: ResMut<ConsoleDispatch>,
    mut transition: ResMut<session::SessionSwapRequest>,
    mut echo: ConsoleEcho,
) {
    if let Some(completed) = transition.take_completed() {
        match completed.result {
            session::SessionSwapResult::Failed { zone, error } => {
                dispatch.release();
                echo.write(format!("map: `{zone}` failed: {error}"));
            }
            _ => dispatch.paused = false,
        }
    }
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct DebugPosOverlay(pub bool);

fn screenshot_path(artifacts: &Path, zone: &str, name: Option<&str>) -> Result<PathBuf, String> {
    let name = name.unwrap_or(zone);
    let relative = Path::new(name);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("name must be relative to iw4l-artifacts/screenshots".into());
    }
    if let Some(extension) = relative.extension()
        && !extension.to_string_lossy().eq_ignore_ascii_case("png")
    {
        return Err("name must use a .png extension".into());
    }
    let mut path = artifacts.join("screenshots").join(relative);
    if path.extension().is_none() {
        path.set_extension("png");
    }
    Ok(path)
}

fn parse_clip_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("usage: clip".into())
    }
}

struct SavedClip {
    id: String,
    ticks: u64,
    duration_ms: u32,
    demo_path: PathBuf,
    dump_path: PathBuf,
}

fn allocate_clip_dir(artifacts: &Path) -> Result<(String, PathBuf), String> {
    for _ in 0..8 {
        let id = replay::new_ulid().map_err(|error| error.to_string())?;
        match replay::create_clip_dir(artifacts, &id) {
            Ok(dir) => return Ok((id, dir)),
            Err(replay::ReplayError::Io(error)) if error.kind() == ErrorKind::AlreadyExists => {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("could not allocate a unique ULID directory".into())
}

fn save_clip_package(
    identity: &LaunchIdentity,
    world: Option<&AuthorityWorld>,
    prediction: &sim::SimWorld,
    ring: &ClipRing,
    authority_clock: Option<&AuthorityClock>,
    presented: Option<&PresentedSnapshot>,
) -> Result<SavedClip, String> {
    let (id, dir) = allocate_clip_dir(&identity.artifacts)?;
    let captured_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_nanos();
    let result = (|| {
        let demo_path = dir.join(CLIP_DEMO_FILE);
        let dump_path = dir.join(CLIP_DUMP_FILE);
        let record_identity = MatchRecordIdentity::from_world_on_zone(
            world.map(|world| &world.0).unwrap_or(prediction),
            &identity.zone,
        );
        let mut recording = Recording::start_at_path(demo_path.clone(), record_identity)
            .map_err(|error| error.to_string())?;
        recording
            .record_clip_ring(ring)
            .map_err(|error| error.to_string())?;
        let (ticks, _) = recording.stop().map_err(|error| error.to_string())?;
        let body = state_dump_body(
            identity,
            captured_unix_ns,
            authority_clock,
            world,
            presented,
            None,
        );
        persist_bytes_atomic(&dump_path, &body)?;
        let mut manifest = replay::clip_manifest(
            &id,
            ticks,
            ring.duration_ms(),
            &identity.zone,
            captured_unix_ns,
        );
        let source = if world.is_some() {
            "authority"
        } else {
            "received"
        };
        manifest.push_str(&format!(
            "source = {source:?}\nrole = {:?}\n",
            identity.role_label
        ));
        persist_bytes_atomic(&dir.join(CLIP_MANIFEST_FILE), &manifest)?;
        replay::rewrite_latest_symlink(&identity.artifacts, &id)
            .map_err(|error| error.to_string())?;
        Ok(SavedClip {
            id,
            ticks,
            duration_ms: ring.duration_ms(),
            demo_path,
            dump_path,
        })
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

fn parse_state_dump_name(args: &[String]) -> Result<String, String> {
    let raw = match args {
        [] => "snapshot",
        [name] => name.strip_suffix(".txt").unwrap_or(name),
        _ => return Err("usage: dump [name]".into()),
    };
    if raw.is_empty()
        || raw.len() > 96
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        || matches!(raw, "." | "..")
    {
        return Err("name must be 1-96 ASCII letters, digits, '.', '-' or '_'".into());
    }
    Ok(raw.to_owned())
}

fn write_current_state_dump(
    identity: Option<&LaunchIdentity>,
    name: &str,
    authority_clock: Option<&AuthorityClock>,
    authority: Option<&AuthorityWorld>,
    presented: Option<&PresentedSnapshot>,
    audio: Option<&str>,
) -> Result<PathBuf, String> {
    let identity = identity.ok_or("launch identity missing (artifacts path unknown)")?;
    let captured_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_nanos();
    let body = state_dump_body(
        identity,
        captured_unix_ns,
        authority_clock,
        authority,
        presented,
        audio,
    );
    persist_state_dump(&identity.artifacts, name, captured_unix_ns, &body)
}

fn audio_dump_section(
    ready: Option<&audio::AudioReady>,
    decisions: Option<&audio::StartDecisions>,
    gaps: Option<&audio::MissingAliasGaps>,
    clips: Option<&audio::ClipStore>,
) -> String {
    let ready = ready.is_some_and(|r| r.0);
    let late = clips.map(audio::ClipStore::late_prepares).unwrap_or(0);
    let mut out = format!("[audio]\nAudioReady = {ready}\nlate_prepares = {late}\n");
    match gaps {
        Some(gaps) if !gaps.is_empty() => {
            out.push_str("missing_aliases =\n");
            for alias in &gaps.aliases {
                out.push_str("  ");
                out.push_str(alias);
                out.push('\n');
            }
        }
        _ => out.push_str("missing_aliases = []\n"),
    }
    out.push_str("starts =\n");
    match decisions {
        Some(decisions) => {
            let mut n = 0usize;
            for line in decisions.lines() {
                n += 1;
                out.push_str("  ");
                out.push_str(&line);
                out.push('\n');
            }
            if n == 0 {
                out.push_str("  (none)\n");
            }
        }
        None => out.push_str("  Unavailable { reason: \"StartDecisions resource absent\" }\n"),
    }
    out
}

fn state_dump_body(
    identity: &LaunchIdentity,
    captured_unix_ns: u128,
    authority_clock: Option<&AuthorityClock>,
    authority: Option<&AuthorityWorld>,
    presented: Option<&PresentedSnapshot>,
    audio: Option<&str>,
) -> String {
    let authority_snapshot = match (authority_clock, authority) {
        (Some(clock), Some(world)) => format!("{:#?}", world.0.snapshot(sim::Tick(clock.tick))),
        (None, Some(_)) => "Unavailable { reason: \"AuthorityClock resource absent\" }".to_owned(),
        (Some(_), None) => "Unavailable { reason: \"AuthorityWorld resource absent\" }".to_owned(),
        (None, None) => {
            "Unavailable { reason: \"AuthorityClock and AuthorityWorld resources absent\" }"
                .to_owned()
        }
    };
    let presented_snapshot = presented
        .map(|snapshot| format!("{snapshot:#?}"))
        .unwrap_or_else(|| {
            "Unavailable { reason: \"PresentedSnapshot resource absent\" }".to_owned()
        });
    let audio =
        audio.unwrap_or("[audio]\nUnavailable { reason: \"not captured with this dump\" }\n");
    let hitvol = match authority {
        Some(world) => {
            let mut out = String::from("[hitvol]\n");
            for line in hitvol_report(&world.0) {
                out.push_str(&line);
                out.push('\n');
            }
            out.push_str("rows =\n");
            for row in world.0.hitvol_dump() {
                out.push_str(&format!("  {row:?}\n"));
            }
            out
        }
        None => "[hitvol]\nUnavailable { reason: \"AuthorityWorld resource absent\" }\n".to_owned(),
    };
    format!(
        "format = \"iw4l-state-dump-1\"\n\
         captured_unix_ns = {captured_unix_ns}\n\
         role = {:?}\n\
         zone = {:?}\n\
         authority_clock = {authority_clock:#?}\n\
         \n[authority_snapshot]\n{authority_snapshot}\n\
         \n[presented_snapshot]\n{presented_snapshot}\n\
         \n{hitvol}\n{audio}",
        identity.role_label, identity.zone,
    )
}

fn persist_state_dump(
    artifacts: &Path,
    name: &str,
    captured_unix_ns: u128,
    body: &str,
) -> Result<PathBuf, String> {
    let directory = artifacts.join("dumps");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let file_name = format!("{captured_unix_ns}-{name}.txt");
    let path = directory.join(&file_name);
    persist_bytes_atomic(&path, body)?;
    Ok(path)
}

fn persist_bytes_atomic(path: &Path, body: &str) -> Result<(), String> {
    let directory = path
        .parent()
        .ok_or_else(|| format!("{} has no parent", path.display()))?;
    std::fs::create_dir_all(directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let file_name = path
        .file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))?
        .to_string_lossy();
    let temporary = directory.join(format!(".{file_name}.tmp"));
    let result = (|| -> Result<(), String> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        file.write_all(body.as_bytes())
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.flush()
            .map_err(|error| format!("flush {}: {error}", temporary.display()))?;
        drop(file);
        std::fs::hard_link(&temporary, path).map_err(|error| {
            format!(
                "link {} to {}: {error}",
                temporary.display(),
                path.display()
            )
        })?;
        std::fs::remove_file(&temporary)
            .map_err(|error| format!("remove {}: {error}", temporary.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn register_feature_commands(registry: &mut crate::ConsoleRegistry, maps: &[String]) {
    if registry.resolve("map").is_none() {
        registry.register(
            crate::CommandSpec::new("map")
                .usage("map <zone> — tear down the current occupancy, then load a zone")
                .arg(crate::StaticCompleter::new(maps.iter().cloned())),
        );
    }
    for (name, usage) in [
        ("help", "help — list registered commands"),
        ("clear", "clear — clear console scrollback"),
        ("screenshot", "screenshot [name] — capture the backbuffer"),
        (
            "record",
            "record [name] — start a demo under iw4l-artifacts",
        ),
        ("stoprecord", "stoprecord — finish the open demo"),
        (
            "clip",
            "clip — save last ≤45s available to this client as iw4l-artifacts/clips/<ULID>/{clip.iw4ldemo, dump.txt} (ours; always-on ring; not a retail command)",
        ),
        (
            "map_restart",
            "map_restart — a new match on the current map, on what the last one prepared",
        ),
        (
            "disconnect",
            "disconnect — leave the session: tear the world down, leave the room, back to the main menu",
        ),
        (
            "demo",
            "demo <name> — tear down the current occupancy, then play iw4l-artifacts/demos/<name>.iw4ldemo or clips/<name>/clip.iw4ldemo",
        ),
        (
            "play",
            "play <name> — launcher alias of demo (not a retail command string)",
        ),
        (
            "exit",
            "exit — quit the process, abandoning unfinished screenshots",
        ),
        (
            "quit",
            "quit — quit the process, abandoning unfinished screenshots",
        ),
        (
            "finish_run",
            "finish_run — finish the run's screenshots, then quit (not a retail command string)",
        ),
        ("ui", "ui [0|1] — hide/show game UI; console Overlay stays"),
        (
            "togglemenu",
            "togglemenu — open the script main menu (g_scriptMainMenu), or escape the top menu",
        ),
        ("openmenu", "openmenu <menu> — open an in-game menuDef"),
        (
            "closemenu",
            "closemenu <menu> — close an open in-game menuDef",
        ),
        (
            "menukey",
            "menukey escape|enter|up|down — a key to the top in-game menu (not a retail command string)",
        ),
        (
            "dump",
            "dump [name] - atomically write the current authority + presented state to iw4l-artifacts/dumps/<timestamp>-<name>.txt (one shot; no history or timing)",
        ),
        (
            "hitvol",
            "hitvol — what the authority holds for a bullet to clip against: world tables, live player volumes, script-model clips, kit models",
        ),
        (
            "bot",
            "bot add [N] | dummy [N] | hold [on|off] | give <id> <weapon> [att...] | fire [all|<id>] | tp all|<id> above <h> | tp all|<id> <x> <y> <z>",
        ),
        (
            "menu",
            "menu [open <screen> | nav up|down|left|right | accept | back | device pad|mouse | dump] — shell surface; back/device remain typed gaps (S2/S4)",
        ),
        (
            "hurt",
            "hurt — stamp one undirected CG_DamageFeedback punch (listen-host experiment)",
        ),
        (
            "splash",
            "splash <key> [optionalNumber] — CG_ActivateSplash slot 0 from mp/splashTable.csv (one_shot_kill, longshot, capture, …)",
        ),
        (
            "wait",
            "wait [seconds|<n>t|world|spawn|torn|ambient] — pause the console FIFO; <n>t = n authority ticks; world = scene.spawned; spawn = AppScreen::InGame; torn = HasWorld false and scene.spawned false (hold after MatchTornDown); ambient = MapAmbientBooted (overlay finished, CreateFX loops spawned)",
        ),
    ] {
        if registry.resolve(name).is_none() {
            registry.register(crate::CommandSpec::new(name).usage(usage));
        }
    }
}

const BOT_USAGE: &str = "usage: bot add [N] | dummy [N] | hold [on|off] | give <id> <weapon> [att...] | fire [all|<id>] | tp all|<id> above <h> | tp all|<id> <x> <y> <z> [yaw] [pitch]";

#[derive(Debug, PartialEq)]
pub(crate) enum BotVerb {
    Add(u32),
    Dummy(u32),
    Hold(bool),
    Give {
        id: ClientId,
        weapon: String,
        attachments: Vec<String>,
    },
    Fire(BotTpTarget),
    Tp(BotTpRequest),
}

pub(crate) fn parse_bot_args(args: &[String]) -> Result<BotVerb, String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "add" => {
            let n = args
                .get(1)
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 16);
            Ok(BotVerb::Add(n))
        }
        "dummy" => {
            if args.len() > 2 {
                return Err("usage: bot dummy [N]".into());
            }
            let n = args
                .get(1)
                .map(|s| s.parse::<u32>())
                .transpose()
                .map_err(|_| "usage: bot dummy [N]".to_owned())?
                .unwrap_or(1);
            if !(1..=16).contains(&n) {
                return Err("bot dummy: count must be 1..16".into());
            }
            Ok(BotVerb::Dummy(n))
        }
        "hold" => match args.get(1).map(String::as_str) {
            None | Some("on") | Some("1") => Ok(BotVerb::Hold(true)),
            Some("off") | Some("0") => Ok(BotVerb::Hold(false)),
            Some(other) => Err(format!("usage: bot hold [on|off] (got `{other}`)")),
        },
        "give" => {
            let usage = || "usage: bot give <id> <weapon> [attachment...]".to_owned();
            let id = args
                .get(1)
                .ok_or_else(usage)?
                .parse::<u32>()
                .map_err(|_| usage())?;
            let weapon = args.get(2).cloned().ok_or_else(usage)?;
            Ok(BotVerb::Give {
                id: ClientId(id),
                weapon,
                attachments: args[3..].to_vec(),
            })
        }
        "fire" => match args.get(1).map(String::as_str) {
            None | Some("all") => Ok(BotVerb::Fire(BotTpTarget::All)),
            Some(s) => {
                let id = s
                    .parse::<u32>()
                    .map_err(|_| "usage: bot fire [all|<id>]".to_owned())?;
                Ok(BotVerb::Fire(BotTpTarget::Id(ClientId(id))))
            }
        },
        "tp" => parse_bot_tp(&args[1..]),
        _ => Err(BOT_USAGE.into()),
    }
}

fn parse_bot_tp(args: &[String]) -> Result<BotVerb, String> {
    let target = match args.first().map(String::as_str) {
        Some("all") => BotTpTarget::All,
        Some(s) => {
            let id = s.parse::<u32>().map_err(|_| BOT_USAGE.to_owned())?;
            BotTpTarget::Id(sim::ClientId(id))
        }
        None => return Err(BOT_USAGE.into()),
    };
    let rest = &args[1..];
    if rest.first().map(String::as_str) == Some("above") {
        let height = rest
            .get(1)
            .ok_or_else(|| "usage: bot tp all|<id> above <h>".to_owned())
            .and_then(parse_finite)?;
        return Ok(BotVerb::Tp(BotTpRequest {
            target,
            where_: BotTpWhere::Above { height },
        }));
    }
    if rest.len() < 3 || rest.len() > 5 {
        return Err(BOT_USAGE.into());
    }
    let origin = [
        parse_finite(&rest[0])?,
        parse_finite(&rest[1])?,
        parse_finite(&rest[2])?,
    ];
    let yaw = rest.get(3).map(parse_finite).transpose()?;
    let pitch = rest.get(4).map(parse_finite).transpose()?;
    Ok(BotVerb::Tp(BotTpRequest {
        target,
        where_: BotTpWhere::Absolute { origin, yaw, pitch },
    }))
}

fn parse_finite(s: &String) -> Result<f32, String> {
    s.parse::<f32>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| format!("bot: not a finite number `{s}`"))
}

const MENU_USAGE: &str = "usage: menu [open <screen> | nav up|down|left|right | accept | back | device pad|mouse | dump]";

#[derive(Debug, PartialEq)]
enum MenuVerb {
    Status,
    Dump,
    Open(String),
    Nav(String),
    Accept,
    Back,
    Device(String),
}

fn parse_menu_args(args: &[String]) -> Result<MenuVerb, String> {
    let sub = args.first().map(String::as_str).unwrap_or("");
    match sub {
        "" => Ok(MenuVerb::Status),
        "dump" => {
            if args.len() != 1 {
                return Err("usage: menu dump".into());
            }
            Ok(MenuVerb::Dump)
        }
        "open" => {
            let name = args
                .get(1)
                .cloned()
                .ok_or_else(|| "usage: menu open <screen>".to_owned())?;
            if args.len() != 2 {
                return Err("usage: menu open <screen>".into());
            }
            Ok(MenuVerb::Open(name))
        }
        "nav" => {
            let dir = args.get(1).map(String::as_str).unwrap_or("");
            match dir {
                "up" | "down" | "left" | "right" if args.len() == 2 => {
                    Ok(MenuVerb::Nav(dir.to_owned()))
                }
                _ => Err("usage: menu nav up|down|left|right".into()),
            }
        }
        "accept" if args.len() == 1 => Ok(MenuVerb::Accept),
        "back" if args.len() == 1 => Ok(MenuVerb::Back),
        "device" => match args.get(1).map(String::as_str) {
            Some("pad") | Some("mouse") if args.len() == 2 => Ok(MenuVerb::Device(args[1].clone())),
            _ => Err("usage: menu device pad|mouse".into()),
        },
        "accept" | "back" => Err(MENU_USAGE.into()),
        _ => Err(MENU_USAGE.into()),
    }
}

fn retail_menu_name(name: &str) -> Option<&str> {
    match name {
        "main" => Some("main"),
        "maps" | "map_setup" | "mapselect" | "play" => Some("map_setup"),
        "settings" | "options" => Some("options"),
        "online" => Some("find_lobbies"),
        "classes" | "class_setup" | "cac" | "classsetup" => Some("class_setup"),
        "game_mode_select" | "game_map_select" | "game_lobby" | "find_lobbies"
        | "lobby_game_setup" => Some(name),
        _ => None,
    }
}
