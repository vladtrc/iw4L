use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use ::replay::{
    CLIP_DEMO_FILE, CLIP_DUMP_FILE, CLIP_MANIFEST_FILE, ClipRing, MatchRecordIdentity, Recording,
    ReplaySession,
};
use bevy::prelude::*;
use frame::{LaunchIdentity, RuntimeRole};
use net::{AuthorityClock, AuthorityWorld, PresentedSnapshot};

use crate::{ConsoleCommand, ConsoleDispatch, ConsoleLine, ConsoleSettings, ConsoleState};

use super::state_dump::{
    audio_dump_section, persist_bytes_atomic, portal_dump_section, state_dump_body,
};

#[expect(
    clippy::type_complexity,
    clippy::too_many_arguments,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
pub(crate) fn route_replay_commands(
    mut events: MessageReader<ConsoleCommand>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
    (identity, merged): (Option<Res<LaunchIdentity>>, Option<Res<assets::MergedMap>>),
    mut recorder: ResMut<ReplaySession>,
    audio: (
        Option<Res<audio::AudioReady>>,
        Option<Res<audio::StartDecisions>>,
        Option<Res<audio::MissingAliasGaps>>,
        Option<Res<audio::ClipStore>>,
        Option<Res<audio::AudioRuntime>>,
    ),
    authority: Option<Res<AuthorityWorld>>,
    mut lifecycle: (
        ResMut<::replay::PendingReplayArm>,
        ResMut<::session::SessionSwapRequest>,
        ResMut<ConsoleDispatch>,
    ),
    clip: (
        Option<Res<ClipRing>>,
        Res<RuntimeRole>,
        Option<Res<net::ClientLevel>>,
        Option<Res<AuthorityClock>>,
        Option<Res<PresentedSnapshot>>,
        Option<Res<net::LocalCommandCapture>>,
    ),
    mut commands: Commands,
    portal: (
        Option<Res<render_frontend::assemble::drawsurf::frame_products::MaterialFrameInputs>>,
        Option<Res<render_frontend::prepare::scene::world::WorldScene>>,
        Option<Res<render::diag::render_frame_diag::RenderFrameDiag>>,
        Option<Res<bevy::render::renderer::RenderDevice>>,
        Option<Res<render_frontend::assemble::drawsurf::frame_products::RenderFrameProducts>>,
    ),
) {
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };
    let (theater, map_transition, dispatch) = &mut lifecycle;
    let (ring, role, level, clip_clock, clip_presented, clip_inputs) = clip;

    for cmd in events.read() {
        match cmd.name.as_str() {
            "demo" | "play" => match cmd.args.as_slice() {
                [name] => {
                    match arm_playback(
                        identity.as_deref(),
                        &mut recorder,
                        theater,
                        map_transition,
                        name,
                        false,
                    ) {
                        Ok(PlaybackArmed { zone, stopped }) => {
                            if let Some(stopped) = stopped {
                                echo(format!("demo: {stopped}"), console, line);
                            }
                            theater.control = None;
                            echo(format!("demo: `{name}` zone `{zone}`"), console, line);
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

            "render" => {
                let (name, fps) = match super::film::parse_render_args(&cmd.args) {
                    Ok(parsed) => parsed,
                    Err(usage) => {
                        echo(usage, console, line);
                        continue;
                    }
                };
                let Some(artifacts) = identity.as_ref().map(|identity| &identity.artifacts) else {
                    echo(
                        "render: launch identity missing (artifacts path unknown)".into(),
                        console,
                        line,
                    );
                    continue;
                };
                let film = match super::film::FilmRender::prepare(artifacts, name, fps) {
                    Ok(film) => film,
                    Err(error) => {
                        echo(format!("render: {error}"), console, line);
                        continue;
                    }
                };
                match arm_playback(
                    identity.as_deref(),
                    &mut recorder,
                    theater,
                    map_transition,
                    name,
                    true,
                ) {
                    Ok(PlaybackArmed { zone, stopped }) => {
                        if let Some(stopped) = stopped {
                            echo(format!("render: {stopped}"), console, line);
                        }
                        // Held on the first tick until the frames settle, so
                        // the film starts at the recording's start.
                        theater.control = Some(::replay::PlaybackControl {
                            paused: true,
                            ..Default::default()
                        });
                        echo(
                            format!(
                                "render: `{name}` zone `{zone}` → {} (quits when written)",
                                film.output().display()
                            ),
                            console,
                            line,
                        );
                        commands.insert_resource(film);
                    }
                    Err(error) => echo(format!("render: {error}"), console, line),
                }
            }

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
                if *role == RuntimeRole::Replay || ring.as_ref().is_none_or(|ring| ring.is_empty())
                {
                    echo(
                        "record: no live match ticks to record".into(),
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
                // A merged map is played back by its name, which loads its
                // other worlds too.
                let zone = merged
                    .as_ref()
                    .map_or(identity.zone.as_str(), |merged| merged.recording_name());
                let Some(record_identity) =
                    record_identity(authority.as_deref(), level.as_deref(), zone)
                else {
                    echo("record: match identity unavailable".into(), console, line);
                    continue;
                };
                let requested = cmd.args.first().map(String::as_str).unwrap_or("");
                match Recording::start_with_identity(
                    &identity.artifacts,
                    requested,
                    record_identity,
                ) {
                    Ok(open) => {
                        if let Some(merged) = merged.as_ref()
                            && let Err(error) = merged.save_recording_layout(
                                open.path(),
                                record_identity.zone_name().unwrap_or(zone),
                            )
                        {
                            echo(
                                format!("record: layout save failed: {error}"),
                                console,
                                line,
                            );
                            continue;
                        }
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
                let (Some(ring), Some(level)) = (ring.as_ref(), level.as_ref()) else {
                    echo("clip: no live match".into(), console, line);
                    continue;
                };
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
                let mut audio_section = audio_dump_section(
                    audio.0.as_deref(),
                    audio.1.as_deref(),
                    audio.2.as_deref(),
                    audio.3.as_deref(),
                    audio.4.as_deref(),
                );
                audio_section.push_str(&portal_dump_section(
                    merged.as_deref(),
                    portal.0.as_deref(),
                    portal.1.as_deref(),
                    portal.2.as_deref(),
                    portal.3.as_deref(),
                    portal.4.as_deref(),
                ));
                match save_clip_package(
                    identity,
                    merged.as_deref(),
                    *role,
                    authority.as_deref().filter(|_| role.runs_authority()),
                    level,
                    (ring.as_ref(), clip_inputs.as_deref()),
                    |captured_unix_ns| {
                        state_dump_body(
                            identity,
                            captured_unix_ns,
                            clip_clock.as_deref().filter(|_| role.runs_authority()),
                            authority.as_deref().filter(|_| role.runs_authority()),
                            clip_presented.as_deref(),
                            Some(&audio_section),
                        )
                    },
                ) {
                    Ok(id) => echo(format!("clip: {id}"), console, line),
                    Err(error) => echo(format!("clip: {error}"), console, line),
                }
            }
            _ => {}
        }
    }
}

struct PlaybackArmed {
    zone: String,
    /// The recording that was open and is finished first.
    stopped: Option<String>,
}

/// Finish any open recording, open `name` and request the swap into it.
fn arm_playback(
    identity: Option<&LaunchIdentity>,
    recorder: &mut ReplaySession,
    theater: &mut ::replay::PendingReplayArm,
    map_transition: &mut ::session::SessionSwapRequest,
    name: &str,
    quit_on_end: bool,
) -> Result<PlaybackArmed, String> {
    let identity =
        identity.ok_or_else(|| "launch identity missing (artifacts path unknown)".to_owned())?;
    let stopped = recorder.0.take().map(|open| match open.stop() {
        Ok((ticks, path)) => format!(
            "stoprecord {ticks} ticks in {} before playback",
            path.display()
        ),
        Err(error) => format!("stoprecord failed: {error}"),
    });
    let playback =
        ::replay::Playback::open(&identity.artifacts, name).map_err(|error| error.to_string())?;
    let zone = playback
        .identity()
        .zone_name()
        .map(str::to_owned)
        .ok_or_else(|| {
            format!(
                "`{name}` has no zone in the header — re-record, or: make play {name} ZONE=<map>"
            )
        })?;
    assets::MergedMap::install_recording_layout(&identity.artifacts, playback.path(), &zone)?;
    theater.playback = Some(playback);
    theater.quit_on_end = quit_on_end;
    theater.in_place = false;
    if let Err(error) = map_transition.request_demo(name.to_owned(), zone.clone(), quit_on_end) {
        theater.playback = None;
        return Err(error);
    }
    Ok(PlaybackArmed { zone, stopped })
}

fn parse_clip_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err("usage: clip".into())
    }
}

fn allocate_clip_dir(artifacts: &Path) -> Result<(String, PathBuf), String> {
    for _ in 0..8 {
        let id = ::replay::new_ulid().map_err(|error| error.to_string())?;
        match ::replay::create_clip_dir(artifacts, &id) {
            Ok(dir) => return Ok((id, dir)),
            Err(::replay::ReplayError::Io(error)) if error.kind() == ErrorKind::AlreadyExists => {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("could not allocate a unique ULID directory".into())
}

fn save_clip_package(
    identity: &LaunchIdentity,
    merged: Option<&assets::MergedMap>,
    role: RuntimeRole,
    world: Option<&AuthorityWorld>,
    level: &net::ClientLevel,
    (ring, inputs): (&ClipRing, Option<&net::LocalCommandCapture>),
    capture_dump: impl FnOnce(u128) -> String,
) -> Result<String, String> {
    let recording_zone = merged.map_or(identity.zone.as_str(), |merged| merged.recording_name());
    let (id, dir) = allocate_clip_dir(&identity.artifacts)?;
    let captured_unix_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_nanos();
    let result = (|| {
        let demo_path = dir.join(CLIP_DEMO_FILE);
        let dump_path = dir.join(CLIP_DUMP_FILE);
        let record_identity =
            record_identity(world, Some(level), recording_zone).expect("clip has a client level");
        if let Some(merged) = merged {
            merged.save_recording_layout(
                &demo_path,
                record_identity.zone_name().unwrap_or(recording_zone),
            )?;
        }
        let mut recording = Recording::start_at_path(demo_path.clone(), record_identity)
            .map_err(|error| error.to_string())?;
        recording
            .record_clip_ring(ring)
            .map_err(|error| error.to_string())?;
        let (ticks, _) = recording.stop().map_err(|error| error.to_string())?;
        let body = capture_dump(captured_unix_ns);
        persist_bytes_atomic(&dump_path, &body)?;
        persist_bytes_atomic(
            &dir.join(super::clip_inputs::FILE_NAME),
            &super::clip_inputs::export(ring, inputs)?,
        )?;
        let mut manifest = ::replay::clip_manifest(
            &id,
            ticks,
            ring.duration_ms(),
            recording_zone,
            captured_unix_ns,
        );
        let source = if role == RuntimeRole::Dedicated {
            "authority"
        } else {
            "received"
        };
        manifest.push_str(&format!(
            "source = {source:?}\nrole = {:?}\nusercmds = {:?}\n",
            identity.role_label,
            super::clip_inputs::FILE_NAME
        ));
        persist_bytes_atomic(&dir.join(CLIP_MANIFEST_FILE), &manifest)?;
        ::replay::rewrite_latest_symlink(&identity.artifacts, &id)
            .map_err(|error| error.to_string())?;
        Ok(id)
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

fn record_identity(
    world: Option<&AuthorityWorld>,
    level: Option<&net::ClientLevel>,
    zone: &str,
) -> Option<MatchRecordIdentity> {
    if let Some(world) = world {
        return Some(MatchRecordIdentity::from_world_on_zone(&world.0, zone));
    }
    let level = level?;
    let mut identity = MatchRecordIdentity {
        rng_scheme: sim::RNG_DOMAIN_SCHEME,
        root_seed: level.root_seed,
        content_digest: level.content_digest,
        ..Default::default()
    };
    identity.set_zone(zone);
    Some(identity)
}
