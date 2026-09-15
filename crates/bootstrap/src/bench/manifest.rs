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
    pub(crate) present_mode: Option<String>,
    /// Threads in each Bevy pool, by pool name.
    pub(crate) pools: Vec<(String, usize)>,
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
            "bevy": built!("IW4L_BUILD_BEVY_VERSION"),
            "wgpu": built!("IW4L_BUILD_WGPU_VERSION"),
        },
        "binary": binary(),
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
        "present_mode": facts.present_mode,
        "pools": facts
            .pools
            .iter()
            .map(|(name, threads)| json!({ "pool": name, "threads": threads }))
            .collect::<Vec<_>>(),
        "workload": {
            "zone": facts.zone,
            "demo": facts.demo,
            "role": facts.role,
            "command_line": std::env::args().collect::<Vec<_>>(),
        },
        "env": toggles(),
        "caches": caches(artifacts),
    })
}

fn dirty() -> Value {
    match env!("IW4L_BUILD_GIT_DIRTY") {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::Null,
    }
}

/// The binary this process is running, by path, size and mtime. Enough to tell
/// two runs of "the same" build apart when one of them was rebuilt in between;
/// not a content hash, because reading a release binary at exit to digest it
/// costs more than the fact is worth.
fn binary() -> Value {
    let Ok(path) = std::env::current_exe() else {
        return Value::Null;
    };
    let metadata = std::fs::metadata(&path).ok();
    json!({
        "path": path.display().to_string(),
        "bytes": metadata.as_ref().map(std::fs::Metadata::len),
        "modified_unix_s": metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|since| since.as_secs()),
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

fn prepared_cache(artifacts: &Path) -> Value {
    let dir = artifacts.join("cache");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return json!({ "path": dir.display().to_string(), "present": false });
    };
    let mut files = 0u64;
    let mut bytes = 0u64;
    for entry in entries.flatten() {
        if let Ok(metadata) = entry.metadata()
            && metadata.is_file()
        {
            files += 1;
            bytes += metadata.len();
        }
    }
    json!({
        "path": dir.display().to_string(),
        "present": true,
        "files": files,
        "bytes": bytes,
    })
}

pub(crate) fn write(path: &PathBuf, manifest: &Value) -> Result<(), String> {
    let bytes =
        serde_json::to_vec_pretty(manifest).map_err(|error| format!("encode manifest: {error}"))?;
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}
