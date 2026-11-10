use std::path::Path;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

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

#[derive(Debug, Clone)]
pub(crate) struct ThreadFacts {
    pub(crate) tid: u32,
    pub(crate) name: String,
    pub(crate) cpus_allowed: Option<String>,
}

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
