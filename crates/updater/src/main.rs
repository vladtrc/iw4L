#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Cursor, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::Arc;

use rustls::pki_types::{CertificateDer, ServerName};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const GAME_NAME: &str = "iw4l.exe";
const CA_NAME: &str = "iw4l-ca.pem";
const EMBEDDED_CA_PEM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/iw4l-ca.pem"));
const MAX_DOWNLOAD_BYTES: usize = 512 * 1024 * 1024;
const SIDECAR_MASTER_ENV: [&str; 3] = [
    "IW4L_MASTER_ADDR",
    "IW4L_MASTER_SERVER_NAME",
    "IW4L_MASTER_CA_CERT",
];

#[derive(Debug, Deserialize)]
struct Manifest {
    channel: String,
    protocol: u16,
    git: String,
    env: BTreeMap<String, String>,
    files: Vec<ManifestFile>,
}

#[derive(Debug, Deserialize)]
struct ManifestFile {
    path: String,
    sha256: String,
    size: u64,
    download: Option<String>,
    download_sha256: Option<String>,
    download_size: Option<u64>,
}

struct HttpsTarget {
    host: String,
    port: u16,
    path: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("[iw4launcher] {error}");
            #[cfg(windows)]
            {
                eprintln!("[iw4launcher] press Enter to exit");
                let _ = std::io::stdin().read_line(&mut String::new());
            }
            ExitCode::FAILURE
        }
    }
}

/// Updating is an explicit action. A release archive ships `iw4l.exe`, so the
/// folder runs offline and no build replaces its own binary on its own. The
/// fetch happens only when asked for: `iw4launcher update`, or `IW4L_UPDATE=1`
/// for a shortcut that cannot pass an argument.
fn update_requested() -> bool {
    if std::env::args()
        .skip(1)
        .any(|arg| arg == "update" || arg == "--update")
    {
        return true;
    }
    std::env::var("IW4L_UPDATE").is_ok_and(|value| {
        let value = value.trim();
        !(value.is_empty() || value == "0" || value.eq_ignore_ascii_case("false"))
    })
}

fn run() -> Result<(), String> {
    let install_dir = install_dir()?;
    load_sidecar_env(&install_dir)?;
    println!("[iw4launcher] install {}", install_dir.display());

    let release_env = if update_requested() {
        update(&install_dir)?
    } else {
        if !install_dir.join(GAME_NAME).is_file() {
            return Err(format!(
                "{GAME_NAME} is not in {}. A release archive carries it; to fetch \
                 one instead, run `iw4launcher update` with IW4L_UPDATE_URL set in \
                 the portable .env",
                install_dir.display()
            ));
        }
        println!(
            "[iw4launcher] starting the {GAME_NAME} already here (pass `update` to fetch a new one)"
        );
        BTreeMap::new()
    };
    start_game(&install_dir, &release_env)
}

/// Fetch the manifest, install what it names, and hand back its env defaults.
fn update(install_dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let update_base = update_base()?;
    println!("[iw4launcher] channel {update_base}");

    let manifest_bytes = https_get(&format!("{update_base}/manifest.json"))?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("manifest parse: {error}"))?;
    if manifest.channel.trim().is_empty()
        || manifest.git.trim().is_empty()
        || manifest.protocol == 0
    {
        return Err("manifest identity is incomplete".to_owned());
    }
    for required in [GAME_NAME, CA_NAME] {
        if !manifest.files.iter().any(|entry| entry.path == required) {
            return Err(format!("manifest has no {required}"));
        }
    }

    for entry in &manifest.files {
        install_entry(&update_base, install_dir, entry)?;
    }
    for (name, value) in &manifest.env {
        if !name.starts_with("IW4L_") || name.contains('=') || name.contains('\0') {
            return Err(format!("manifest contains invalid env name `{name}`"));
        }
        if value.contains(['\r', '\n', '\0']) {
            return Err(format!("manifest contains invalid env value for `{name}`"));
        }
    }
    install_default_sidecar(install_dir, &manifest.env)?;
    println!(
        "[iw4launcher] updated to channel={} protocol={} git={}",
        manifest.channel, manifest.protocol, manifest.git
    );
    Ok(manifest.env)
}

fn start_game(install_dir: &Path, release_env: &BTreeMap<String, String>) -> Result<(), String> {
    println!("[iw4launcher] starting {GAME_NAME}");
    let mut game = Command::new(install_dir.join(GAME_NAME));
    game.current_dir(install_dir).arg("menu");
    if std::env::var_os("IW4L_GAMES").is_none() {
        game.env("IW4L_GAMES", install_dir);
    }
    for (name, value) in release_env {
        if std::env::var_os(name).is_none() {
            game.env(name, value);
        }
    }
    game.spawn()
        .map_err(|error| format!("spawn {GAME_NAME}: {error}"))?;
    Ok(())
}

fn load_sidecar_env(install_dir: &Path) -> Result<(), String> {
    let path = install_dir.join(".env");
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        return Err(format!("{} exists but is not a file", path.display()));
    }
    dotenvy::from_path(&path)
        .map(|_| ())
        .map_err(|error| format!("load {}: {error}", path.display()))
}

fn install_default_sidecar(
    install_dir: &Path,
    release_env: &BTreeMap<String, String>,
) -> Result<(), String> {
    let path = install_dir.join(".env");
    if path.is_file() {
        return Ok(());
    }
    if path.exists() {
        return Err(format!("{} exists but is not a file", path.display()));
    }
    let mut contents = String::from(
        "# iw4l portable runtime configuration; local values override release defaults.\n",
    );
    for name in SIDECAR_MASTER_ENV {
        let value = release_env
            .get(name)
            .ok_or_else(|| format!("manifest has no {name}"))?;
        contents.push_str(name);
        contents.push('=');
        contents.push_str(value);
        contents.push('\n');
    }
    if let Ok(url) = std::env::var("IW4L_UPDATE_URL")
        && !url.trim().is_empty()
        && !url.contains(['\r', '\n', '\0'])
    {
        contents.push_str("IW4L_UPDATE_URL=");
        contents.push_str(url.trim());
        contents.push('\n');
    }
    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => file
            .write_all(contents.as_bytes())
            .map_err(|error| format!("write {}: {error}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(format!("create {}: {error}", path.display())),
    }
}

fn update_base() -> Result<String, String> {
    parse_update_url(std::env::var("IW4L_UPDATE_URL").ok())
}

fn parse_update_url(raw: Option<String>) -> Result<String, String> {
    let raw = raw.ok_or_else(|| {
        "IW4L_UPDATE_URL is unset; set it in the portable .env before the first HTTP request"
            .to_owned()
    })?;
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("IW4L_UPDATE_URL is empty".to_owned());
    }
    if trimmed.contains(['\r', '\n', '\0']) {
        return Err("IW4L_UPDATE_URL contains invalid characters".to_owned());
    }
    Ok(trimmed.to_owned())
}

fn require_file_name(name: &str) -> Result<&str, String> {
    if Path::new(name)
        .file_name()
        .and_then(|file_name| file_name.to_str())
        == Some(name)
    {
        Ok(name)
    } else {
        Err(format!("manifest name must be a file name: `{name}`"))
    }
}

fn install_entry(base: &str, dir: &Path, entry: &ManifestFile) -> Result<(), String> {
    require_file_name(&entry.path)?;
    let destination = dir.join(&entry.path);
    let expected = normalized_hash(&entry.sha256)?;
    if destination.is_file()
        && fs::metadata(&destination).map(|meta| meta.len()).ok() == Some(entry.size)
        && file_sha256(&destination)? == expected
    {
        println!("[iw4launcher] {} up to date", entry.path);
        return Ok(());
    }
    let remote = require_file_name(entry.download.as_deref().unwrap_or(&entry.path))?;
    let blob = https_get(&format!("{base}/{remote}"))?;
    if let Some(size) = entry.download_size
        && blob.len() as u64 != size
    {
        return Err(format!("{remote} size {} != {size}", blob.len()));
    }
    if let Some(hash) = &entry.download_sha256
        && bytes_sha256(&blob) != normalized_hash(hash)?
    {
        return Err(format!("{remote} download sha256 mismatch"));
    }
    let bytes = if remote.ends_with(".zst") {
        zstd::decode_all(blob.as_slice()).map_err(|error| format!("decode {remote}: {error}"))?
    } else {
        blob
    };
    if bytes.len() as u64 != entry.size {
        return Err(format!(
            "{} size {} != {}",
            entry.path,
            bytes.len(),
            entry.size
        ));
    }
    if bytes_sha256(&bytes) != expected {
        return Err(format!("{} sha256 mismatch", entry.path));
    }
    atomic_install(&destination, &bytes)?;
    println!("[iw4launcher] installed {}", entry.path);
    Ok(())
}

fn https_get(url: &str) -> Result<Vec<u8>, String> {
    let target = parse_https_url(url)?;
    let mut roots = rustls::RootCertStore::empty();
    let mut reader = BufReader::new(Cursor::new(EMBEDDED_CA_PEM));
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut reader)
        .collect::<Result<_, _>>()
        .map_err(|error| format!("embedded CA parse: {error}"))?;
    if certs.is_empty() {
        return Err(
            "embedded CA contains no certificate; rebuild with `make launcher windows`".to_owned(),
        );
    }
    for cert in certs {
        roots
            .add(cert)
            .map_err(|error| format!("embedded CA: {error}"))?;
    }
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = ServerName::try_from(target.host.clone())
        .map_err(|_| format!("invalid TLS server name `{}`", target.host))?;
    let connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| format!("TLS init: {error}"))?;
    let socket = TcpStream::connect((target.host.as_str(), target.port))
        .map_err(|error| format!("connect {}:{}: {error}", target.host, target.port))?;
    let mut stream = rustls::StreamOwned::new(connection, socket);
    write!(
        stream,
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: iw4launcher/0.1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n",
        target.path, target.host
    )
    .map_err(|error| format!("GET {url}: {error}"))?;
    stream
        .flush()
        .map_err(|error| format!("GET {url}: {error}"))?;
    let mut response = Vec::new();
    stream
        .take((MAX_DOWNLOAD_BYTES + 64 * 1024 + 1) as u64)
        .read_to_end(&mut response)
        .map_err(|error| format!("GET {url}: {error}"))?;
    if response.len() > MAX_DOWNLOAD_BYTES + 64 * 1024 {
        return Err(format!("GET {url}: response exceeds limit"));
    }
    let header_end = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| format!("GET {url}: malformed HTTP response"))?;
    let headers = std::str::from_utf8(&response[..header_end])
        .map_err(|_| format!("GET {url}: non-UTF8 HTTP headers"))?;
    let status = headers
        .lines()
        .next()
        .ok_or_else(|| format!("GET {url}: missing HTTP status"))?;
    if !status.starts_with("HTTP/1.1 2") && !status.starts_with("HTTP/1.0 2") {
        return Err(format!("GET {url}: {status}"));
    }
    if headers.lines().any(|line| {
        line.to_ascii_lowercase().starts_with("transfer-encoding:")
            && line.to_ascii_lowercase().contains("chunked")
    }) {
        return Err(format!("GET {url}: chunked responses are unsupported"));
    }
    let content_length = headers.lines().find_map(|line| {
        line.to_ascii_lowercase()
            .strip_prefix("content-length:")
            .and_then(|value| value.trim().parse::<usize>().ok())
    });
    let body = response.split_off(header_end + 4);
    if let Some(length) = content_length
        && body.len() != length
    {
        return Err(format!("GET {url}: body length {} != {length}", body.len()));
    }
    Ok(body)
}

fn parse_https_url(url: &str) -> Result<HttpsTarget, String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| format!("update URL must use https: `{url}`"))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = authority
        .rsplit_once(':')
        .map_or((authority, 443), |(host, port)| {
            (host, port.parse::<u16>().unwrap_or(0))
        });
    if host.is_empty() || port == 0 {
        return Err(format!("invalid update URL `{url}`"));
    }
    Ok(HttpsTarget {
        host: host.to_owned(),
        port,
        path: format!("/{path}"),
    })
}

fn install_dir() -> Result<PathBuf, String> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "cannot determine updater directory".to_owned())
}

fn normalized_hash(hash: &str) -> Result<String, String> {
    let hash = hash.trim().to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid sha256 `{hash}`"));
    }
    Ok(hash)
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn bytes_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn atomic_install(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    let part = destination.with_extension("part");
    let backup = destination.with_extension("bak");
    {
        let mut file =
            File::create(&part).map_err(|error| format!("create {}: {error}", part.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("write {}: {error}", part.display()))?;
        file.sync_all()
            .map_err(|error| format!("sync {}: {error}", part.display()))?;
    }
    if destination.exists() {
        let _ = fs::remove_file(&backup);
        fs::rename(destination, &backup)
            .map_err(|error| format!("rename {} to backup: {error}", destination.display()))?;
    }
    if let Err(error) = fs::rename(&part, destination) {
        if backup.exists() {
            let _ = fs::rename(&backup, destination);
        }
        return Err(format!("install {}: {error}", destination.display()));
    }
    let _ = fs::remove_file(backup);
    Ok(())
}
