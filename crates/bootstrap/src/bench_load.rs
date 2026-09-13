use std::path::{Path, PathBuf};
use std::time::Instant;

use assets::{LoadingScreen, MatchLoadBusy, MatchLoadRequest};
use audio::MapAmbientBooted;
use bevy::prelude::*;
use render::diag::capture::{CaptureQueue, CaptureRequest};
use render_frontend::prepare::scene::cull::DpvsFrameStats;
use render_frontend::prepare::scene::world::WorldScene;
use session::MatchInstalled;

static COMMAND_START: std::sync::OnceLock<(Instant, &'static str)> = std::sync::OnceLock::new();

pub fn begin_command() {
    if !enabled() {
        return;
    }
    let now = Instant::now();
    let clock = match std::env::var("IW4L_BENCH_LOAD_STARTED_NS") {
        Ok(stamp) => {
            let elapsed = stamp
                .parse::<u128>()
                .ok()
                .and_then(|stamp| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()?
                        .as_nanos()
                        .checked_sub(stamp)
                })
                .and_then(|ns| u64::try_from(ns).ok());
            elapsed
                .and_then(|ns| now.checked_sub(std::time::Duration::from_nanos(ns)))
                .map(|at| (at, "make"))
        }
        Err(std::env::VarError::NotPresent) => Some((now, "process")),
        Err(_) => None,
    };
    if let Some(clock) = clock {
        let _ = COMMAND_START.set(clock);
    } else {
        eprintln!("bench-load: invalid command timestamp; command timings will be MISS");
    }
}

pub fn enabled() -> bool {
    match std::env::var("IW4L_BENCH_LOAD") {
        Ok(value)
            if value.is_empty()
                || value == "0"
                || value.eq_ignore_ascii_case("false")
                || value.eq_ignore_ascii_case("off") =>
        {
            false
        }
        Ok(_) => true,
        Err(_) => false,
    }
}

pub fn screenshot_path(artifacts: &Path, zone: &str) -> PathBuf {
    artifacts
        .join("screenshots")
        .join("bench-load")
        .join(format!("{}.png", sanitize_zone(zone)))
}

fn sanitize_zone(zone: &str) -> String {
    let mut out = String::with_capacity(zone.len());
    for ch in zone.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_start_matches('.');
    if trimmed.is_empty() {
        "unknown".to_owned()
    } else {
        trimmed.to_owned()
    }
}

struct BenchLoad {
    zone: String,
    path: PathBuf,
    t0: Instant,
    request: Option<Instant>,
    installed: Option<Instant>,
    spawned: Option<Instant>,
    overlay_down: Option<Instant>,
    overlay_seen: bool,
    ingame: Option<Instant>,
    rendered_spawn: Option<Instant>,
    ready_frames: u32,
    ambient: Option<Instant>,
    shot: Option<Instant>,
    batches_at_shot: Option<u32>,
    g0_at_shot: Option<u32>,
    last_batches: Option<u32>,
    last_g0: Option<u32>,
    last_spawned: bool,
}

static BENCH: std::sync::Mutex<Option<BenchLoad>> = std::sync::Mutex::new(None);

fn bench_mut(fill: impl FnOnce(&mut BenchLoad)) {
    let mut guard = BENCH.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(bench) = guard.as_mut() {
        fill(bench);
    }
}

impl BenchLoad {
    fn new(zone: String, path: PathBuf) -> Self {
        Self {
            zone,
            path,
            t0: Instant::now(),
            request: None,
            installed: None,
            spawned: None,
            overlay_down: None,
            overlay_seen: false,
            ingame: None,
            rendered_spawn: None,
            ready_frames: 0,
            ambient: None,
            shot: None,
            batches_at_shot: None,
            g0_at_shot: None,
            last_batches: None,
            last_g0: None,
            last_spawned: false,
        }
    }
}

pub fn insert(app: &mut App, zone: &str, artifacts: &Path) {
    let path = screenshot_path(artifacts, zone);
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        diag::error!(Launch, "bench-load: create {}: {error}", parent.display());
        return;
    }

    let _ = std::fs::remove_file(&path);
    app.world_mut()
        .get_resource_mut::<CaptureQueue>()
        .expect("CaptureQueue: RenderPlugin must be added before the bench-load capture")
        .push(CaptureRequest {
            path: path.clone(),
            exit_after_capture: false,
        });
    *BENCH.lock().unwrap_or_else(|poison| poison.into_inner()) =
        Some(BenchLoad::new(zone.to_owned(), path.clone()));
    app.add_systems(Last, poll_bench_load);
    diag::info!(Launch, "bench-load: screenshot {}", path.display());
}

#[allow(clippy::too_many_arguments)]
fn poll_bench_load(
    request: Option<Res<MatchLoadRequest>>,
    busy: Option<Res<MatchLoadBusy>>,
    mut installed: MessageReader<MatchInstalled>,
    scene: Option<Res<WorldScene>>,
    loading: Option<Res<LoadingScreen>>,
    screen: Option<Res<ui::AppScreen>>,
    ambient: Option<Res<MapAmbientBooted>>,
    stats: Option<Res<DpvsFrameStats>>,
    working: Option<Res<render::ColourWorkingSet>>,
) {
    let now = Instant::now();
    let saw_request = request.is_some() || busy.is_some_and(|b| b.0);
    let saw_installed = installed.read().next().is_some();
    let spawned_now = scene.as_deref().is_some_and(|s| s.spawned);
    let overlay_now = loading.is_some();
    let ingame_now = screen.is_some_and(|s| matches!(*s, ui::AppScreen::InGame));
    let ambient_now = ambient.is_some_and(|b| b.0);
    let census = stats
        .as_deref()
        .map(|s| (s.submitted_batches, s.g0_world_surfs.len() as u32));
    bench_mut(|bench| {
        if bench.request.is_none() && saw_request {
            bench.request = Some(now);
        }
        if bench.installed.is_none() && saw_installed {
            bench.installed = Some(now);
        }
        if spawned_now {
            bench.last_spawned = true;
            if bench.spawned.is_none() {
                bench.spawned = Some(now);
            }
        }
        if overlay_now {
            bench.overlay_seen = true;
        } else if bench.overlay_down.is_none() && bench.overlay_seen {
            bench.overlay_down = Some(now);
        }
        if bench.ingame.is_none() && ingame_now {
            bench.ingame = Some(now);
        } else if ingame_now && bench.rendered_spawn.is_none() {
            let ready = working
                .as_ref()
                .is_some_and(|set| set.hits > 0 && set.pipeline_not_ready == 0);
            bench.ready_frames = if ready { bench.ready_frames + 1 } else { 0 };
            if bench.ready_frames >= 2 {
                bench.rendered_spawn = Some(now);
            }
        }
        if bench.ambient.is_none() && ambient_now {
            bench.ambient = Some(now);
        }
        if let Some((batches, g0)) = census {
            bench.last_batches = Some(batches);
            bench.last_g0 = Some(g0);
        }

        if bench.shot.is_none()
            && let Ok(meta) = std::fs::metadata(&bench.path)
            && meta.len() > 0
        {
            bench.shot = Some(now);
            bench.batches_at_shot = bench.last_batches;
            bench.g0_at_shot = bench.last_g0;
        }
    });
}

fn offset(t0: Instant, t: Option<Instant>) -> String {
    match t {
        Some(t) => format!("{:.1}s", t.duration_since(t0).as_secs_f64()),
        None => "MISS".to_owned(),
    }
}

fn stage(first: Option<Instant>, second: Option<Instant>) -> String {
    match (first, second) {
        (Some(first), Some(second)) if second >= first => {
            format!("{:.1}s", (second - first).as_secs_f64())
        }

        (Some(_), Some(_)) => "overlap".to_owned(),
        _ => "MISS".to_owned(),
    }
}

fn kib(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{} KiB", bytes / 1024)
    }
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width != 0 && height != 0).then_some((width, height))
}

pub fn print_summary(trace: Option<PathBuf>) {
    let guard = BENCH.lock().unwrap_or_else(|poison| poison.into_inner());
    let Some(bench) = guard.as_ref() else {
        diag::announce_stdout(
            "bench-load: MISS (no bench state — IW4L_BENCH_LOAD without insert?)",
        );
        return;
    };
    let total = bench.t0.elapsed();
    let mut lines = vec![format!("bench-load: zone={}", bench.zone)];
    let command_offset = |end: Option<Instant>| match (COMMAND_START.get(), end) {
        (Some((start, _)), Some(end)) => {
            format!("{:.3}s", end.duration_since(*start).as_secs_f64())
        }
        _ => "MISS".to_owned(),
    };
    let playable = bench
        .rendered_spawn
        .zip(bench.overlay_down)
        .map(|(spawn, overlay)| spawn.max(overlay));
    lines.push(format!(
        "bench-load: command_to_spawn={} command_to_rendered_spawn={} command_to_playable={} start={}",
        command_offset(bench.ingame), command_offset(bench.rendered_spawn), command_offset(playable),
        COMMAND_START.get().map_or("MISS", |(_, source)| *source),
    ));
    match std::fs::metadata(&bench.path) {
        Ok(meta) if meta.len() > 0 => {
            let dims = std::fs::read(&bench.path)
                .ok()
                .and_then(|bytes| png_dimensions(&bytes))
                .map(|(w, h)| format!(", {w}x{h}"))
                .unwrap_or(String::new());
            lines.push(format!(
                "bench-load: screenshot={} ({}{dims})",
                bench.path.display(),
                kib(meta.len())
            ));
        }
        _ => lines.push(format!(
            "bench-load: screenshot=MISS (no file at {})",
            bench.path.display()
        )),
    }
    lines.push(format!(
        "bench-load: t=request={} installed={} spawned={} overlay_down={} ingame={} ambient={} shot={}",
        offset(bench.t0, bench.request),
        offset(bench.t0, bench.installed),
        offset(bench.t0, bench.spawned),
        offset(bench.t0, bench.overlay_down),
        offset(bench.t0, bench.ingame),
        offset(bench.t0, bench.ambient),
        offset(bench.t0, bench.shot),
    ));
    lines.push(format!(
        "bench-load: total={:.1}s zone_walk={} spawn_gpu={} ambient={} screenshot={}",
        total.as_secs_f64(),
        stage(bench.request, bench.installed),
        stage(bench.installed, bench.spawned),
        stage(bench.spawned, bench.ambient),
        stage(bench.ambient, bench.shot),
    ));
    match (bench.batches_at_shot, bench.g0_at_shot) {
        (Some(batches), Some(g0)) => lines.push(format!(
            "bench-load: world_batches={batches} g0={g0} spawned={}",
            i32::from(bench.last_spawned)
        )),
        _ => lines.push(format!(
            "bench-load: world_batches=MISS spawned={}",
            i32::from(bench.last_spawned)
        )),
    }
    match trace {
        Some(dir) => lines.push(format!(
            "bench-load: trace={}",
            dir.join("trace.pftrace").display()
        )),
        None => lines.push("bench-load: trace=MISS (perf off or flush failed)".to_owned()),
    }
    for line in &lines {
        diag::announce_stdout(line);
        diag::info!(Launch, "{line}");
    }
}
