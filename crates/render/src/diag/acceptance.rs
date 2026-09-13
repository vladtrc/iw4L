use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::Real;
use bevy::window::PresentMode;
use frame::HasWorld;
use net::ClientSet;
use serde::{Deserialize, Serialize};

use render_frontend::prepare::scene::camera::SimCamera;
use render_frontend::prepare::scene::cull::{DpvsFrameStats, apply_dpvs_cull};
use render_frontend::prepare::scene::world::WorldScene;

pub const ACCEPTANCE_WIDTH: u32 = 1280;

pub const ACCEPTANCE_HEIGHT: u32 = 720;

pub const ACCEPTANCE_WARM_FRAMES: u32 = 120;

pub const ACCEPTANCE_SAMPLE_FRAMES: u32 = 600;

pub const ACCEPTANCE_PRESENT_MODE: PresentMode = PresentMode::AutoNoVsync;

pub const ACCEPTANCE_MAPS: &[&str] = &["mp_boneyard", "mp_favela", "mp_rust", "mp_highrise"];

pub const ACCEPTANCE_ENV: &str = "IW4L_RENDER_ACCEPTANCE";

pub const ACCEPTANCE_FLAG: &str = "--render-acceptance";

const ACCEPTANCE_FORMAT: &str = "iw4l-render-acceptance-v2";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetricValue {
    Finite(f64),
    Unavailable,
}

impl Serialize for MetricValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Finite(v) => serializer.serialize_f64(*v),
            Self::Unavailable => serializer.serialize_str("Unavailable"),
        }
    }
}

impl<'de> Deserialize<'de> for MetricValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = MetricValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a finite f64 or the string \"Unavailable\"")
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                if v.is_finite() {
                    Ok(MetricValue::Finite(v))
                } else {
                    Err(E::custom("non-finite metric"))
                }
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                self.visit_f64(v as f64)
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                self.visit_f64(v as f64)
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                if v == "Unavailable" {
                    Ok(MetricValue::Unavailable)
                } else {
                    Err(E::custom(format!("unknown metric string: {v}")))
                }
            }
        }
        deserializer.deserialize_any(V)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AcceptanceSample {
    pub i: u32,
    pub wall_frame_ms: f64,
    pub visible_cells: u32,
    pub bsp_visible_surfaces: u32,
    pub bsp_admitted_surfaces: u32,
    pub bsp_run_n: u32,
    pub bsp_input_gap_n: u32,
    pub bsp_ranges_unavailable: u32,
    pub bsp_submitted_surfaces: [u32; 4],
    pub bsp_submit_refused_surfaces: [u32; 4],
    pub bsp_drawn_surfaces: [u32; 4],
    pub bsp_draw_refused_surfaces: [u32; 4],
    pub surfaces: u32,
    pub triangles: u32,
    pub rebinds: u32,
    pub submitted_batches: u32,
    pub rewritten_index_bytes: u64,
    pub visibility_changes: u32,
    pub gpu_frame_ms: MetricValue,
    pub draw_calls: MetricValue,

    pub process_allocations: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Percentiles {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AcceptanceSummary {
    pub format: &'static str,
    pub map: String,
    pub width: u32,
    pub height: u32,
    pub present_mode: &'static str,
    pub warm_frames: u32,
    pub sample_frames: u32,
    pub samples_written: u32,
    pub wall_frame_ms: Percentiles,
    pub gpu_frame_ms: MetricValue,
    pub draw_calls: MetricValue,
    pub png: String,
    pub trace: String,
    pub process_allocations_end: u64,
}

#[derive(Resource, Clone, Debug)]
pub struct AcceptanceRun {
    pub artifact_dir: PathBuf,
    pub map: String,
}

impl AcceptanceRun {
    pub fn from_env(map: impl Into<String>) -> Option<Self> {
        let dir = std::env::var_os(ACCEPTANCE_ENV)?;
        Some(Self {
            artifact_dir: PathBuf::from(dir),
            map: map.into(),
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    WaitWorld,
    Warm,
    Sample,
    Capture,
    Done,
    Failed,
}

#[derive(Resource, Default)]
struct AcceptanceState {
    phase: Phase,
    warm_seen: u32,
    samples: Vec<AcceptanceSample>,
    capture_armed: bool,
    capture_wait: u32,
    fail_reason: Option<String>,
}

pub fn register_acceptance_systems(app: &mut App) {
    if !app.world().contains_resource::<AcceptanceRun>() {
        return;
    }
    app.init_resource::<AcceptanceState>().add_systems(
        Update,
        (
            acceptance_prepare,
            acceptance_sample.after(apply_dpvs_cull),
            acceptance_capture_and_exit,
        )
            .chain()
            .in_set(ClientSet::Present),
    );
}

fn acceptance_prepare(
    run: Res<AcceptanceRun>,
    mut state: ResMut<AcceptanceState>,
    has_world: Res<HasWorld>,
    scene: Res<WorldScene>,
    mut sim_cam: Option<ResMut<SimCamera>>,
) {
    if state.phase != Phase::WaitWorld {
        return;
    }
    if !has_world.0 {
        return;
    }
    if scene.intermission_view.is_none() {
        let reason = format!(
            "acceptance refused: map `{}` has no authored mp_global_intermission",
            run.map
        );
        diag::error!(World, "{reason}");
        state.fail_reason = Some(reason);
        state.phase = Phase::Failed;
        return;
    }
    if let Some(cam) = sim_cam.as_mut() {
        cam.freeze_fly = true;
        cam.enabled = false;
    }
    let _ = std::fs::create_dir_all(&run.artifact_dir);
    diag::info!(
        World,
        "acceptance: world ready on {}; warm {} then sample {}",
        run.map,
        ACCEPTANCE_WARM_FRAMES,
        ACCEPTANCE_SAMPLE_FRAMES
    );
    state.phase = Phase::Warm;
}

fn acceptance_sample(
    mut state: ResMut<AcceptanceState>,
    stats: Res<DpvsFrameStats>,
    frame: Res<crate::diag::render_frame_diag::RenderFrameDiag>,
    real: Res<Time<Real>>,
) {
    match state.phase {
        Phase::Warm => {
            if stats.unculled_triangles == 0 && stats.triangles == 0 {
                return;
            }
            state.warm_seen = state.warm_seen.saturating_add(1);
            if state.warm_seen >= ACCEPTANCE_WARM_FRAMES {
                state.phase = Phase::Sample;
                state.samples.clear();
                state.samples.reserve(ACCEPTANCE_SAMPLE_FRAMES as usize);
                diag::info!(
                    World,
                    "acceptance: sampling {} frames",
                    ACCEPTANCE_SAMPLE_FRAMES
                );
            }
        }
        Phase::Sample => {
            if state.samples.len() as u32 >= ACCEPTANCE_SAMPLE_FRAMES {
                return;
            }
            let wall_ms = real.delta_secs_f64() * 1000.0;
            let alloc = diag::process_allocations();
            let i = state.samples.len() as u32;
            state.samples.push(AcceptanceSample {
                i,
                wall_frame_ms: wall_ms,
                visible_cells: stats.visible_cells,
                bsp_visible_surfaces: stats.bsp_visible_surfaces,
                bsp_admitted_surfaces: stats.bsp_admitted_surfaces,
                bsp_run_n: stats.bsp_run_n,
                bsp_input_gap_n: stats.bsp_input_gap_n,
                bsp_ranges_unavailable: stats.bsp_ranges_unavailable,
                bsp_submitted_surfaces: frame.bsp_submitted_surfaces,
                bsp_submit_refused_surfaces: frame.bsp_submit_refused_surfaces,
                bsp_drawn_surfaces: frame.bsp_drawn_surfaces,
                bsp_draw_refused_surfaces: frame.bsp_draw_refused_surfaces,
                surfaces: stats.surfaces,
                triangles: stats.triangles,
                rebinds: stats.rebinds,
                submitted_batches: stats.submitted_batches,
                rewritten_index_bytes: stats.rewritten_index_bytes,
                visibility_changes: stats.visibility_changes,
                gpu_frame_ms: MetricValue::Unavailable,
                draw_calls: MetricValue::Unavailable,
                process_allocations: alloc.process_allocations,
            });
            if state.samples.len() as u32 >= ACCEPTANCE_SAMPLE_FRAMES {
                state.phase = Phase::Capture;
                diag::info!(World, "acceptance: samples complete; capturing PNG");
            }
        }
        _ => {}
    }
}

fn acceptance_capture_and_exit(
    mut commands: Commands,
    run: Res<AcceptanceRun>,
    mut state: ResMut<AcceptanceState>,
    mut exit: MessageWriter<AppExit>,
) {
    if state.phase == Phase::Failed {
        if let Some(reason) = &state.fail_reason {
            let path = run.artifact_dir.join("FAILURE.txt");
            let _ = std::fs::write(&path, format!("{reason}\n"));
        }
        exit.write(AppExit::from_code(2));
        state.phase = Phase::Done;
        return;
    }
    if state.phase != Phase::Capture {
        return;
    }
    if !state.capture_armed {
        if let Err(e) = write_trace_and_summary(&run, &state.samples) {
            diag::error!(World, "acceptance: write ledger failed: {e}");
            state.fail_reason = Some(e);
            state.phase = Phase::Failed;
            return;
        }
        let png = run.artifact_dir.join("acceptance.png");
        diag::info!(World, "acceptance: screenshot {}", png.display());
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(png));
        state.capture_armed = true;
        state.capture_wait = 0;
        return;
    }
    state.capture_wait = state.capture_wait.saturating_add(1);

    if state.capture_wait >= 30 {
        state.phase = Phase::Done;
        exit.write(AppExit::Success);
    }
}

fn write_trace_and_summary(
    run: &AcceptanceRun,
    samples: &[AcceptanceSample],
) -> Result<(), String> {
    if samples.len() as u32 != ACCEPTANCE_SAMPLE_FRAMES {
        return Err(format!(
            "expected {} samples, have {}",
            ACCEPTANCE_SAMPLE_FRAMES,
            samples.len()
        ));
    }
    let trace_path = run.artifact_dir.join("acceptance.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&trace_path)
        .map_err(|e| format!("open jsonl: {e}"))?;
    for sample in samples {
        let line = serde_json::to_string(sample).map_err(|e| format!("jsonl: {e}"))?;
        writeln!(file, "{line}").map_err(|e| format!("jsonl write: {e}"))?;
    }
    file.flush().map_err(|e| format!("jsonl flush: {e}"))?;

    let wall: Vec<f64> = samples.iter().map(|s| s.wall_frame_ms).collect();
    let alloc_end = samples.last().map(|s| s.process_allocations).unwrap_or(0);
    let summary = AcceptanceSummary {
        format: ACCEPTANCE_FORMAT,
        map: run.map.clone(),
        width: ACCEPTANCE_WIDTH,
        height: ACCEPTANCE_HEIGHT,
        present_mode: "AutoNoVsync",
        warm_frames: ACCEPTANCE_WARM_FRAMES,
        sample_frames: ACCEPTANCE_SAMPLE_FRAMES,
        samples_written: samples.len() as u32,
        wall_frame_ms: percentiles(&wall)?,
        gpu_frame_ms: MetricValue::Unavailable,
        draw_calls: MetricValue::Unavailable,
        png: "acceptance.png".into(),
        trace: "acceptance.jsonl".into(),
        process_allocations_end: alloc_end,
    };
    let summary_path = run.artifact_dir.join("summary.json");
    let body = serde_json::to_string_pretty(&summary).map_err(|e| format!("summary: {e}"))?;
    let mut out = File::create(&summary_path).map_err(|e| format!("summary create: {e}"))?;
    writeln!(out, "{body}").map_err(|e| format!("summary write: {e}"))?;
    Ok(())
}

pub fn percentiles(values: &[f64]) -> Result<Percentiles, String> {
    if values.is_empty() {
        return Err("percentiles: empty series".into());
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("percentiles: non-finite value".into());
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    let pick = |pct: usize| -> f64 {
        let rank = ((pct * n).div_ceil(100)).saturating_sub(1).min(n - 1);
        sorted[rank]
    };
    Ok(Percentiles {
        p50: pick(50),
        p95: pick(95),
        max: sorted[n - 1],
    })
}
