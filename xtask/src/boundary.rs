//! `cargo xtask boundary` — keeps each game's rules inside that game's crates.
//!
//! Every workspace crate declares `[package.metadata.iw4l] layer` (`neutral`,
//! `format`, `game`, `session`, `mixed`) and, for `format`/`game`, the `game` it
//! belongs to. Three checks:
//!
//! * graph — a game crate depends only on neutral crates and its own game's
//!   format/game crates; a format crate on neutral crates and its own game's
//!   format crates; a neutral crate on neutral and format crates;
//! * patterns — outside game/format crates, no code names a game: game enum
//!   variants, `GameModeKind::Zombies`, a game crate's path, a game literal,
//!   `is_<game>` helpers. The session's registration point is the exception;
//! * ledger — every `unknown!("<id>", ..)` in a game crate is listed in
//!   `docs/fidelity/<game>.md`.
//!
//! What exists today is recorded in `xtask/boundary/allow.txt`. The file is a
//! ratchet: a violation it does not list is new, an entry that no longer
//! occurs as often is stale, and `--update` only ever lowers it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::shell::Res;

const ALLOW: &str = "xtask/boundary/allow.txt";
const REGISTRATION: &str = "crates/session/src/games.rs";
/// The game identity itself has to name the games.
const IDENTITY: &str = "crates/asset_core/src/family.rs";
/// This check has to spell the shapes it hunts for.
const SELF: &str = "xtask/src/boundary.rs";
const GAMES: [&str; 4] = ["iw4", "t5", "iw5", "t6"];
const GAME_VARIANTS: [&str; 4] = ["Iw4", "T5", "Iw5", "T6"];
const GAME_ENUMS: [&str; 4] = ["ZoneGame", "AssetNamespace", "FamilyId", "Realm"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layer {
    Neutral,
    Format,
    Game,
    Session,
    Mixed,
}

impl Layer {
    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "neutral" => Self::Neutral,
            "format" => Self::Format,
            "game" => Self::Game,
            "session" => Self::Session,
            "mixed" => Self::Mixed,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Format => "format",
            Self::Game => "game",
            Self::Session => "session",
            Self::Mixed => "mixed",
        }
    }
}

struct Package {
    name: String,
    ident: String,
    layer: Option<Layer>,
    game: Option<String>,
    dir: PathBuf,
    deps: Vec<String>,
}

/// Violation key → occurrences. Keys are `edge <from> <to>`, `mixed <crate>`
/// and `pattern <file> <shape>`.
type Findings = BTreeMap<String, usize>;

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let enforce = args.iter().any(|a| a == "--enforce");
    let update = args.iter().any(|a| a == "--update");
    if let Some(other) = args
        .iter()
        .find(|a| !matches!(a.as_str(), "--enforce" | "--update"))
    {
        return Err(format!(
            "unknown argument {other}; usage: boundary [--enforce] [--update]"
        ));
    }
    let packages = workspace(root)?;
    let mut errors = Vec::new();
    let mut findings = Findings::new();

    let by_name: BTreeMap<&str, &Package> = packages.iter().map(|p| (p.name.as_str(), p)).collect();
    for package in &packages {
        let Some(layer) = package.layer else {
            errors.push(format!(
                "{}: no [package.metadata.iw4l] layer",
                package.name
            ));
            continue;
        };
        if matches!(layer, Layer::Format | Layer::Game)
            && !package.game.as_deref().is_some_and(|g| GAMES.contains(&g))
        {
            errors.push(format!(
                "{}: layer {} needs game = one of {GAMES:?}",
                package.name,
                layer.name()
            ));
        }
        if layer == Layer::Mixed {
            findings.insert(format!("mixed {}", package.name), 1);
        }
        for dep in &package.deps {
            let Some(target) = by_name.get(dep.as_str()) else {
                continue;
            };
            if target
                .layer
                .is_some_and(|to| !edge_allowed(layer, package, to, target))
            {
                findings.insert(format!("edge {} {}", package.name, dep), 1);
            }
        }
    }

    let game_idents: BTreeSet<&str> = packages
        .iter()
        .filter(|p| match p.layer {
            Some(Layer::Game | Layer::Format) => true,
            Some(Layer::Neutral) => false,
            _ => {
                p.ident.starts_with("game_")
                    || GAMES.iter().any(|g| p.ident.ends_with(&format!("_{g}")))
            }
        })
        .map(|p| p.ident.as_str())
        .collect();
    let mut unknown_ids = 0;
    for package in &packages {
        for file in rust_files(&package.dir) {
            let rel = file
                .strip_prefix(root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            match package.layer {
                Some(Layer::Game) => {
                    let game = package.game.as_deref().unwrap_or("?");
                    for id in unknown_ids_in(&text) {
                        unknown_ids += 1;
                        if !ledger_lists(root, game, &id) {
                            errors.push(format!(
                                "{rel}: unknown!(\"{id}\") is not in docs/fidelity/{game}.md"
                            ));
                        }
                    }
                }
                Some(Layer::Format) | None => {}
                Some(_) if [REGISTRATION, IDENTITY, SELF].contains(&rel.as_str()) => {}
                Some(_) => {
                    for (shape, count) in game_shapes(&text, &game_idents) {
                        findings.insert(format!("pattern {rel} {shape}"), count);
                    }
                }
            }
        }
    }

    let allow_path = root.join(ALLOW);
    let allowed = match std::fs::read_to_string(&allow_path) {
        Ok(text) => parse_allow(&text)?,
        Err(_) if update => {
            write_allow(&allow_path, &findings)?;
            println!("boundary: wrote {ALLOW} with {} entries", findings.len());
            return Ok(());
        }
        Err(e) => return Err(format!("{ALLOW}: {e}")),
    };
    let mut new = Vec::new();
    let mut stale = Vec::new();
    for (key, &count) in &findings {
        let allow = allowed.get(key).copied().unwrap_or(0);
        if count > allow {
            new.push(format!("{key} {count} (allowed {allow})"));
        }
    }
    for (key, &allow) in &allowed {
        let count = findings.get(key).copied().unwrap_or(0);
        if count < allow {
            stale.push(format!("{key} {count} (allowed {allow})"));
        }
    }

    summary(&packages, &findings, unknown_ids);
    for error in &errors {
        println!("error: {error}");
    }
    for line in &new {
        println!("new:   {line}");
    }
    for line in &stale {
        println!("stale: {line}");
    }
    if update {
        let lowered: Findings = allowed
            .iter()
            .filter_map(|(key, &allow)| {
                let count = findings.get(key).copied().unwrap_or(0).min(allow);
                (count > 0).then(|| (key.clone(), count))
            })
            .collect();
        write_allow(&allow_path, &lowered)?;
        println!("boundary: {ALLOW} lowered to {} entries", lowered.len());
    }
    let failed = !errors.is_empty() || !new.is_empty() || (!stale.is_empty() && !update);
    if !failed {
        println!("boundary: nothing beyond {ALLOW}");
        return Ok(());
    }
    if enforce {
        return Err(format!(
            "{} errors, {} new, {} stale (stale entries: `cargo xtask boundary --update`)",
            errors.len(),
            new.len(),
            stale.len()
        ));
    }
    println!("boundary: report only; `--enforce` fails on the lines above");
    Ok(())
}

fn edge_allowed(from: Layer, package: &Package, to: Layer, target: &Package) -> bool {
    let same_game = package.game.is_some() && package.game == target.game;
    match from {
        Layer::Session | Layer::Mixed => true,
        Layer::Neutral => matches!(to, Layer::Neutral | Layer::Format),
        Layer::Format => to == Layer::Neutral || (to == Layer::Format && same_game),
        Layer::Game => {
            to == Layer::Neutral || (matches!(to, Layer::Format | Layer::Game) && same_game)
        }
    }
}

fn summary(packages: &[Package], findings: &Findings, unknown_ids: usize) {
    let mut layers: BTreeMap<&str, usize> = BTreeMap::new();
    for package in packages {
        *layers
            .entry(package.layer.map_or("none", Layer::name))
            .or_default() += 1;
    }
    let layers: Vec<String> = layers.iter().map(|(l, n)| format!("{l} {n}")).collect();
    println!(
        "boundary: {} crates — {}",
        packages.len(),
        layers.join(", ")
    );
    let edges = findings.keys().filter(|k| k.starts_with("edge ")).count();
    println!("  (a) graph: {edges} dependencies outside the layer rules");
    let mut per_crate: BTreeMap<String, usize> = BTreeMap::new();
    for (key, count) in findings.iter().filter(|(k, _)| k.starts_with("pattern ")) {
        let file = key.split(' ').nth(1).unwrap_or("");
        let krate = file
            .strip_prefix("crates/")
            .unwrap_or(file)
            .split('/')
            .next()
            .unwrap_or("");
        *per_crate.entry(krate.to_owned()).or_default() += count;
    }
    let total: usize = per_crate.values().sum();
    let mut ranked: Vec<_> = per_crate.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let ranked: Vec<String> = ranked.iter().map(|(c, n)| format!("{c} {n}")).collect();
    println!(
        "  (b) patterns: {total} game checks outside game crates: {}",
        ranked.join(", ")
    );
    println!("  (c) ledger: {unknown_ids} unknown!() ids in game crates");
}

fn workspace(root: &Path) -> Res<Vec<Package>> {
    let output = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let packages = meta["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    packages
        .iter()
        .map(|p| {
            let name = p["name"]
                .as_str()
                .ok_or("cargo metadata: a package without a name")?;
            let manifest = p["manifest_path"]
                .as_str()
                .ok_or_else(|| format!("cargo metadata: {name} has no manifest_path"))?;
            let iw4l = &p["metadata"]["iw4l"];
            Ok(Package {
                ident: name.replace('-', "_"),
                layer: iw4l["layer"].as_str().and_then(Layer::parse),
                game: iw4l["game"].as_str().map(str::to_owned),
                dir: Path::new(manifest)
                    .parent()
                    .ok_or_else(|| format!("{manifest}: no parent directory"))?
                    .to_path_buf(),
                deps: p["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|d| d["name"].as_str().map(str::to_owned))
                    .collect(),
                name: name.to_owned(),
            })
        })
        .collect()
}

fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let file_path = entry.path();
            if file_path.is_dir() {
                if file_path.file_name().is_some_and(|n| n != "target") {
                    stack.push(file_path);
                }
            } else if file_path.extension().is_some_and(|e| e == "rs") {
                files.push(file_path);
            }
        }
    }
    files.sort();
    files
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Count of `needle` in `line` as a whole token: no identifier character right
/// before it, and none right after unless `open_end`.
fn token_hits(line: &str, needle: &str, open_end: bool) -> usize {
    line.match_indices(needle)
        .filter(|(at, _)| {
            let before = line[..*at].chars().next_back();
            let after = line[at + needle.len()..].chars().next();
            !before.is_some_and(is_ident) && (open_end || !after.is_some_and(is_ident))
        })
        .count()
}

/// The code part of a line: whole-line comments are dropped, a trailing `//`
/// comment is cut when it is not inside a string.
fn code_of(line: &str) -> &str {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return "";
    }
    let mut quotes = 0;
    let bytes = line.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'"' if i == 0 || bytes[i - 1] != b'\\' => quotes += 1,
            b'/' if quotes % 2 == 0 && bytes.get(i + 1) == Some(&b'/') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn game_shapes(text: &str, game_idents: &BTreeSet<&str>) -> BTreeMap<String, usize> {
    let mut shapes: BTreeMap<String, usize> = BTreeMap::new();
    let mut add = |shape: String, n: usize| {
        if n > 0 {
            *shapes.entry(shape).or_default() += n;
        }
    };
    for line in text.lines().map(code_of) {
        add(
            "GameModeKind::Zombies".into(),
            token_hits(line, "GameModeKind::Zombies", false),
        );
        for game_enum in GAME_ENUMS {
            let n = GAME_VARIANTS
                .iter()
                .map(|v| token_hits(line, &format!("{game_enum}::{v}"), false))
                .sum();
            add(format!("{game_enum}::<game>"), n);
        }
        for ident in game_idents {
            add(
                format!("{ident}::"),
                token_hits(line, &format!("{ident}::"), true),
            );
        }
        let literals = GAMES
            .iter()
            .map(|g| line.matches(&format!("\"{g}\"")).count())
            .sum();
        add("\"<game>\"".into(), literals);
        let helpers = GAMES
            .iter()
            .map(|g| g.to_string())
            .chain(["zombies".to_owned()])
            .map(|g| token_hits(line, &format!("is_{g}"), false))
            .sum();
        add("is_<game>".into(), helpers);
    }
    shapes
}

fn unknown_ids_in(text: &str) -> Vec<String> {
    let code: Vec<&str> = text.lines().map(code_of).collect();
    let code = code.join("\n");
    code.match_indices("unknown!(")
        .filter_map(|(at, m)| {
            let rest = code[at + m.len()..].trim_start().strip_prefix('"')?;
            Some(rest[..rest.find('"')?].to_owned())
        })
        .collect()
}

fn ledger_lists(root: &Path, game: &str, id: &str) -> bool {
    std::fs::read_to_string(root.join("docs/fidelity").join(format!("{game}.md")))
        .is_ok_and(|ledger| ledger.contains(&format!("`{id}`")))
}

fn parse_allow(text: &str) -> Res<Findings> {
    let mut allowed = Findings::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, count) = match line.rsplit_once(' ') {
            Some((key, count)) if line.starts_with("pattern ") => (key, count.parse().ok()),
            _ => (line, Some(1)),
        };
        let count = count.ok_or_else(|| format!("{ALLOW}:{}: bad count", n + 1))?;
        allowed.insert(key.to_owned(), count);
    }
    Ok(allowed)
}

fn write_allow(path: &Path, findings: &Findings) -> Res<()> {
    let mut text = String::from(
        "# Game-boundary debt that exists today (`cargo xtask boundary`).\n\
         # It may only shrink: lower it with `cargo xtask boundary --update`.\n",
    );
    for (key, count) in findings {
        if key.starts_with("pattern ") {
            text.push_str(&format!("{key} {count}\n"));
        } else {
            text.push_str(&format!("{key}\n"));
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
