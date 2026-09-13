//! `cargo xtask mr fmt <paths>` — rustfmt exactly the files named, nothing else.
//!
//! `cargo fmt --all` (and `-p`) rewrites files the agent does not own, which
//! lands in someone else's diff. Every refusal here exists so the helper cannot
//! quietly guess "my files": no arguments, a directory, a non-Rust path and a
//! path outside the repo are all errors rather than an expansion.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::mrs::install_hook;
use crate::shell::{Res, run, tool_on_path};

/// Matches the `edition` in `Cargo.toml`; rustfmt is invoked directly, so it
/// does not read the workspace manifest to find it.
pub const EDITION: &str = "2024";

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    if !root.join(".git").exists() {
        return Err(format!("not a git repo: {}", root.display()));
    }
    if args.is_empty() {
        return Err(
            "no files. pass an explicit list: cargo xtask mr fmt crates/foo/src/bar.rs".into(),
        );
    }
    if !tool_on_path("rustfmt") {
        return Err("rustfmt not on PATH (rust-toolchain.toml lists the component)".into());
    }

    let files = resolve(root, args)?;
    install_hook(root)?;
    format(&files)?;
    for file in &files {
        let shown = file.strip_prefix(root).unwrap_or(file);
        println!("    formatted {}", shown.display());
    }
    Ok(())
}

/// Every path an internal caller (`ship`) hands over is already repo-relative
/// and known to be `.rs`; it still goes through the same refusals.
pub fn format_paths(root: &Path, rel: &[String]) -> Res<()> {
    let files = resolve(root, rel)?;
    format(&files)
}

fn format(files: &[PathBuf]) -> Res<()> {
    println!("==> rustfmt --edition {EDITION} ({} file(s))", files.len());
    let mut cmd = Command::new("rustfmt");
    cmd.arg("--edition").arg(EDITION).args(files);
    run(&mut cmd)
}

fn resolve(root: &Path, args: &[String]) -> Res<Vec<PathBuf>> {
    let mut files = Vec::with_capacity(args.len());
    for raw in args {
        if raw.is_empty() {
            return Err("empty path".into());
        }
        if raw.starts_with('-') {
            return Err(format!(
                "refusing flag {raw:?} — this is not cargo fmt; pass .rs paths only"
            ));
        }
        if !raw.ends_with(".rs") {
            return Err(format!("not a .rs path: {raw}"));
        }
        let path = if Path::new(raw).is_absolute() {
            PathBuf::from(raw)
        } else {
            root.join(raw)
        };
        if !path.exists() {
            return Err(format!("missing: {raw}"));
        }
        if path.is_dir() {
            return Err(format!("directory refused (no recursive expand): {raw}"));
        }
        let resolved = path
            .canonicalize()
            .map_err(|error| format!("{raw}: {error}"))?;
        if !resolved.is_file() {
            return Err(format!("not a regular file: {raw}"));
        }
        let root_real = root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", root.display()))?;
        if !resolved.starts_with(&root_real) {
            return Err(format!(
                "path escapes repo: {raw} -> {}",
                resolved.display()
            ));
        }
        files.push(resolved);
    }
    Ok(files)
}
