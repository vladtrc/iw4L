//! Run identity: the facts two reports have to agree on before their numbers
//! can be compared.
//!
//! The manifest already named the revision and the profile. That is not enough
//! to pair two runs: a binary can be rebuilt from the same revision with a
//! different uncommitted patch, a demo can be re-recorded under the same name,
//! and a lock file can move without either. So the binary and the demo are
//! digested — SHA-256, the whole file — and the digest is what the pairing is
//! keyed on.
//!
//! Digesting a release binary is tens of milliseconds to a few hundred, and
//! doing it at exit would charge that to the run it is describing. It happens
//! on its own thread as soon as the bench is inserted; if the run ends before
//! the thread does, the field is null, which is the truth and not a delay.

use std::path::Path;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

/// What the digest thread produced. Absent until it finishes.
static DIGESTS: OnceLock<std::sync::Mutex<Digests>> = OnceLock::new();

#[derive(Debug, Default, Clone)]
pub(crate) struct Digests {
    pub(crate) binary: Option<String>,
    pub(crate) demo: Option<String>,
    pub(crate) lock: Option<String>,
}

fn slot() -> &'static std::sync::Mutex<Digests> {
    DIGESTS.get_or_init(|| std::sync::Mutex::new(Digests::default()))
}

/// Start digesting the binary, the demo and the lock file. Returns at once.
pub(crate) fn spawn(demo: Option<&Path>, root: &Path) {
    let binary = std::env::current_exe().ok();
    let demo = demo.map(Path::to_path_buf);
    let lock = root.join("Cargo.lock");
    let _ = std::thread::Builder::new()
        .name("iw4l bench digest".to_owned())
        .spawn(move || {
            let digests = Digests {
                binary: binary.as_deref().and_then(sha256_file),
                demo: demo.as_deref().and_then(sha256_file),
                lock: sha256_file(&lock),
            };
            *slot().lock().unwrap_or_else(|poison| poison.into_inner()) = digests;
        });
}

/// What has been digested so far. A field that is still being read, or that
/// could not be read at all, is `None`.
pub(crate) fn digests() -> Digests {
    slot()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

fn sha256_file(path: &Path) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).ok()?;
    Some(format!("{:x}", hasher.finalize()))
}

/// One OS thread of this process, as the kernel describes it.
#[derive(Debug, Clone)]
pub(crate) struct ThreadFacts {
    pub(crate) tid: u32,
    pub(crate) name: String,
    /// The CPUs this thread may run on, in the kernel's list notation.
    pub(crate) cpus_allowed: Option<String>,
}

/// Every thread, with its name and its actual affinity.
///
/// Read from `/proc` rather than sampled by scheduling a task onto each pool:
/// a sampled task lands on whichever worker the pool picked, so it can say
/// "some worker of this pool has this mask" and never "every one of them
/// does". `/proc/self/task` is the whole set, which is the question — the load
/// pool pins its workers to the process's CPUs, and nothing else here proves
/// that it worked.
#[cfg(target_os = "linux")]
pub(crate) fn threads() -> Vec<ThreadFacts> {
    let Ok(entries) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let Ok(tid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let status =
            std::fs::read_to_string(entry.path().join("status")).unwrap_or_else(|_| String::new());
        let field = |key: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .map(|value| value.trim().to_owned())
        };
        out.push(ThreadFacts {
            tid,
            name: field("Name:").unwrap_or_else(|| "<unnamed>".to_owned()),
            cpus_allowed: field("Cpus_allowed_list:"),
        });
    }
    out.sort_by_key(|thread| thread.tid);
    out
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn threads() -> Vec<ThreadFacts> {
    Vec::new()
}

/// The main thread's CPU mask, as `/proc/self/status` reports it.
///
/// Not "the process's" mask: Linux has no such thing — affinity is per thread,
/// `/proc/self/status` is the thread group leader's, and this process narrows
/// that leader to the performance cores at startup while deliberately handing
/// the load pool the wider set it began with. Calling this the process mask
/// would turn that design into a contradiction in the manifest. The per-thread
/// table beside it is the whole answer.
#[cfg(target_os = "linux")]
pub(crate) fn main_thread_cpus_allowed() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
        .map(|value| value.trim().to_owned())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn main_thread_cpus_allowed() -> Option<String> {
    None
}
