//! What the bench manifest can only learn at build time.
//!
//! A run report has to be able to say which source produced the binary that
//! produced it. Asking `git` at runtime cannot: the process may be running from
//! a copied binary, from another checkout, or from a tree that has moved on
//! since it was built, and the answer would name the tree rather than the
//! build. So the revision, the dirty flag, the toolchain and the dependency
//! versions are read here, where they are facts about *this* compilation, and
//! baked in.
//!
//! Everything is optional. A build from a tarball with no `git` and no
//! `Cargo.lock` still compiles; the manifest then says the field is unknown,
//! which is true, instead of guessing.

use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let root = workspace_root();
    emit("IW4L_BUILD_GIT_SHA", git(&root, &["rev-parse", "HEAD"]));
    emit(
        "IW4L_BUILD_GIT_DESCRIBE",
        git(&root, &["describe", "--always", "--dirty", "--tags"]),
    );
    emit("IW4L_BUILD_GIT_DIRTY", git_dirty(&root));
    // A dirty flag says the tree differed from the revision; it does not say
    // whether two dirty runs differed from it in the *same* way. This does:
    // the same patch hash on two runs means the same uncommitted diff. Empty
    // on a clean tree, where `git` prints nothing and the field stays null —
    // "no patch", which `git_dirty` already said.
    emit(
        "IW4L_BUILD_GIT_PATCH_HASH",
        git(&root, &["diff", "HEAD"]).map(|diff| fnv1a_hex(&diff)),
    );
    emit("IW4L_BUILD_RUSTC", rustc_version());
    // Two different answers, and the report needs the first one. `PROFILE` is
    // only ever `debug` or `release` — a build script cannot see that this is
    // `[profile.play]`, which inherits release and is the binary `make bench`
    // actually runs. The target directory carries the real name.
    emit("IW4L_BUILD_PROFILE", profile_dir());
    emit("IW4L_BUILD_PROFILE_KIND", std::env::var("PROFILE").ok());
    emit("IW4L_BUILD_TARGET", std::env::var("TARGET").ok());
    emit("IW4L_BUILD_OPT_LEVEL", std::env::var("OPT_LEVEL").ok());

    let lock = root.join("Cargo.lock");
    let lock_text = std::fs::read_to_string(&lock).ok();
    emit("IW4L_BUILD_LOCK_HASH", lock_text.as_deref().map(fnv1a_hex));
    for package in ["bevy", "wgpu"] {
        emit(
            &format!("IW4L_BUILD_{}_VERSION", package.to_uppercase()),
            lock_text
                .as_deref()
                .and_then(|text| locked_version(text, package)),
        );
    }

    // The revision and the dirty flag are stale the moment a commit lands, and
    // the lock hash the moment a dependency moves. Rebuilding on both is what
    // keeps the manifest describing the binary rather than some earlier one.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", lock.display());
    for head in ["HEAD", "refs"] {
        let path = root.join(".git").join(head);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}

/// The name cargo built under — `play`, `release`, `debug`, or whatever else
/// the invocation named. `OUT_DIR` is `<target>/<profile>/build/<crate>-<hash>/
/// out`, so the profile directory is the fourth ancestor counting `out` itself
/// as the first. `None` if the shape ever changes, which is better than
/// confidently naming the wrong profile.
fn profile_dir() -> Option<String> {
    let out = PathBuf::from(std::env::var("OUT_DIR").ok()?);
    let dir = out.ancestors().nth(3)?;
    let name = dir.file_name()?.to_str()?.to_owned();
    (!name.is_empty()).then_some(name)
}

/// `OUT_DIR` is `<target>/<profile>/build/<crate>-<hash>/out`; the workspace
/// root is the manifest's parent twice over. `CARGO_MANIFEST_DIR` is this
/// crate, so `../..` from `crates/bootstrap`.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").as_deref().unwrap_or(""));
    manifest
        .parent()
        .and_then(Path::parent)
        .map_or(manifest.clone(), Path::to_path_buf)
}

fn emit(key: &str, value: Option<String>) {
    // An absent value is emitted as an empty string rather than left unset, so
    // `env!` in the crate always compiles and the runtime side has exactly one
    // shape of "not known" to handle.
    println!("cargo:rustc-env={key}={}", value.as_deref().unwrap_or(""));
}

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

/// Whether the tracked tree had uncommitted changes at build time. `None` when
/// `git` could not answer at all, which is not the same as "clean".
fn git_dirty(root: &Path) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(if output.stdout.is_empty() {
        "false".to_owned()
    } else {
        "true".to_owned()
    })
}

fn rustc_version() -> Option<String> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
    let output = Command::new(rustc).arg("--version").output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
        .map(|value| value.trim().to_owned())
}

/// The version `Cargo.lock` pins for `package`. The lock is TOML, but this
/// build script has no TOML dependency and does not need one: the file is a
/// flat list of `[[package]]` tables with `name` and `version` on their own
/// lines, so the first `version` after the matching `name` is the answer.
fn locked_version(lock: &str, package: &str) -> Option<String> {
    let wanted = format!("name = \"{package}\"");
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() != wanted {
            continue;
        }
        for line in lines.by_ref().take(4) {
            if let Some(version) = line.trim().strip_prefix("version = ") {
                return Some(version.trim_matches('"').to_owned());
            }
        }
        return None;
    }
    None
}

/// FNV-1a over the lock file. Not a cryptographic digest and not claimed to be
/// one — it answers "is this the same lock as the other run", which is the only
/// question the manifest asks of it, without pulling in a hash crate.
fn fnv1a_hex(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
