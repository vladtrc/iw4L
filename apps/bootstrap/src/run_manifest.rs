mod facts;
mod identity;
mod manifest;
mod observations;

use bevy::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

struct Run {
    artifacts: PathBuf,
    progress: asset_transport::LoadProgress,
}
static RUN: Mutex<Option<Run>> = Mutex::new(None);
static ENABLED: OnceLock<bool> = OnceLock::new();
static FINISHED: AtomicBool = AtomicBool::new(false);

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| perf::switch("IW4L_BENCH"))
}

pub fn arm() {
    asset_transport::load_jobs::arm(enabled());
}

pub fn announce_runtime(app: &mut App) {
    app.add_systems(Last, facts::announce);
}

pub fn insert(
    app: &mut App,
    zone: &str,
    demo: Option<&str>,
    role: &str,
    artifacts: &Path,
    progress: asset_transport::LoadProgress,
) {
    facts::workload(zone, demo, role);
    facts::scheduling(app);
    identity::spawn(
        demo.map(Path::new),
        artifacts.parent().unwrap_or_else(|| Path::new(".")),
    );
    let filename: String = zone
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let filename = filename.trim_start_matches('.');
    let screenshot = artifacts.join("screenshots/bench").join(format!(
        "{}.png",
        if filename.is_empty() {
            "unknown"
        } else {
            filename
        }
    ));
    if let Some(parent) = screenshot.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        diag::error!(Launch, "create {}: {error}", parent.display());
        return;
    }
    let _ = std::fs::remove_file(&screenshot);
    app.world_mut()
        .resource_mut::<render::diag::capture::CaptureQueue>()
        .push(render::diag::capture::CaptureRequest {
            path: screenshot,
            exit_after_capture: false,
            any_screen: false,
        });
    *RUN.lock().unwrap_or_else(|p| p.into_inner()) = Some(Run {
        artifacts: artifacts.to_owned(),
        progress,
    });
    // Exit hooks run in reverse registration order: observations precede the trace flush.
    diag::exit::at_exit(finish);
    app.add_systems(Last, (facts::collect, observe));
    #[cfg(not(unix))]
    app.add_systems(PostUpdate, finish_on_exit);
}

#[cfg(not(unix))]
fn finish_on_exit(mut exit: MessageReader<AppExit>) {
    if exit.read().last().is_some() {
        finish();
    }
}

pub fn finish() {
    if FINISHED.swap(true, Ordering::SeqCst) {
        return;
    }
    let guard = RUN.lock().unwrap_or_else(|p| p.into_inner());
    let Some(run) = guard.as_ref() else {
        return;
    };
    let load_jobs = observations::write(&run.progress);
    let _ = perf::flush();
    let Ok(dir) = perf::run::dir() else {
        return;
    };
    let path = dir.join("manifest.json");
    let mut value: serde_json::Value = std::fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let facts = manifest::build(
        &facts::snapshot(),
        perf::run::id().as_deref(),
        &run.artifacts,
    );
    if let (Some(target), Some(facts)) = (value.as_object_mut(), facts.as_object()) {
        target.extend(facts.clone());
    }
    value["observations"] = serde_json::json!({ "load_jobs": load_jobs });
    if let Some(git) = value
        .pointer("/build/git_sha")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
    {
        value["git"] = git.into();
        value["git_error"] = serde_json::Value::Null;
    }
    if let Err(error) = manifest::write(&path, &value) {
        diag::error!(Launch, "{error}");
    }
}

fn observe(
    scene: Option<Res<render_frontend::prepare::scene::world::WorldScene>>,
    loading: Option<Res<assets::LoadingScreen>>,
    screen: Option<Res<ui::AppScreen>>,
    working: Option<Res<render_gpu::ColourWorkingSet>>,
    mut last: Local<Option<(bool, bool, bool)>>,
    mut ready: Local<u32>,
    mut playable: Local<bool>,
) {
    let spawned = scene.as_deref().is_some_and(|s| s.spawned);
    let ingame = screen.is_some_and(|s| matches!(*s, ui::AppScreen::InGame));
    let drawing = working
        .as_ref()
        .is_some_and(|s| s.hits > 0 && s.pipeline_not_ready == 0);
    let state = (loading.is_some(), spawned, ingame);
    if *last != Some(state) {
        let command_elapsed_ns = std::env::var("IW4L_BENCH_STARTED_NS")
            .ok()
            .and_then(|value| value.parse::<u128>().ok())
            .and_then(|start| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|now| now.as_nanos().checked_sub(start))
            })
            .and_then(|elapsed| u64::try_from(elapsed).ok());
        perf::instant!("load_state", "iw4l.frame", "loading" => state.0 as u64,
            "spawned" => state.1 as u64, "ingame" => state.2 as u64,
            "command_elapsed_ns" => command_elapsed_ns);
        *last = Some(state);
    }
    *ready = if ingame && drawing { *ready + 1 } else { 0 };
    if !*playable && spawned && !state.0 && *ready >= 2 {
        perf::instant!("benchmark_playable", "iw4l.frame");
        *playable = true;
    }
}
