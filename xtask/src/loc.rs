//! The workspace Rust footprint: per-crate files/lines/bytes, the retail-facts
//! vs host split, and the heaviest files. Behind `make loc`.
//!
//! Counts `crates/` and `xtask` only — never `target/`, and never the game's
//! own content.

use std::path::{Path, PathBuf};

use crate::shell::Res;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// A crate transcribing retail facts: `*_iw4`, `*_iw5`, `*_t5`.
    Retail,
    Host,
    Xtask,
}

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::Retail => "re",
            Self::Host => "host",
            Self::Xtask => "xtask",
        }
    }
}

struct Crate {
    name: String,
    kind: Kind,
    files: usize,
    lines: usize,
    bytes: u64,
}

struct Source {
    path: PathBuf,
    lines: usize,
    bytes: u64,
}

fn human(bytes: u64) -> String {
    let value = bytes as f64;
    if bytes >= 1024 * 1024 {
        format!("{:.1}M", value / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1}K", value / 1024.0)
    } else {
        format!("{bytes}B")
    }
}

fn collect(dir: &Path, out: &mut Vec<Source>) -> Res<()> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("reading {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("reading {}: {error}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("stat {}: {error}", path.display()))?;
        if file_type.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect(&path, out)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            let text = std::fs::read(&path)
                .map_err(|error| format!("reading {}: {error}", path.display()))?;
            out.push(Source {
                lines: text.iter().filter(|byte| **byte == b'\n').count(),
                bytes: text.len() as u64,
                path,
            });
        }
    }
    Ok(())
}

fn kind_of(name: &str) -> Kind {
    if name.ends_with("_iw4") || name.ends_with("_iw5") || name.ends_with("_t5") {
        Kind::Retail
    } else {
        Kind::Host
    }
}

pub fn run_cli(root: &Path) -> Res<()> {
    let mut crates = Vec::new();
    let mut all = Vec::new();

    let mut dirs: Vec<(String, Kind, PathBuf)> = std::fs::read_dir(root.join("crates"))
        .map_err(|error| format!("reading crates/: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = kind_of(&name);
            (name, kind, entry.path())
        })
        .collect();
    dirs.push(("xtask".to_string(), Kind::Xtask, root.join("xtask")));

    for (name, kind, path) in dirs {
        let mut sources = Vec::new();
        collect(&path, &mut sources)?;
        if sources.is_empty() {
            continue;
        }
        crates.push(Crate {
            name,
            kind,
            files: sources.len(),
            lines: sources.iter().map(|source| source.lines).sum(),
            bytes: sources.iter().map(|source| source.bytes).sum(),
        });
        all.append(&mut sources);
    }
    crates.sort_by_key(|item| std::cmp::Reverse(item.lines));

    let rule = "-".repeat(64);
    println!(
        "{:<24} {:>6} {:>8} {:>8}  kind",
        "crate", "files", "lines", "size"
    );
    println!("{rule}");
    for item in &crates {
        println!(
            "{:<24} {:>6} {:>8} {:>8}  {}",
            item.name,
            item.files,
            item.lines,
            human(item.bytes),
            item.kind.label()
        );
    }
    println!("{rule}");
    let total_files: usize = crates.iter().map(|item| item.files).sum();
    let total_lines: usize = crates.iter().map(|item| item.lines).sum();
    let total_bytes: u64 = crates.iter().map(|item| item.bytes).sum();
    println!(
        "{:<24} {total_files:>6} {total_lines:>8} {:>8}",
        "TOTAL",
        human(total_bytes)
    );

    println!();
    println!(
        "{:<24} {:>6} {:>8} {:>8}  crates",
        "group", "files", "lines", "size"
    );
    println!("{rule}");
    for (label, kind) in [
        ("retail facts", Kind::Retail),
        ("host", Kind::Host),
        ("xtask", Kind::Xtask),
    ] {
        let group: Vec<&Crate> = crates.iter().filter(|item| item.kind == kind).collect();
        println!(
            "{label:<24} {:>6} {:>8} {:>8}  {}",
            group.iter().map(|item| item.files).sum::<usize>(),
            group.iter().map(|item| item.lines).sum::<usize>(),
            human(group.iter().map(|item| item.bytes).sum()),
            group.len()
        );
    }

    println!();
    println!("Largest .rs files (top 15 by lines):");
    println!("{rule}");
    all.sort_by_key(|source| std::cmp::Reverse(source.lines));
    for source in all.iter().take(15) {
        println!("{:>8}  {}", source.lines, relative(root, &source.path));
    }

    println!();
    println!("Largest .rs files (top 10 by bytes):");
    println!("{rule}");
    all.sort_by_key(|source| std::cmp::Reverse(source.bytes));
    for source in all.iter().take(10) {
        println!(
            "{:>8}  {}",
            human(source.bytes),
            relative(root, &source.path)
        );
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}
