use std::path::{Path, PathBuf};
use std::process::Command;

use master_protocol::PROTOCOL_VERSION;
use serde_json::{Value, json};

use crate::dotenv::Env;
use crate::server::{Descriptor, root_dir};
use crate::shell::{Res, Step, capture, require_tools, run};
use crate::windows;

const ZSTD_LEVEL: &str = "3";
const ZSTD_THREADS: &str = "0";

const ARCHIVE_PASSWORD: &[u8] = b"t.me/contextrot";

pub struct Git {
    pub rev: String,
    pub dirty: bool,
}

pub fn git_identity(root: &Path) -> Res<Git> {
    let rev =
        capture(
            Command::new("git")
                .current_dir(root)
                .args(["rev-parse", "--short=12", "HEAD"]),
        )?
        .trim()
        .to_string();
    let status = capture(
        Command::new("git")
            .current_dir(root)
            .args(["status", "--porcelain"]),
    )?;
    Ok(Git {
        rev,
        dirty: !status.trim().is_empty(),
    })
}

pub fn file_sha256(path: &Path) -> Res<String> {
    use sha2::{Digest as _, Sha256};
    let mut file =
        std::fs::File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    Ok(hex(&hasher.finalize()))
}

pub fn file_size(path: &Path) -> Res<u64> {
    std::fs::metadata(path)
        .map(|meta| meta.len())
        .map_err(|error| format!("stat {}: {error}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    hex(&Sha256::digest(bytes))
}

fn write_json(path: &Path, value: &Value) -> Res<()> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("encoding {}: {error}", path.display()))?;
    text.push('\n');
    std::fs::write(path, text).map_err(|error| format!("writing {}: {error}", path.display()))
}

fn zstd_version() -> Res<String> {
    let text = capture(Command::new("zstd").arg("--version"))?;
    let version = text
        .split('v')
        .find_map(|chunk| {
            let digits: String = chunk
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            (digits.contains('.') && digits.starts_with(|c: char| c.is_ascii_digit()))
                .then_some(digits)
        })
        .ok_or("cannot parse zstd --version")?;
    Ok(version.trim_end_matches('.').to_string())
}

struct Scratch {
    path: PathBuf,
    keep: bool,
}

impl Scratch {
    fn new(under: &Path, tag: &str) -> Res<Self> {
        let path = under.join(format!(".stage-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .map_err(|error| format!("creating {}: {error}", path.display()))?;
        Ok(Self { path, keep: false })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn master_manifest(root: &Path) -> Res<PathBuf> {
    let metadata = capture(Command::new("cargo").current_dir(root).args([
        "metadata",
        "--format-version",
        "1",
        "--locked",
    ]))?;
    let metadata: Value = serde_json::from_str(&metadata).map_err(|error| error.to_string())?;
    let owners: Vec<_> = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata has no packages")?
        .iter()
        .filter(|package| package["name"] == "master_protocol")
        .filter_map(|package| package["manifest_path"].as_str())
        .collect();
    let [owner] = owners.as_slice() else {
        return Err("master build requires exactly one master_protocol dependency owner".into());
    };
    let manifest = capture(Command::new("cargo").current_dir(root).args([
        "locate-project",
        "--workspace",
        "--message-format",
        "plain",
        "--manifest-path",
        owner,
    ]))?;
    Ok(PathBuf::from(manifest.trim()))
}

pub fn build_master(root: &Path, profile: &str) -> Res<PathBuf> {
    windows::require_profile(profile)?;
    require_tools(&["cargo"])?;
    let step = Step::start("build.master", &format!("profile={profile} crt-static"));
    let rustflags = format!(
        "{} -C target-feature=+crt-static",
        std::env::var("RUSTFLAGS").unwrap_or_else(|_| String::new())
    );
    let manifest = master_manifest(root)?;
    let messages = capture(
        Command::new("cargo")
            .current_dir(root)
            .arg("build")
            .arg("--manifest-path")
            .arg(&manifest)
            .args(["--profile", profile, "--locked", "--target"])
            .arg(windows::LINUX_TARGET)
            .args([
                "-p",
                "iw4l-master",
                "--message-format=json-render-diagnostics",
            ])
            .env("RUSTFLAGS", rustflags.trim()),
    )?;
    let bin = windows::cargo_json_bin(&messages, "iw4l-master")?;
    step.done(&format!("bin={}", bin.display()));
    Ok(bin)
}

struct GameBlob {
    name: String,
    exe_sha: String,
    exe_size: u64,
    blob_sha: String,
    blob_size: u64,
}

fn package_game(root: &Path, game_exe: &Path, dest_dir: &Path) -> Res<GameBlob> {
    require_tools(&["zstd"])?;
    let exe_sha = file_sha256(game_exe)?;
    let zstd_ver = zstd_version()?;
    let cache_key = format!("{exe_sha}.zstd-{zstd_ver}.{ZSTD_LEVEL}.T{ZSTD_THREADS}");
    let cache_dir = root.join("dist/cache/zstd");
    std::fs::create_dir_all(&cache_dir)
        .map_err(|error| format!("creating {}: {error}", cache_dir.display()))?;
    let cache_blob = cache_dir.join(format!("{cache_key}.zst"));
    if cache_blob.is_file() {
        println!(
            "package.game: reused sha256={exe_sha} zstd={zstd_ver} -{ZSTD_LEVEL} -T{ZSTD_THREADS}"
        );
    } else {
        let step = Step::start(
            "package.game",
            &format!("zstd -{ZSTD_LEVEL} -T{ZSTD_THREADS} sha256={exe_sha}"),
        );
        let part = cache_blob.with_extension("zst.part");
        run(Command::new("zstd")
            .arg(format!("-{ZSTD_LEVEL}"))
            .arg(format!("-T{ZSTD_THREADS}"))
            .arg("-o")
            .arg(&part)
            .arg(game_exe))?;
        std::fs::rename(&part, &cache_blob)
            .map_err(|error| format!("renaming {}: {error}", part.display()))?;
        step.done("");
    }
    let blob_sha = file_sha256(&cache_blob)?;
    let name = format!("iw4l-{exe_sha}.exe.zst");
    let dest = dest_dir.join(&name);
    if dest.is_file() {
        if file_sha256(&dest)? != blob_sha {
            return Err(format!(
                "existing blob {} does not match cache",
                dest.display()
            ));
        }
    } else {
        std::fs::copy(&cache_blob, &dest)
            .map_err(|error| format!("copying to {}: {error}", dest.display()))?;
    }
    Ok(GameBlob {
        exe_sha,
        exe_size: file_size(game_exe)?,
        blob_sha,
        blob_size: file_size(&dest)?,
        name,
    })
}

fn write_toml(path: &Path, value: &impl serde::Serialize) -> Res<()> {
    let text = toml::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("write {}: {e}", path.display()))
}

fn entry(role: &str, local: &str, remote: String, stage: &Path, immutable: bool) -> Res<Value> {
    let path = stage.join(local);
    Ok(json!({
        "role": role,
        "local": local,
        "remote": remote,
        "sha256": file_sha256(&path)?,
        "size": file_size(&path)?,
        "immutable": immutable,
    }))
}

struct Meta<'a> {
    release_id: &'a str,
    servers: &'a [Descriptor],
    profile: &'a str,
    git: &'a Git,
    master_sha: &'a str,
}

fn release_descriptor(stage: &Path, meta: &Meta<'_>, blob: &GameBlob) -> Res<Value> {
    let Meta {
        release_id,
        servers,
        profile,
        git,
        master_sha,
    } = *meta;
    let mut manifest = entry(
        "manifest",
        "server/updates/manifest.toml",
        format!("manifests/{release_id}.toml"),
        stage,
        false,
    )?;
    manifest["live"] = json!("manifest.toml");
    Ok(json!({
        "id": release_id,
        "servers": servers.iter().map(Descriptor::file_name).collect::<Vec<_>>(),
        "profile": profile,
        "target": windows::TARGET,
        "git": git.rev,
        "dirty": git.dirty,
        "protocol": PROTOCOL_VERSION,
        "files": [
            entry(
                "game-blob",
                &format!("server/updates/{}", blob.name),
                blob.name.clone(),
                stage,
                true,
            )?,
            manifest,
            entry(
                "master",
                "server/iw4l-master",
                format!("masters/{master_sha}/iw4l-master"),
                stage,
                true,
            )?,
        ],
    }))
}

pub struct Bins {
    game: PathBuf,
    master: PathBuf,
}

pub fn build_all(root: &Path, profile: &str) -> Res<Bins> {
    Ok(Bins {
        game: windows::build(profile)?.game,
        master: build_master(root, profile)?,
    })
}

pub fn prepare(root: &Path, servers: &[Descriptor], profile: &str, bins: &Bins) -> Res<PathBuf> {
    let git = git_identity(root)?;
    for server in servers {
        println!("release.server: {}", server.summary());
    }

    let releases = root.join("dist/releases");
    std::fs::create_dir_all(&releases)
        .map_err(|error| format!("creating {}: {error}", releases.display()))?;
    let mut scratch = Scratch::new(&releases, "release")?;
    let stage = scratch.path.clone();
    let client_dir = stage.join("client");
    std::fs::create_dir_all(&client_dir)
        .map_err(|error| format!("creating {}: {error}", client_dir.display()))?;
    let updates_dir = stage.join("server/updates");
    std::fs::create_dir_all(&updates_dir)
        .map_err(|error| format!("creating {}: {error}", updates_dir.display()))?;

    let blob = package_game(root, &bins.game, &updates_dir)?;
    copy(&bins.game, &client_dir.join("iw4l.exe"))?;
    for server in servers {
        write(&client_dir.join(server.file_name()), &server.bytes)?;
    }
    for (from, to) in LEGAL_FILES {
        copy(&root.join(from), &stage.join("server").join(to))?;
    }
    let staged_master = stage.join("server/iw4l-master");
    copy(&bins.master, &staged_master)?;
    set_mode(&staged_master, 0o755)?;
    let master_sha = file_sha256(&staged_master)?;

    let licenses = LEGAL_FILES
        .iter()
        .map(|(from, name)| Ok(json!({ "name": name, "sha256": file_sha256(&root.join(from))? })))
        .collect::<Res<Vec<_>>>()?;
    let descriptors = servers
        .iter()
        .map(|server| json!({ "name": server.file_name(), "sha256": sha256_hex(&server.bytes) }))
        .collect::<Vec<_>>();
    let identity = json!({
        "licenses": licenses,
        "servers": descriptors,
        "dirty": git.dirty,
        "git": git.rev,
        "master_sha256": master_sha,
        "profile": profile,
        "protocol": PROTOCOL_VERSION,
        "target": windows::TARGET,
        "files": {
            "game_blob": blob.blob_sha,
            "game_exe": blob.exe_sha,
        },
    });
    let compact =
        serde_json::to_string(&identity).map_err(|error| format!("encoding identity: {error}"))?;
    let release_id = sha256_hex(compact.as_bytes())[..16].to_string();

    let client_identity = json!({ "exe": blob.exe_sha, "size": blob.exe_size,
        "compressed_size": blob.blob_size, "protocol": PROTOCOL_VERSION });
    let manifest = updater::Manifest {
        schema: 1,
        release: sha256_hex(client_identity.to_string().as_bytes())[..16].into(),
        protocol: PROTOCOL_VERSION,
        file: updater::ManifestFile {
            name: "iw4l.exe".into(),
            path: blob.name.clone(),
            sha256: blob.exe_sha.clone(),
            size: blob.exe_size,
            compressed_size: blob.blob_size,
        },
    };
    write_toml(&updates_dir.join("manifest.toml"), &manifest)?;

    let dest = releases.join(&release_id);
    if dest.join("deployment.json").is_file() {
        println!("release: reused {}", dest.display());
    } else {
        let deployment = release_descriptor(
            &stage,
            &Meta {
                release_id: &release_id,
                servers,
                profile,
                git: &git,
                master_sha: &master_sha,
            },
            &blob,
        )?;
        write_json(&stage.join("deployment.json"), &deployment)?;
        let _ = std::fs::remove_dir_all(&dest);
        std::fs::rename(&stage, &dest)
            .map_err(|error| format!("renaming into {}: {error}", dest.display()))?;
        scratch.keep = true;
        world_readable(&dest)?;
        println!("release: wrote {}", dest.display());
    }
    std::fs::write(releases.join("LATEST"), format!("{release_id}\n"))
        .map_err(|error| format!("writing LATEST: {error}"))?;
    server_archive(
        &dest.join("server"),
        &dest.join(format!("iw4l-server-release-{release_id}.zip")),
    )?;
    let names = servers
        .iter()
        .map(Descriptor::file_name)
        .collect::<Vec<_>>();
    player_archive(
        &dest.join("client"),
        &dest.join("iw4l-windows-community.zip"),
        &names,
    )?;
    player_archive(&dest.join("client"), &dest.join("iw4l-windows.zip"), &[])?;
    println!("release.done path=dist/releases/{release_id}");
    Ok(dest)
}

pub fn release_dir(root: &Path) -> Res<PathBuf> {
    if let Some(explicit) = std::env::var("RELEASE").ok().filter(|v| !v.is_empty()) {
        return Ok(root.join(explicit));
    }
    let releases = root.join("dist/releases");
    let id = std::fs::read_to_string(releases.join("LATEST")).map_err(|_| {
        "no RELEASE= and no dist/releases/LATEST; run make release first".to_string()
    })?;
    Ok(releases.join(id.trim()))
}

const LEGAL_FILES: &[(&str, &str)] = &[
    ("LICENSE", "LICENSE"),
    ("NOTICE", "NOTICE"),
    ("crates/ui/assets/OFL-Oxanium.txt", "OFL-Oxanium.txt"),
    ("crates/console/assets/OFL-FiraMono.txt", "OFL-FiraMono.txt"),
];

pub fn bundles(root: &Path, env: &Env, profile: &str) -> Res<()> {
    let servers = Descriptor::all_in(&root_dir(env)?)?;
    let bins = windows::build(profile)?;

    let out = root.join("dist/windows");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out)
        .map_err(|error| format!("creating {}: {error}", out.display()))?;
    let scratch = Scratch::new(&out, "bundles")?;
    copy(&bins.game, &scratch.path.join("iw4l.exe"))?;
    for server in &servers {
        write(&scratch.path.join(server.file_name()), &server.bytes)?;
    }
    let archive = out.join("iw4l-windows-community.zip");
    let names = servers
        .iter()
        .map(Descriptor::file_name)
        .collect::<Vec<_>>();
    player_archive(&scratch.path, &archive, &names)?;
    println!("[windows] archive: {}", archive.display());
    Ok(())
}

fn player_archive(stage: &Path, archive: &Path, descriptors: &[&str]) -> Res<()> {
    let files = std::iter::once("iw4l.exe")
        .chain(descriptors.iter().copied())
        .map(|name| stage.join(name).display().to_string())
        .collect::<Vec<_>>();
    crate::bundle_zip::write_archive(archive, &files, ARCHIVE_PASSWORD)
}

fn server_archive(stage: &Path, archive: &Path) -> Res<()> {
    use zip::write::SimpleFileOptions;
    let mut writer =
        zip::ZipWriter::new(std::fs::File::create(archive).map_err(|e| e.to_string())?);
    let mut files = vec![stage.join("iw4l-master")];
    files.extend(LEGAL_FILES.iter().map(|(_, name)| stage.join(name)));
    for entry in std::fs::read_dir(stage.join("updates")).map_err(|e| e.to_string())? {
        files.push(entry.map_err(|e| e.to_string())?.path());
    }
    files.sort();
    for path in files {
        let name = path
            .strip_prefix(stage)
            .map_err(|e| e.to_string())?
            .to_string_lossy();
        let mode = if name == "iw4l-master" { 0o755 } else { 0o644 };
        writer
            .start_file(name, SimpleFileOptions::default().unix_permissions(mode))
            .map_err(|e| e.to_string())?;
        std::io::copy(
            &mut std::fs::File::open(path).map_err(|e| e.to_string())?,
            &mut writer,
        )
        .map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Res<()> {
    std::fs::write(path, bytes).map_err(|error| format!("write {}: {error}", path.display()))
}

fn copy(from: &Path, to: &Path) -> Res<()> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|error| format!("copy {} -> {}: {error}", from.display(), to.display()))
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Res<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| format!("chmod {mode:o} {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Res<()> {
    Ok(())
}

#[cfg(unix)]
fn world_readable(path: &Path) -> Res<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let meta = std::fs::symlink_metadata(path)
        .map_err(|error| format!("stat {}: {error}", path.display()))?;
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    let mut mode = meta.permissions().mode();
    mode |= 0o444;
    if meta.is_dir() || mode & 0o100 != 0 {
        mode |= 0o111;
    }
    set_mode(path, mode)?;
    if meta.is_dir() {
        let entries = std::fs::read_dir(path)
            .map_err(|error| format!("reading {}: {error}", path.display()))?;
        for child in entries {
            let child = child.map_err(|error| format!("reading {}: {error}", path.display()))?;
            world_readable(&child.path())?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn world_readable(_path: &Path) -> Res<()> {
    Ok(())
}

pub fn release(root: &Path, env: &Env) -> Res<(Vec<Descriptor>, PathBuf)> {
    let profile = windows::profile(env)?;
    windows::require_profile(&profile)?;
    crate::licenses::check(root)?;
    let servers = Descriptor::all_in(&root_dir(env)?)?;
    let bins = build_all(root, &profile)?;
    let dir = prepare(root, &servers, &profile, &bins)?;
    Ok((servers, dir))
}

pub fn run_cli(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    match args {
        [] => release(root, env).map(|_| ()),
        [arg] if arg == "bundles" => {
            crate::licenses::check(root)?;
            bundles(root, env, &windows::profile(env)?)
        }
        _ => Err("usage: cargo xtask release [bundles]".to_string()),
    }
}
