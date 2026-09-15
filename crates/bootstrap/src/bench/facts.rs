//! Reading the manifest's runtime half out of the running app.
//!
//! The device, the surface and the pools do not exist when the bench is
//! inserted — the render plugin has not finished, the window has no size and
//! the task pools have not been asked for a thread. They all exist by the first
//! frame, and none of them changes after it, so this snapshots once and hands
//! the result to the exit hook, which runs with no `World` to ask.

use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::render::renderer::{RenderAdapterInfo, RenderDevice};
use bevy::tasks::ComputeTaskPool;
use bevy::window::PrimaryWindow;

use crate::bench::manifest::{AdapterFacts, RuntimeFacts, WindowFacts};

static FACTS: OnceLock<RuntimeFacts> = OnceLock::new();

/// The workload, which is known when the bench is inserted rather than when
/// the device is. Kept beside the collected facts so `snapshot` returns one
/// complete answer.
static WORKLOAD: OnceLock<(String, Option<String>, String)> = OnceLock::new();

/// What was asked for, recorded at insert time: zone, demo path and role.
pub(crate) fn workload(zone: &str, demo: Option<&str>, role: &str) {
    let _ = WORKLOAD.set((zone.to_owned(), demo.map(str::to_owned), role.to_owned()));
}

/// Whether rendering runs pipelined. Read from the assembled `App` at insert
/// time, because by the first frame the plugin is no longer a thing to ask
/// about — it has already moved the render world onto its own thread.
static PIPELINED: OnceLock<bool> = OnceLock::new();

pub(crate) fn scheduling(app: &App) {
    let _ = PIPELINED
        .set(app.is_plugin_added::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>());
}

/// What was collected, or an empty set if the run never reached a frame. Every
/// field is optional, so "never reached a frame" reads as a manifest of nulls
/// rather than as a manifest of wrong values.
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

/// Collect once, then do nothing. Runs every frame because the cost of the
/// `OnceLock` load is smaller than the cost of a run condition that has to be
/// evaluated to reach the same answer.
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
            name: adapter.0.name.clone(),
            backend: format!("{:?}", adapter.0.backend),
            device_type: format!("{:?}", adapter.0.device_type),
            driver: adapter.0.driver.clone(),
            driver_info: adapter.0.driver_info.clone(),
            // Whether the *device* was created with the feature, not whether
            // the adapter could have offered it: a pass can only be timed if
            // the device we actually hold can resolve the query.
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
        // The largest target any camera drew into. Several cameras share one
        // surface here; the biggest is the one the frame cost scales with, and
        // a camera that has not drawn yet reports nothing rather than zero.
        render_target: cameras
            .iter()
            .filter_map(|camera| camera.physical_target_size())
            .map(|size| (size.x, size.y))
            .max_by_key(|(width, height)| u64::from(*width) * u64::from(*height)),
        // Not the same as the target: a camera with a viewport draws into part
        // of the surface, and the passes and the postfx chain are sized by this
        // one. Printing only the target is how a manifest headed 2880x1800 sat
        // next to a postfx log reading 2880x1688 with nothing to say which the
        // workload was.
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
        // Not the ECS executor kind — Bevy exposes no getter for it — but the
        // thing that decides whether systems can overlap at all: how many
        // threads the compute pool was built with.
        compute_threads: ComputeTaskPool::try_get().map(|pool| pool.thread_num()),
    };
    let _ = FACTS.set(facts);
}

/// Threads in each Bevy pool. The pools are global and already built by the
/// time a frame runs, so this reads them rather than constructing anything.
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
    // Not a Bevy pool: the asset walk builds its own, sized from the CPUs the
    // process was given, and it is the one every load stage runs on.
    out.push(("load".to_owned(), assets::session_load::load_workers()));
    out
}
