use community::{Community, CommunityFile, trust_roots, update_url};
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Result<T> = std::result::Result<T, String>;
const MAX_BINARY: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST: u64 = 64 * 1024;
const SELECTION_FILE: &str = "community.json";

/// Descriptors live in the games root; the choice lives in the artifacts dir.
struct CommunityRoots {
    games: PathBuf,
    artifacts: PathBuf,
}

static ROOTS: OnceLock<CommunityRoots> = OnceLock::new();

struct CommunityFiles {
    entries: Vec<CommunityFile>,
    selected: Option<PathBuf>,
}

fn read_community(path: &Path) -> Result<Community> {
    let bytes = read_bounded(
        File::open(path).map_err(|e| format!("{}: {e}", path.display()))?,
        MAX_MANIFEST,
    )?;
    Community::parse(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

fn discover_communities(roots: &CommunityRoots) -> Result<CommunityFiles> {
    let root = &roots.games;
    let mut paths: Vec<_> = fs::read_dir(root)
        .map_err(|e| e.to_string())?
        .map(|entry| entry.map(|entry| entry.path()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "iw4l-server"))
        .collect();
    let saved = match File::open(roots.artifacts.join(SELECTION_FILE)) {
        Ok(file) => match read_bounded(file, MAX_MANIFEST)
            .and_then(|bytes| serde_json::from_slice::<PathBuf>(&bytes).map_err(|e| e.to_string()))
        {
            Ok(path) => Some(root.join(path)),
            Err(error) => {
                println!("[iw4l] cannot read community selection: {error}");
                None
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            println!("[iw4l] cannot read community selection: {error}");
            None
        }
    };
    if let Some(path) = &saved
        && !paths.contains(path)
        && path.is_file()
    {
        paths.push(path.clone());
    }
    let mut entries = Vec::new();
    for path in paths {
        match read_community(&path) {
            Ok(community) => entries.push(CommunityFile {
                path,
                name: community.name,
            }),
            Err(error) => println!("[iw4l] skipping {}: {error}", path.display()),
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.path.cmp(&b.path)));
    let selected = saved
        .filter(|path| entries.iter().any(|entry| entry.path == *path))
        .or_else(|| (entries.len() == 1).then(|| entries[0].path.clone()));
    Ok(CommunityFiles { entries, selected })
}

fn restart_with_community(path: &Path) -> Result<()> {
    read_community(path)?;
    let roots = ROOTS.get().ok_or("community roots are not installed")?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let selection = roots.artifacts.join(SELECTION_FILE);
    let previous = match fs::read(&selection) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("read community selection: {error}")),
    };
    let payload = serde_json::to_vec(path.strip_prefix(&roots.games).unwrap_or(path))
        .map_err(|e| e.to_string())?;
    save_selection(&roots.artifacts, &payload)?;
    if let Err(error) = Command::new(&exe)
        .arg("menu")
        .env_remove("IW4L_MASTER_HOST_NAME")
        .env_remove("IW4L_MASTER_JOIN")
        .env_remove("IW4L_CMDS")
        .stdin(std::process::Stdio::null())
        .spawn()
    {
        let rollback = match previous {
            Some(bytes) => save_selection(&roots.artifacts, &bytes),
            None => fs::remove_file(selection).map_err(|e| e.to_string()),
        };
        rollback.map_err(|rollback| format!("restart: {error}; restore selection: {rollback}"))?;
        return Err(format!("restart community: {error}"));
    }
    Ok(())
}

fn save_selection(artifacts: &Path, payload: &[u8]) -> Result<()> {
    let temp = artifacts.join(format!("community.{}.tmp", std::process::id()));
    let result = write_synced(&temp, payload).and_then(|()| {
        fs::rename(&temp, artifacts.join(SELECTION_FILE)).map_err(|e| e.to_string())
    });
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result.map_err(|e| format!("save community selection: {e}"))
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub release: String,
    pub protocol: u16,
    pub file: ManifestFile,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub size: u64,
    pub compressed_size: u64,
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() as u64 > MAX_MANIFEST {
            return Err("update manifest exceeds limit".into());
        }
        let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
        let manifest: Self = toml::from_str(text).map_err(|e| format!("update manifest: {e}"))?;
        let file = &manifest.file;
        if manifest.schema != 1 || manifest.release.trim().is_empty() || manifest.protocol == 0 {
            return Err("unsupported or incomplete update manifest".into());
        }
        if file.name != "iw4l.exe"
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || file.path != format!("iw4l-{}.exe.zst", file.sha256.to_ascii_lowercase())
            || file.size == 0
            || file.size > MAX_BINARY
            || file.compressed_size == 0
            || file.compressed_size > MAX_BINARY
        {
            return Err("invalid update file identity or size".into());
        }
        Ok(manifest)
    }
}

pub fn startup(games: &Path, artifacts: &Path) -> Result<Option<Vec<OsString>>> {
    let mut args: Vec<OsString> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--iw4l-replace") {
        if args.len() != 2 {
            return Err("replacement helper requires one job path".into());
        }
        replace(Path::new(&args[1]))?;
        return Ok(None);
    }
    let restarted = args.first().is_some_and(|a| a == "--iw4l-updated");
    if restarted {
        args.remove(0);
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let root = exe.parent().ok_or("executable has no directory")?;
    let roots = ROOTS.get_or_init(|| CommunityRoots {
        games: games.to_owned(),
        artifacts: artifacts.to_owned(),
    });
    let files = discover_communities(roots)?;
    let descriptor = files.selected.clone();
    let selected = descriptor.as_deref().map(read_community).transpose()?;
    community::install_context(
        community::CommunityContext {
            entries: files.entries,
            selected_path: descriptor,
            selected: selected.clone(),
        },
        restart_with_community,
    )?;
    let Some(community) = selected else {
        return Ok(Some(args));
    };
    if !cfg!(windows) {
        return Ok(Some(args));
    }
    update_log(&format!("checking updates for {}", community.name));
    match check_update(&exe, root, &community, &args, restarted) {
        Ok(true) => Ok(None),
        Ok(false) => Ok(Some(args)),
        Err(error) => {
            let message =
                format!("update failed: {error}; starting game with the installed version");
            diag::announce_stdout(&format!("[iw4l] {message}"));
            diag::warn!(Launch, "{message}");
            Ok(Some(args))
        }
    }
}

fn update_log(message: &str) {
    diag::announce_stdout(&format!("[iw4l] {message}"));
    diag::info!(Launch, "{message}");
}

fn check_update(
    exe: &Path,
    root: &Path,
    community: &Community,
    args: &[OsString],
    restarted: bool,
) -> Result<bool> {
    let mut stage = "acquire update lock";
    let result = (|| {
        let lock = open_update_lock(root)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if lock.try_lock_exclusive().is_ok() {
                break;
            }
            if !restarted || Instant::now() >= deadline {
                return Err("another iw4l process is checking or applying an update".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        stage = "clean up previous update helper";
        cleanup_helper(root)?;
        stage = "resolve update host and configure TLS";
        let (client, url) = client(community)?;
        stage = "fetch update manifest";
        let manifest =
            Manifest::parse(&get(&client, url.clone(), MAX_MANIFEST, "manifest", None)?)?;
        update_log(&format!(
            "release {}: {} compressed bytes, {} executable bytes",
            manifest.release, manifest.file.compressed_size, manifest.file.size
        ));
        stage = "verify installed executable";
        update_log("checking installed executable size and SHA-256");
        if fs::metadata(exe).map_err(|e| e.to_string())?.len() == manifest.file.size
            && file_sha256(exe)? == manifest.file.sha256.to_ascii_lowercase()
        {
            let _ = fs::remove_file(root.join("iw4l.previous.exe"));
            update_log("installed executable is up to date; starting game");
            return Ok(false);
        }
        stage = "download release";
        update_log(&format!("downloading release {}", manifest.release));
        let blob_url = url.join(&manifest.file.path).map_err(|e| e.to_string())?;
        let blob = get(
            &client,
            blob_url,
            manifest.file.compressed_size,
            "release",
            Some(manifest.file.compressed_size),
        )?;
        stage = "decompress release";
        update_log("decompressing release");
        let mut decoder =
            zstd::stream::read::Decoder::new(blob.as_slice()).map_err(|e| e.to_string())?;
        decoder.window_log_max(29).map_err(|e| e.to_string())?;
        let decoded = read_bounded(decoder, manifest.file.size)?;
        stage = "verify downloaded executable";
        update_log("verifying downloaded executable size and SHA-256");
        if decoded.len() as u64 != manifest.file.size {
            return Err(format!(
                "executable size mismatch: expected {} bytes, received {}",
                manifest.file.size,
                decoded.len()
            ));
        }
        let actual_hash = hash(&decoded);
        if actual_hash != manifest.file.sha256.to_ascii_lowercase() {
            return Err(format!(
                "executable SHA-256 mismatch: expected {}, received {actual_hash}",
                manifest.file.sha256
            ));
        }
        stage = "stage verified executable";
        update_log("writing verified executable to iw4l.update.exe");
        let staged = root.join("iw4l.update.exe");
        write_synced(&staged, &decoded)?;
        fs::set_permissions(
            &staged,
            fs::metadata(exe).map_err(|e| e.to_string())?.permissions(),
        )
        .map_err(|e| e.to_string())?;
        stage = "prepare replacement helper";
        update_log("preparing replacement helper");
        let temp = std::env::temp_dir().join(format!(
            "iw4l-update-{}-{}",
            std::process::id(),
            &manifest.file.sha256[..16]
        ));
        fs::create_dir(&temp).map_err(|e| format!("create helper directory: {e}"))?;
        let helper = temp.join("iw4l-update-helper.exe");
        fs::copy(exe, &helper).map_err(|e| e.to_string())?;
        let job = Job {
            pid: std::process::id(),
            target: exe.to_path_buf(),
            source: staged,
            sha256: manifest.file.sha256,
            size: manifest.file.size,
            args: args
                .iter()
                .map(|a| {
                    a.to_str()
                        .map(str::to_owned)
                        .ok_or("non-UTF8 launch argument")
                })
                .collect::<std::result::Result<_, _>>()?,
            cwd: std::env::current_dir().map_err(|e| e.to_string())?,
        };
        let job_path = temp.join("job.json");
        write_synced(
            &job_path,
            &serde_json::to_vec(&job).map_err(|e| e.to_string())?,
        )?;
        write_synced(
            &root.join("iw4l.update-helper"),
            temp.to_str().ok_or("non-UTF8 helper directory")?.as_bytes(),
        )?;
        stage = "start replacement helper";
        update_log("starting replacement helper and waiting for handoff");
        let mut child = Command::new(helper)
            .arg("--iw4l-replace")
            .arg(&job_path)
            .stdin(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let handoff = (|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !temp.join("ready").is_file() {
                if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                    return Err(format!("update helper exited before handoff: {status}"));
                }
                if Instant::now() >= deadline {
                    return Err("update helper handoff timeout".into());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        })();
        if let Err(error) = handoff {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        update_log("replacement helper ready; handing off to install and restart");
        Ok(true)
    })();
    result.map_err(|error| format!("{stage}: {error}"))
}

fn cleanup_helper(root: &Path) -> Result<()> {
    let marker = root.join("iw4l.update-helper");
    if !marker.exists() {
        return Ok(());
    }
    let bytes = read_bounded(
        File::open(&marker).map_err(|e| e.to_string())?,
        MAX_MANIFEST,
    )?;
    let temp = PathBuf::from(std::str::from_utf8(&bytes).map_err(|e| e.to_string())?);
    if temp.parent() != Some(std::env::temp_dir().as_path())
        || !temp
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("iw4l-update-"))
    {
        return Err("invalid update helper cleanup directory".into());
    }
    for _ in 0..30 {
        if fs::remove_dir_all(&temp).is_ok() || !temp.exists() {
            fs::remove_file(&marker).map_err(|e| e.to_string())?;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn client(community: &Community) -> Result<(reqwest::blocking::Client, reqwest::Url)> {
    let mut url = update_url(community)?;
    let port = url
        .port_or_known_default()
        .ok_or("update URL has no port")?;
    update_log(&format!(
        "resolving update host {}:{port} (DNS timeout 8s)",
        url.host_str().ok_or("missing update host")?
    ));
    let host = url.host_str().ok_or("missing update host")?.to_owned();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let lookup_host = host.clone();
    std::thread::spawn(move || {
        let addresses = (lookup_host.as_str(), port)
            .to_socket_addrs()
            .map(|addresses| addresses.collect::<Vec<_>>());
        let _ = sender.send(addresses);
    });
    let addresses = receiver
        .recv_timeout(Duration::from_secs(8))
        .map_err(|e| format!("DNS lookup {host}:{port} (8s timeout): {e}"))?
        .map_err(|e| format!("DNS lookup {host}:{port}: {e}"))?;
    if addresses.is_empty() {
        return Err("update host resolved no addresses".into());
    }
    update_log(&format!(
        "resolved update addresses: {addresses:?}; TLS server name: {}",
        community.master.server_name
    ));
    let roots = trust_roots(&community.updates.ca_pem)?;
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    url.set_host(Some(&community.master.server_name))
        .map_err(|e| e.to_string())?;
    let client = reqwest::blocking::Client::builder()
        .use_preconfigured_tls(tls)
        .https_only(true)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&community.master.server_name, &addresses)
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(180))
        .user_agent("iw4l-updater/1")
        .build()
        .map_err(|e| e.to_string())?;
    Ok((client, url))
}

fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    message
}

fn get(
    client: &reqwest::blocking::Client,
    url: reqwest::Url,
    cap: u64,
    label: &str,
    expected: Option<u64>,
) -> Result<Vec<u8>> {
    let started = Instant::now();
    update_log(&format!(
        "requesting {label}: {url} (connect timeout 8s, request timeout 180s)"
    ));
    let mut response = client
        .get(url.clone())
        .header(reqwest::header::ACCEPT_ENCODING, "identity")
        .send()
        .map_err(|e| format!("GET {url}: {}", error_chain(&e)))?;
    update_log(&format!(
        "{label}: HTTP {} after {:.1}s, content length {:?}",
        response.status(),
        started.elapsed().as_secs_f64(),
        response.content_length()
    ));
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("GET {url}: HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|size| size > cap) {
        return Err(format!("GET {url}: response exceeds {cap} byte limit"));
    }
    let total = expected.or(response.content_length());
    let mut bytes = Vec::new();
    let mut buffer = [0; 64 * 1024];
    let mut last_progress = Instant::now();
    update_log(&format!("{label}: download started, 0 bytes received"));
    loop {
        let remaining = (cap + 1 - bytes.len() as u64).min(buffer.len() as u64) as usize;
        let count = response.read(&mut buffer[..remaining]).map_err(|e| {
            format!(
                "GET {url}: read failed after {} bytes in {:.1}s: {}",
                bytes.len(),
                started.elapsed().as_secs_f64(),
                error_chain(&e)
            )
        })?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() as u64 > cap {
            return Err(format!("GET {url}: response exceeds {cap} byte limit"));
        }
        if last_progress.elapsed() >= Duration::from_secs(1) {
            log_progress(label, bytes.len() as u64, total, started.elapsed());
            last_progress = Instant::now();
        }
    }
    if let Some(expected) = total
        && bytes.len() as u64 != expected
    {
        return Err(format!(
            "GET {url}: incomplete download: received {} of {expected} bytes in {:.1}s",
            bytes.len(),
            started.elapsed().as_secs_f64()
        ));
    }
    log_progress(label, bytes.len() as u64, total, started.elapsed());
    update_log(&format!("{label}: download complete"));
    Ok(bytes)
}

fn log_progress(label: &str, received: u64, total: Option<u64>, elapsed: Duration) {
    let size = match total.filter(|&total| total > 0) {
        Some(total) => format!(
            "{received}/{total} bytes ({:.1}%)",
            received as f64 * 100.0 / total as f64
        ),
        None => format!("{received} bytes"),
    };
    update_log(&format!(
        "{label}: {size}, {:.1}s elapsed, {:.1} KiB/s",
        elapsed.as_secs_f64(),
        received as f64 / 1024.0 / elapsed.as_secs_f64().max(0.001)
    ));
}

fn read_bounded(reader: impl Read, cap: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(cap + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > cap {
        return Err("update input exceeds limit".into());
    }
    Ok(bytes)
}

pub fn file_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    std::io::copy(&mut file, &mut digest).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", digest.finalize()))
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn write_synced(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = File::create(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    file.sync_all().map_err(|e| e.to_string())
}

fn open_update_lock(root: &Path) -> Result<File> {
    let artifacts = root.join("iw4l-artifacts");
    fs::create_dir_all(&artifacts).map_err(|e| e.to_string())?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(artifacts.join("iw4l.update.lock"))
        .map_err(|e| e.to_string())
}

#[derive(Serialize, Deserialize)]
struct Job {
    pid: u32,
    target: PathBuf,
    source: PathBuf,
    sha256: String,
    size: u64,
    args: Vec<String>,
    cwd: PathBuf,
}

fn replace(path: &Path) -> Result<()> {
    let job: Job = serde_json::from_slice(&read_bounded(
        File::open(path).map_err(|e| e.to_string())?,
        MAX_MANIFEST,
    )?)
    .map_err(|e| e.to_string())?;
    let temp = path.parent().ok_or("helper job has no directory")?;
    let root = job
        .target
        .parent()
        .ok_or("update target has no directory")?;
    if job.source != root.join("iw4l.update.exe") || job.source == job.target {
        return Err("invalid replacement source".into());
    }
    wait_parent(job.pid, &temp.join("ready"))?;
    let lock = open_update_lock(root)?;
    lock.try_lock_exclusive()
        .map_err(|_| "another process interrupted replacement")?;
    if fs::metadata(&job.source).map_err(|e| e.to_string())?.len() != job.size
        || file_sha256(&job.source)? != job.sha256.to_ascii_lowercase()
    {
        return Err("staged replacement verification failed".into());
    }
    let backup = root.join("iw4l.previous.exe");
    if backup.exists() {
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
    }
    fs::rename(&job.target, &backup).map_err(|e| format!("backup executable: {e}"))?;
    if let Err(e) = fs::rename(&job.source, &job.target) {
        fs::rename(&backup, &job.target)
            .map_err(|rollback| format!("install: {e}; rollback: {rollback}"))?;
        return Err(format!("install update: {e}"));
    }
    // Keep the lock through spawning and rollback; the restarted process waits for this handoff.
    if let Err(e) = Command::new(&job.target)
        .arg("--iw4l-updated")
        .args(&job.args)
        .current_dir(&job.cwd)
        .spawn()
    {
        fs::remove_file(&job.target)
            .map_err(|rollback| format!("restart: {e}; remove failed update: {rollback}"))?;
        fs::rename(&backup, &job.target)
            .map_err(|rollback| format!("restart: {e}; restore: {rollback}"))?;
        return Err(format!("restart update: {e}; previous executable restored"));
    }
    Ok(())
}

#[cfg(windows)]
fn wait_parent(pid: u32, ready: &Path) -> Result<()> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // Open the handle before acknowledging handoff so PID reuse cannot change the process we wait for.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return Err(format!(
            "open update parent: {}",
            std::io::Error::last_os_error()
        ));
    }
    let result = (|| {
        write_synced(ready, b"ready")?;
        if unsafe { WaitForSingleObject(handle, 120_000) } != WAIT_OBJECT_0 {
            return Err("update parent wait failed or timed out".into());
        }
        Ok(())
    })();
    unsafe {
        CloseHandle(handle);
    }
    result
}

#[cfg(not(windows))]
fn wait_parent(pid: u32, ready: &Path) -> Result<()> {
    write_synced(ready, b"ready")?;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let running = fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .is_some_and(|stat| {
                !stat
                    .rsplit_once(')')
                    .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
            });
        if !running {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("update parent wait timeout".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
