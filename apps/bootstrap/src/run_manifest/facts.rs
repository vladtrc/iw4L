use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::tasks::ComputeTaskPool;
use bevy::window::PrimaryWindow;

use crate::run_manifest::manifest::{AdapterFacts, RuntimeFacts, WindowFacts};

static FACTS: OnceLock<RuntimeFacts> = OnceLock::new();

static WORKLOAD: OnceLock<(String, Option<String>, String)> = OnceLock::new();

pub(crate) fn workload(zone: &str, demo: Option<&str>, role: &str) {
    let _ = WORKLOAD.set((zone.to_owned(), demo.map(str::to_owned), role.to_owned()));
}

static PIPELINED: OnceLock<bool> = OnceLock::new();

pub(crate) fn scheduling(app: &App) {
    let _ = PIPELINED
        .set(app.is_plugin_added::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>());
}

pub(crate) fn snapshot() -> RuntimeFacts {
    let mut facts = match FACTS.get() {
        Some(facts) => facts.clone(),
        None => RuntimeFacts::default(),
    };
    if let Some((zone, demo, role)) = WORKLOAD.get() {
        facts.zone = Some(zone.clone());
        facts.demo = demo.clone();
        facts.role = Some(role.clone());
    }
    facts
}

pub(crate) fn collect(
    adapter: Option<Res<RenderAdapterInfo>>,
    device: Option<Res<RenderDevice>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<&Camera>,
) {
    if FACTS.get().is_some() {
        return;
    }
    let facts = RuntimeFacts {
        zone: None,
        demo: None,
        role: None,
        adapter: adapter.map(|adapter| AdapterFacts {
            name: adapter.name.clone(),
            backend: format!("{:?}", adapter.backend),
            device_type: format!("{:?}", adapter.device_type),
            driver: adapter.driver.clone(),
            driver_info: adapter.driver_info.clone(),
            timestamp_queries: device.map(|device| {
                device
                    .features()
                    .contains(bevy::render::render_resource::WgpuFeatures::TIMESTAMP_QUERY)
            }),
        }),
        window: windows.single().ok().map(|window| WindowFacts {
            width: window.resolution.physical_width(),
            height: window.resolution.physical_height(),
            scale: f64::from(window.resolution.scale_factor()),
        }),
        render_target: cameras
            .iter()
            .filter_map(|camera| camera.physical_target_size())
            .map(|size| (size.x, size.y))
            .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height)),
        view_extent: cameras
            .iter()
            .filter_map(|camera| camera.physical_viewport_size())
            .map(|size| (size.x, size.y))
            .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height)),
        present_mode: windows
            .single()
            .ok()
            .map(|window| format!("{:?}", window.present_mode)),
        pools: pools(),
        pipelined_rendering: PIPELINED.get().copied(),
        compute_threads: ComputeTaskPool::try_get().map(|pool| pool.thread_num()),
        bevy_tracing: cfg!(feature = "bevy-trace"),
        frame_latency_requested: crate::plugins::frame_latency(),
    };
    let _ = FACTS.set(facts);
}

pub(crate) fn announce(
    mut done: Local<bool>,
    adapter: Option<Res<RenderAdapterInfo>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<&Camera>,
) {
    if *done {
        return;
    }
    let Some(target) = cameras
        .iter()
        .filter_map(|camera| camera.physical_target_size())
        .max_by_key(|size| u64::from(size.x) * u64::from(size.y))
    else {
        return;
    };
    *done = true;
    let window = windows.single().ok();
    diag::announce_stdout(&format!(
        "runtime: backend={} adapter={:?} window={} target={}x{} present={}",
        adapter
            .as_ref()
            .map_or_else(|| "unknown".to_owned(), |a| format!("{:?}", a.backend)),
        adapter.as_ref().map_or("unknown", |a| a.name.as_str()),
        window.map_or_else(
            || "unknown".to_owned(),
            |w| format!(
                "{}x{}",
                w.resolution.physical_width(),
                w.resolution.physical_height()
            )
        ),
        target.x,
        target.y,
        window.map_or_else(|| "unknown".to_owned(), |w| format!("{:?}", w.present_mode)),
    ));
}

fn pools() -> Vec<(String, usize)> {
    use bevy::tasks::{AsyncComputeTaskPool, IoTaskPool};
    let mut out = Vec::new();
    if let Some(pool) = ComputeTaskPool::try_get() {
        out.push(("compute".to_owned(), pool.thread_num()));
    }
    if let Some(pool) = AsyncComputeTaskPool::try_get() {
        out.push(("async_compute".to_owned(), pool.thread_num()));
    }
    if let Some(pool) = IoTaskPool::try_get() {
        out.push(("io".to_owned(), pool.thread_num()));
    }
    out.push(("load".to_owned(), assets::session_load::load_workers()));
    out
}
