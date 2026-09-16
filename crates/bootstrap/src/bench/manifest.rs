//! `manifest.json`: what this run *was*, so two reports can be compared.
//!
//! A number without a manifest is not a measurement, it is an anecdote. Two
//! runs differing by 3 ms mean nothing until the reader knows they were the
//! same binary, the same map, the same present mode and the same cache state —
//! and the most expensive mistake in this repository's history of benchmarking
//! is comparing a report against source that was never the source it was built
//! from.
//!
//! So every field here is one of three things and says which:
//!
//! * baked in at build time (`build.rs`) — the revision, the toolchain, the
//!   locked `bevy` and `wgpu`. These describe the *binary*, not the tree the
//!   process happens to be standing in;
//! * read from the running process — the adapter, the window, the pools;
//! * absent. An absent field is `null`. There is no default, no zero and no
//!   "unknown" string that sorts next to a real value.
//!
//! Cache state is deliberately narrow. The manifest names the layers this
//! process controls — the prepared-artifact cache it can see on disk — and
//! says nothing about the OS page cache or the GPU driver's shader cache,
//! because it does not control them and calling a run "cold" on the strength of
//! a guess about them is how a cache hit gets reported as an optimisation.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// A build-time string, or `null` when the build could not learn it. `build.rs`
/// emits an empty string for "not known" so `env!` always compiles.
macro_rules! built {
    ($key:literal) => {
        match env!($key) {
            "" => Value::Null,
            value => Value::String(value.to_owned()),
        }
    };
}

/// What the running process knows about itself that the build could not.
#[derive(Debug, Default, Clone)]
pub(crate) struct RuntimeFacts {
    pub(crate) zone: Option<String>,
    pub(crate) demo: Option<String>,
    pub(crate) role: Option<String>,
    /// Adapter name, backend, device type and driver, as wgpu reported them.
    pub(crate) adapter: Option<AdapterFacts>,
    pub(crate) window: Option<WindowFacts>,
    /// The surface the camera actually drew into. Not the window: a scale
    /// factor, a letterbox or a render-to-texture camera make the two differ,
    /// and it is this one the frame cost scales with.
    pub(crate) render_target: Option<(u32, u32)>,
    /// The part of that surface a camera actually rendered — its viewport. The
    /// passes and the postfx chain are sized by this, not by the surface, and
    /// the two differ whenever a camera is letterboxed or inset.
    pub(crate) view_extent: Option<(u32, u32)>,
    pub(crate) present_mode: Option<String>,
    /// Threads in each Bevy pool, by pool name.
    pub(crate) pools: Vec<(String, usize)>,
    /// Whether rendering runs a frame behind on its own thread, and whether
    /// this build has Bevy's multi-threaded executor at all. Two runs with
    /// different answers here are not comparable.
    pub(crate) pipelined_rendering: Option<bool>,
    pub(crate) compute_threads: Option<usize>,
    /// Frames the surface was *asked* to let the CPU run ahead of the GPU.
    /// wgpu treats it as a hint and a backend may clamp it — on Vulkan it is
    /// tied to the swapchain image count — so this is the request and not the
    /// grant, and a pair of runs across it is only comparable on what each one
    /// asked for.
    pub(crate) frame_latency_requested: u32,
    /// Whether this binary was built with Bevy's own `tracing` spans on. Off
    /// is the normal build and the one a timed run uses; on is a diagnostic
    /// build where the spans inside the render graph — the `queue_submit`
    /// around `RenderQueue::submit` among them — are recorded, and where the
    /// frame times are the subscriber's as much as the runtime's.
    ///
    /// The flag is here so a report can never be read as if a submit had been
    /// timed on a run that could not have timed it.
    pub(crate) bevy_tracing: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AdapterFacts {
    pub(crate) name: String,
    pub(crate) backend: String,
    pub(crate) device_type: String,
    pub(crate) driver: String,
    pub(crate) driver_info: String,
    /// Whether the device can resolve GPU timestamp queries. Without it every
    /// `gpu_*` counter in the report is MISS, and the reader needs to know that
    /// is a capability and not a workload that drew nothing.
    pub(crate) timestamp_queries: Option<bool>,
}

#[derive(Debug, Clone)]
pub(crate) struct WindowFacts {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) scale: f64,
}

pub(crate) fn build(facts: &RuntimeFacts, run_id: Option<&str>, artifacts: &Path) -> Value {
    let digests = super::identity::digests();
    json!({
        "format": "iw4l-bench-1",
        "run": run_id,
        "build": {
            "git_sha": built!("IW4L_BUILD_GIT_SHA"),
            "git_describe": built!("IW4L_BUILD_GIT_DESCRIBE"),
            "git_dirty": dirty(),
            "rustc": built!("IW4L_BUILD_RUSTC"),
            "profile": built!("IW4L_BUILD_PROFILE"),
            "profile_kind": built!("IW4L_BUILD_PROFILE_KIND"),
            "target": built!("IW4L_BUILD_TARGET"),
            "opt_level": built!("IW4L_BUILD_OPT_LEVEL"),
            "debug_assertions": cfg!(debug_assertions),
            "cargo_lock_fnv1a": built!("IW4L_BUILD_LOCK_HASH"),
            "cargo_lock_sha256": digests.lock,
            "git_patch_fnv1a": built!("IW4L_BUILD_GIT_PATCH_HASH"),
            "bevy": built!("IW4L_BUILD_BEVY_VERSION"),
            "wgpu": built!("IW4L_BUILD_WGPU_VERSION"),
        },
        "binary": binary(digests.binary.as_deref()),
        "host": host(),
        "gpu": facts.adapter.as_ref().map_or(Value::Null, |adapter| json!({
            "adapter": adapter.name,
            "backend": adapter.backend,
            "device_type": adapter.device_type,
            "driver": adapter.driver,
            "driver_info": adapter.driver_info,
            "timestamp_queries": adapter.timestamp_queries,
        })),
        "window": facts.window.as_ref().map_or(Value::Null, |window| json!({
            "width": window.width,
            "height": window.height,
            "scale_factor": window.scale,
        })),
        "render_target": facts.render_target.map_or(Value::Null, |(width, height)| json!({
            "width": width,
            "height": height,
            "what": "the surface a camera drew into",
        })),
        "view_extent": facts.view_extent.map_or(Value::Null, |(width, height)| json!({
            "width": width,
            "height": height,
            "what": "the viewport the passes and the postfx chain were sized by",
        })),
        "present_mode": facts.present_mode,
        "scheduling": {
            "pipelined_rendering": facts.pipelined_rendering,
            "compute_threads": facts.compute_threads,
            "frame_latency_requested": facts.frame_latency_requested,
            "bevy_tracing": facts.bevy_tracing,
        },
        "threads": threads(),
        "pools": pools(facts),
        "workload": {
            "zone": facts.zone,
            "demo": facts.demo,
            "demo_sha256": digests.demo,
            "role": facts.role,
            "command_line": std::env::args().collect::<Vec<_>>(),
        },
        "env": toggles(),
        "caches": caches(artifacts),
    })
}

/// Every pool, by name and thread count.
///
/// Most are read on the first frame, because that is when they exist and
/// nothing changes them after. The clip-prep pool is not: it starts when the
/// match asks for its first sound, which is long after the first frame, so
/// reading it with the others reported zero threads for a pool that had two.
/// It is read here, at exit, instead.
fn pools(facts: &RuntimeFacts) -> Value {
    let mut pools: Vec<Value> = facts
        .pools
        .iter()
        .map(|(name, threads)| json!({ "pool": name, "threads": threads }))
        .collect();
    pools.push(json!({
        "pool": "audio_prep",
        "threads": audio::clip_prep_cost().workers,
    }));
    Value::Array(pools)
}

fn dirty() -> Value {
    match env!("IW4L_BUILD_GIT_DIRTY") {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::Null,
    }
}

/// The binary this process is running: path, size, mtime and content digest.
///
/// The digest is what pairs two runs — size and mtime agree across a rebuild
/// that changed nothing observable and disagree across a copy that changed
/// nothing at all — and it is read on its own thread from the moment the bench
/// is inserted, so it costs the run nothing. `null` if that thread had not
/// finished when the run ended.
fn binary(sha256: Option<&str>) -> Value {
    let Ok(path) = std::env::current_exe() else {
        return Value::Null;
    };
    let metadata = std::fs::metadata(&path).ok();
    json!({
        "path": path.display().to_string(),
        "bytes": metadata.as_ref().map(std::fs::Metadata::len),
        "sha256": sha256,
        "modified_unix_s": metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| since.as_secs()),
    })
}

/// Every OS thread with its name and the CPUs it is allowed on. A pool that
/// pins its workers is only pinned if the kernel agrees, and this is where
/// that shows — as is the load pool being handed a wider set than the thread
/// that spawned it.
fn threads() -> Value {
    let threads = super::identity::threads();
    if threads.is_empty() {
        return Value::Null;
    }
    json!({
        "main_thread_cpus_allowed": super::identity::main_thread_cpus_allowed(),
        "note": "affinity is per thread on Linux; there is no process-wide mask. \
                 The main thread is narrowed to the performance cores at startup \
                 and the load pool is given back the wider set, so the two rows \
                 differing is the design and not a mistake.",
        "sampled_at": "exit",
        "threads": threads
            .iter()
            .map(|thread| json!({
                "tid": thread.tid,
                "name": thread.name,
                "cpus_allowed": thread.cpus_allowed,
            }))
            .collect::<Vec<_>>(),
    })
}

fn host() -> Value {
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "kernel": kernel(),
        "available_parallelism": std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .ok(),
        "cpu_model": cpu_model(),
    })
}

#[cfg(target_os = "linux")]
fn kernel() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|value| value.trim().to_owned())
}

#[cfg(not(target_os = "linux"))]
fn kernel() -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn cpu_model() -> Option<String> {
    let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
    text.lines()
        .find(|line| line.starts_with("model name"))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_owned())
}

#[cfg(not(target_os = "linux"))]
fn cpu_model() -> Option<String> {
    None
}

/// The `IW4L_*` variables that change what is measured. Recorded by name and
/// value because "the same command" is not the same run when one of these
/// differs, and the command line does not show them.
fn toggles() -> Value {
    let mut set: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| key.starts_with("IW4L_"))
        .collect();
    set.sort();
    Value::Object(
        set.into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect(),
    )
}

/// The cache layers this process can actually see. Each entry says what was
/// found, never whether the run was "cold": the OS page cache and the GPU
/// driver's shader cache are outside this process and are reported as unknown
/// rather than assumed empty.
fn caches(artifacts: &Path) -> Value {
    json!({
        "prepared_artifacts": prepared_cache(artifacts),
        "os_page_cache": Value::Null,
        "driver_shader_cache": Value::Null,
        "note": "os_page_cache and driver_shader_cache are outside this process; \
                 they are reported as unknown rather than assumed cold.",
    })
}

/// What the prepared-artifact cache holds, by kind.
///
/// The store nests: `cache/<kind>/<prefix>/<key>`, so reading only the top
/// directory counted zero files on a run whose log reported seven thousand mip
/// hits and five hundred clip hits. A zero there does not mean a cold run — it
/// meant the wrong directory — and the two have to be told apart, so this walks
/// the kinds and reports each one.
fn prepared_cache(artifacts: &Path) -> Value {
    let dir = artifacts.join("cache");
    let Ok(kinds) = std::fs::read_dir(&dir) else {
        return json!({ "path": dir.display().to_string(), "present": false });
    };
    let mut by_kind = serde_json::Map::new();
    let mut files = 0u64;
    let mut bytes = 0u64;
    for kind in kinds.flatten() {
        if !kind.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let (kind_files, kind_bytes) = walk(&kind.path());
        files += kind_files;
        bytes += kind_bytes;
        by_kind.insert(
            kind.file_name().to_string_lossy().into_owned(),
            json!({ "files": kind_files, "bytes": kind_bytes }),
        );
    }
    json!({
        "path": dir.display().to_string(),
        "present": true,
        "files": files,
        "bytes": bytes,
        "by_kind": Value::Object(by_kind),
    })
}

/// Files and bytes under a directory, following its subdirectories.
fn walk(dir: &Path) -> (u64, u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    let mut files = 0u64;
    let mut bytes = 0u64;
    for entry in entries.flatten() {
        match entry.metadata() {
            Ok(metadata) if metadata.is_file() => {
                files += 1;
                bytes += metadata.len();
            }
            Ok(metadata) if metadata.is_dir() => {
                let (sub_files, sub_bytes) = walk(&entry.path());
                files += sub_files;
                bytes += sub_bytes;
            }
            _ => {}
        }
    }
    (files, bytes)
}

pub(crate) fn write(path: &PathBuf, manifest: &Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(manifest).map_err(|error| format!("encode manifest: {error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}
