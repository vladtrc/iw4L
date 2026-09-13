//! `cargo xtask mr new|ship|ls|fmt` — the lifecycle of an agent clone under
//! `context/mrs/`.
//!
//! One agent on the root needs no clone; a clone exists only when someone else
//! is already working on the root. The agent's territory is code and `git
//! commit` *inside the clone*. Landing is this one command — rebase onto master
//! → rustfmt the touched `.rs` → fast-forward → delete the clone — never
//! reproduced by hand, and never `git push origin HEAD:master`.
//!
//! Everything here is fail-closed. Uncommitted tracked WIP, an untracked `.rs`,
//! a dirty root, a rebase conflict and a shared path that is not the symlink we
//! put there all refuse and leave the clone on disk. The expensive mistake this
//! guards against is `rm -rf` walking through a symlink into the shared trees,
//! so the removal step verifies every link before it unlinks it.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fmt;
use crate::shell::{Res, capture, run};

/// Clones live under `context/`, next to the artifacts and the RE bases they
/// borrow — working memory, all of it outside git.
const MRS_DIR: &str = "context/mrs";

/// Shared trees a clone gets as a symlink instead of a copy: the journal and
/// the external checkouts are one tree for every agent, and they are large.
/// `context/mrs` itself is not here — a clone does not contain the clones.
const SHARED: [(&str, Required); 2] = [
    ("context/artifacts", Required::Yes),
    ("context/externals", Required::No),
];

/// The files under `context/re/` that are code and therefore already in the
/// clone's checkout. Everything else in that folder is a base or its scratch,
/// and gets symlinked to the root's copy — the same allowlist as
/// `context/re/.gitignore` and the pre-commit hook.
const RE_TRACKED: [&str; 5] = [".gitignore", "README.md", "re", "schema.sql", "ghidra"];

const HOOK: &str = include_str!("../githooks/pre-commit");

#[derive(Clone, Copy, PartialEq, Eq)]
enum Required {
    Yes,
    No,
}

const USAGE: &str = "usage: cargo xtask mr <new|ship|ls|fmt> …";

/// One verb for the whole clone lifecycle: `mr new`, `mr ship`, `mr ls`, and
/// `mr fmt` — the formatter belongs here because the only reason it exists is
/// the same one the clones do: nobody rewrites files they do not own.
pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    let Some(sub) = args.first() else {
        return Err(USAGE.into());
    };
    let rest = args.get(1..).unwrap_or(&[]);
    match sub.as_str() {
        "new" => new_mr(root, &parse_name(rest, "new")?),
        "ship" => ship(root, &parse_name(rest, "ship")?),
        "ls" => list(root),
        "fmt" => crate::fmt::run_cli(root, rest),
        other => Err(format!("unknown mr command: {other}\n{USAGE}")),
    }
}

fn parse_name(args: &[String], sub: &str) -> Res<String> {
    let Some(name) = args.first() else {
        return Err(format!("usage: cargo xtask mr {sub} <name>"));
    };
    let simple = !name.is_empty()
        && name.starts_with(|c: char| c.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !name.contains("..");
    if !simple {
        return Err(format!("name must be a simple slug: {name}"));
    }
    Ok(name.clone())
}

/// What is on disk right now, and whose move it is: a clone with commits of its
/// own is waiting to be shipped, a dirty one is still being written in.
fn list(root: &Path) -> Res<()> {
    let dir = root.join(MRS_DIR);
    let mut names: Vec<String> = match std::fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(format!("reading {}: {error}", dir.display())),
    };
    names.sort();
    if names.is_empty() {
        println!("no clones under {MRS_DIR}/ — a single agent on the root needs none");
        return Ok(());
    }
    for name in names {
        let clone = dir.join(&name);
        if !clone.join(".git").is_dir() {
            println!("{name:<24} not a git clone (leftover; inspect by hand)");
            continue;
        }
        let branch = git_out(&clone, &["rev-parse", "--abbrev-ref", "HEAD"])?;
        let ahead = git_out(&clone, &["rev-list", "--count", "origin/master..HEAD"])?;
        let mut state = Vec::new();
        if ahead != "0" {
            state.push(format!("{ahead} commit(s) to ship"));
        }
        if tracked_dirty(&clone)? {
            state.push("tracked WIP".to_string());
        }
        if untracked_rs(&clone)? {
            state.push("untracked .rs".to_string());
        }
        if state.is_empty() {
            state.push("clean, nothing to ship".to_string());
        }
        println!("{name:<24} {branch:<24} {}", state.join(", "));
    }
    Ok(())
}

// --- the clone ---------------------------------------------------------------

/// Path-clone the root into `context/mrs/<name>` on branch `agent/<name>`, with
/// the local-only trees already in place so the agent never recreates them.
fn new_mr(root: &Path, name: &str) -> Res<()> {
    let clone = root.join(MRS_DIR).join(name);
    let branch = format!("agent/{name}");

    require_repo(root)?;
    if clone.symlink_metadata().is_ok() {
        return Err(format!("already exists: {}", clone.display()));
    }
    let env_file = root.join(".env");
    if !env_file.is_file() {
        return Err("root has no .env — copy .env.example to .env and set IW4L_GAMES first".into());
    }
    for (rel, required) in SHARED {
        if required == Required::Yes && !root.join(rel).is_dir() {
            return Err(format!(
                "root has no {rel}/ — the shared local tree must exist"
            ));
        }
    }

    std::fs::create_dir_all(root.join(MRS_DIR))
        .map_err(|error| format!("creating {MRS_DIR}: {error}"))?;

    println!("==> clone {} (branch {branch})", clone.display());
    git(
        root,
        &["clone", "--local", &path_arg(root)?, &path_arg(&clone)?],
    )?;
    git(&clone, &["checkout", "-b", &branch])?;

    let mut linked = Vec::new();
    for (rel, required) in SHARED {
        if link_shared(root, &clone, rel, required)? {
            linked.push(rel.to_string());
        }
    }
    for rel in re_data(root)? {
        link_shared(root, &clone, &rel, Required::Yes)?;
    }
    if root.join("iw4l-artifacts").is_dir() {
        link_shared(root, &clone, "iw4l-artifacts", Required::No)?;
        linked.push("iw4l-artifacts".to_string());
    }

    // A regular file, not a symlink: agents read "already set up" off a real
    // file and stop trying to recreate it. It is machine-local and tiny.
    let clone_env = clone.join(".env");
    let _ = std::fs::remove_file(&clone_env);
    std::fs::copy(&env_file, &clone_env).map_err(|error| format!("copying .env: {error}"))?;
    let env_text =
        std::fs::read_to_string(&clone_env).map_err(|error| format!("reading .env: {error}"))?;
    if !env_text.lines().any(|line| line.starts_with("IW4L_GAMES=")) {
        return Err("clone .env has no IW4L_GAMES=".into());
    }

    install_hook(&clone)?;
    install_hook(root)?;

    println!("==> done");
    println!("    cd {}", clone.display());
    println!("    branch: {branch}");
    println!("    target/ is cargo's default in this clone — do not export CARGO_TARGET_DIR");
    for rel in &linked {
        println!("    {rel} -> the shared tree (symlink; not in git)");
    }
    println!("    context/re/ tools are checked out; the bases are symlinks to the root's");
    println!("    .env copied from root (IW4L_GAMES; gitignored)");
    println!("    do not recreate .env or the shared trees — they are already here");
    println!("    ship:  cargo xtask mr ship {name}     # from the repo root, after git commit:");
    println!("           rebase → rustfmt touched .rs + commit → FF → rm clone");
    println!("           do none of those by hand; fix only what ship refuses on, then rerun");
    println!("    before ship: read git diff origin/master...HEAD — probes, throwaway tests and");
    println!(
        "           debug prints come out of the tree first (CONTEXT.md, \"Before shipping\")"
    );
    Ok(())
}

/// The entries of `context/re/` that are data rather than code, as repo-relative
/// paths. New bases appear here on their own — the allowlist names the code.
fn re_data(root: &Path) -> Res<Vec<String>> {
    let dir = root.join("context/re");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let entries =
        std::fs::read_dir(&dir).map_err(|error| format!("reading {}: {error}", dir.display()))?;
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("reading {}: {error}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if RE_TRACKED.contains(&name.as_str()) || name.ends_with(".py") {
            continue;
        }
        out.push(format!("context/re/{name}"));
    }
    out.sort();
    Ok(out)
}

/// Replace `clone/<rel>` with a relative symlink to the root's tree. Returns
/// false when the root has nothing at `rel` and it was optional. Never deletes
/// the shared tree: a clone path that already resolves there is a refusal.
fn link_shared(root: &Path, clone: &Path, rel: &str, required: Required) -> Res<bool> {
    let src = root.join(rel);
    if !src.exists() {
        if required == Required::Yes {
            return Err(format!("root has no {rel}"));
        }
        return Ok(false);
    }
    let dst = clone.join(rel);
    if dst.symlink_metadata().is_ok() {
        if dst.is_symlink() {
            std::fs::remove_file(&dst).map_err(|error| format!("unlinking {rel}: {error}"))?;
        } else {
            if same_path(&dst, &src) {
                return Err(format!(
                    "clone {rel} resolved to the shared tree — refusing to delete it"
                ));
            }
            println!("    dropping cloned {rel} (will symlink the shared tree)");
            if dst.is_dir() {
                std::fs::remove_dir_all(&dst)
                    .map_err(|error| format!("removing {rel}: {error}"))?;
            } else {
                std::fs::remove_file(&dst).map_err(|error| format!("removing {rel}: {error}"))?;
            }
        }
    }
    let parent = dst
        .parent()
        .ok_or_else(|| format!("{rel} has no parent directory"))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("creating {}: {error}", parent.display()))?;
    let target = relative_target(root, parent, rel)?;
    symlink(&target, &dst).map_err(|error| format!("linking {rel}: {error}"))?;
    if !same_path(&dst, &src) {
        return Err(format!("{rel} symlink does not resolve to the shared tree"));
    }
    Ok(true)
}

/// `../` per directory between the link and the repo root, then the target's
/// repo-relative path. Relative so the whole `context/mrs` tree can be moved.
fn relative_target(root: &Path, link_dir: &Path, rel: &str) -> Res<String> {
    let depth = link_dir
        .strip_prefix(root)
        .map_err(|_| format!("{} is outside {}", link_dir.display(), root.display()))?
        .components()
        .count();
    Ok(format!("{}{rel}", "../".repeat(depth)))
}

fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(unix)]
fn symlink(target: &str, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(not(unix))]
fn symlink(_target: &str, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other(
        "agent clones need symlinks; run this on the Linux side",
    ))
}

/// Copy the hook into `<repo>/.git/hooks/`. Every command that touches a repo
/// installs it, because the wall is worth nothing in the one clone that skipped
/// it. A repo without `.git` (a fixture) is not an error.
pub fn install_hook(repo: &Path) -> Res<()> {
    let hooks = repo.join(".git/hooks");
    if !repo.join(".git").is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(&hooks)
        .map_err(|error| format!("creating {}: {error}", hooks.display()))?;
    let dest = hooks.join("pre-commit");
    std::fs::write(&dest, HOOK).map_err(|error| format!("writing {}: {error}", dest.display()))?;
    make_executable(&dest)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Res<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .map_err(|error| format!("chmod {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Res<()> {
    Ok(())
}

// --- ship --------------------------------------------------------------------

/// Rebase → rustfmt → fast-forward → delete, in that order, refusing rather
/// than repairing. A refusal leaves the clone exactly where it was.
///
/// What it does **not** check is whether the branch still carries its
/// scaffolding — a probe, a test that asserts only that the code ran, a
/// leftover `dbg!`. No grep tells those apart from the real thing (`probe` is a
/// lighting term in half this tree), so the rule lives in `CONTEXT.md`
/// ("Before shipping") and is the agent's own last step: read the branch diff
/// and take the disposable half back out before running this.
fn ship(root: &Path, name: &str) -> Res<()> {
    require_repo(root)?;
    let clone = root.join(MRS_DIR).join(name);
    if clone.symlink_metadata().is_err() {
        return Err(format!(
            "no clone at {} — run from the repo root (not from {MRS_DIR}/<name>)",
            clone.display()
        ));
    }
    if !clone.join(".git").is_dir() {
        return Err(format!(
            "not a git clone: {} (leftover; inspect and remove by hand)",
            clone.display()
        ));
    }
    refuse_target_dir(
        &clone,
        std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
    )?;

    install_hook(root)?;
    install_hook(&clone)?;
    refuse_wip(&clone, name)?;
    refuse_root_wip(root)?;

    println!("==> fetch origin in clone");
    git(&clone, &["fetch", "origin"])?;

    if is_ancestor(&clone, "HEAD", "origin/master")? {
        println!("    clone HEAD already on root master — nothing to rebase/fmt");
    } else {
        if !is_ancestor(&clone, "origin/master", "HEAD")? {
            println!("==> rebase clone onto origin/master");
            if git(&clone, &["rebase", "origin/master"]).is_err() {
                let _ = git(&clone, &["rebase", "--abort"]);
                return Err(format!(
                    "rebase conflict. resolve by hand in {} (git rebase origin/master), then cargo xtask mr ship {name}",
                    clone.display()
                ));
            }
        }
        fmt_touched(&clone, name)?;
    }

    land(root, &clone, name)?;
    remove(root, &clone, name)?;
    let head = git_out(root, &["rev-parse", "--short", "HEAD"])?;
    println!("    shipped {name} → master {head}");
    Ok(())
}

/// rustfmt only what the branch touched, and commit that diff as its own
/// commit. Formatting the whole tree here would put files the branch never
/// opened into its history.
fn fmt_touched(clone: &Path, name: &str) -> Res<()> {
    println!("==> rustfmt only the .rs files this branch touched");
    let listing = git_out(
        clone,
        &[
            "diff",
            "--name-only",
            "--diff-filter=ACMR",
            "origin/master...HEAD",
            "--",
            "*.rs",
        ],
    )?;
    let files: Vec<String> = listing
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    if files.is_empty() {
        println!("    no .rs changes");
        return Ok(());
    }
    fmt::format_paths(clone, &files)?;
    if !tracked_dirty(clone)? {
        println!("    already formatted");
        return Ok(());
    }
    let mut args = vec!["add", "--"];
    args.extend(files.iter().map(String::as_str));
    git(clone, &args)?;
    git(
        clone,
        &[
            "commit",
            "--quiet",
            "-m",
            &format!("style: rustfmt (cargo xtask mr ship {name})"),
        ],
    )?;
    let head = git_out(clone, &["rev-parse", "--short", "HEAD"])?;
    println!("    committed fmt diff: {head}");
    Ok(())
}

/// Fast-forward the root onto the clone's HEAD. No stash, no rebase here, and
/// no push into the checked-out root: the objects are fetched *from* the clone.
fn land(root: &Path, clone: &Path, name: &str) -> Res<()> {
    refuse_wip(clone, name)?;
    refuse_root_wip(root)?;

    let origin = git_out(clone, &["remote", "get-url", "origin"])?;
    if !same_path(Path::new(&origin), root) {
        return Err(format!("clone origin is {origin}, not {}", root.display()));
    }

    println!("==> fetch origin in clone (refresh view of root HEAD)");
    git(clone, &["fetch", "origin"])?;

    let clone_head = git_out(clone, &["rev-parse", "HEAD"])?;
    let root_head = git_out(root, &["rev-parse", "HEAD"])?;
    let origin_master = git_out(clone, &["rev-parse", "origin/master"])?;
    if origin_master != root_head {
        return Err(format!(
            "clone origin/master is {origin_master}, root HEAD is {root_head}"
        ));
    }

    // Compared inside the clone: it has origin/master plus its own objects,
    // while the root does not have the clone's commits until the fetch below.
    if is_ancestor(clone, &clone_head, "origin/master")? {
        println!("ship: {clone_head} already an ancestor of root HEAD — nothing to merge");
        return Ok(());
    }
    if !is_ancestor(clone, "origin/master", &clone_head)? {
        return Err(format!(
            "not fast-forward: clone and root have diverged (master moved during ship?). run cargo xtask mr ship {name} again — it rebases. do not git push origin HEAD:master"
        ));
    }

    println!("==> fetch clone HEAD into root (objects live in the clone until this)");
    git(root, &["fetch", &path_arg(clone)?, "HEAD"])?;
    println!("==> merge --ff-only {clone_head} onto master");
    git(root, &["merge", "--ff-only", "--no-edit", "FETCH_HEAD"])?;
    if !is_ancestor(root, &clone_head, "HEAD")? {
        return Err("post-land ancestor check failed".into());
    }
    println!("    ancestor=true");
    Ok(())
}

/// Delete the clone once the root contains it. The shared names are unlinked
/// first, one by one, after each has been proved to be the symlink we made —
/// a directory walk through one of them would eat the shared tree.
fn remove(root: &Path, clone: &Path, name: &str) -> Res<()> {
    refuse_wip(clone, name)?;

    git(clone, &["fetch", "origin"])?;
    if !is_ancestor(clone, "HEAD", "origin/master")? {
        let _ = git(clone, &["log", "--oneline", "origin/master..HEAD"]);
        return Err(format!(
            "clone has commits not in root HEAD after land — should not happen; run cargo xtask mr ship {name} again"
        ));
    }

    let mut shared: Vec<(String, Required)> = SHARED
        .iter()
        .map(|(rel, required)| ((*rel).to_string(), *required))
        .collect();
    shared.push(("iw4l-artifacts".to_string(), Required::No));
    for rel in re_data(root)? {
        shared.push((rel, Required::No));
    }

    println!("==> unlink shared names, then remove {}", clone.display());
    for (rel, required) in &shared {
        let dst = clone.join(rel);
        if dst.symlink_metadata().is_err() {
            if *required == Required::Yes {
                return Err(format!("missing {rel} at {}", dst.display()));
            }
            continue;
        }
        if !dst.is_symlink() {
            return Err(format!(
                "{rel} is not a symlink — refusing to delete (would hit the shared tree?)"
            ));
        }
        if !same_path(&dst, &root.join(rel)) {
            return Err(format!(
                "{rel} does not resolve to the root's tree — refusing to delete"
            ));
        }
        std::fs::remove_file(&dst).map_err(|error| format!("unlinking {rel}: {error}"))?;
    }

    // `remove_dir_all` never follows a symlink, but the loop above has already
    // taken every link out of the tree; what is left is the clone's own files.
    std::fs::remove_dir_all(clone)
        .map_err(|error| format!("removing {}: {error}", clone.display()))?;
    if clone.symlink_metadata().is_ok() {
        return Err(format!("still exists: {}", clone.display()));
    }
    println!("    removed {}", clone.display());
    Ok(())
}

// --- refusals ----------------------------------------------------------------

/// cargo with `CARGO_TARGET_DIR` inside the clone recreates the tree right
/// after we remove it, leaving a directory that is no longer a clone. The value
/// is a parameter so the refusal can be gated without touching the process
/// environment.
pub fn refuse_target_dir(clone: &Path, target_dir: Option<PathBuf>) -> Res<()> {
    let Some(target_dir) = target_dir else {
        return Ok(());
    };
    if same_path(&target_dir, clone) || target_dir.starts_with(clone) {
        return Err(format!(
            "CARGO_TARGET_DIR={} points at this clone. unset it first so cargo does not recreate the tree",
            target_dir.display()
        ));
    }
    Ok(())
}

fn require_repo(root: &Path) -> Res<()> {
    if root.join(".git").exists() {
        Ok(())
    } else {
        Err(format!("not a git repo: {}", root.display()))
    }
}

fn refuse_wip(clone: &Path, name: &str) -> Res<()> {
    if tracked_dirty(clone)? {
        print_status(clone)?;
        return Err(format!(
            "clone has uncommitted tracked WIP. git commit it (or restore), then cargo xtask mr ship {name}"
        ));
    }
    if untracked_rs(clone)? {
        print_status(clone)?;
        return Err(format!(
            "clone has untracked .rs files. git add + commit them (or remove), then cargo xtask mr ship {name}"
        ));
    }
    Ok(())
}

fn refuse_root_wip(root: &Path) -> Res<()> {
    if tracked_dirty(root)? {
        print_status(root)?;
        return Err("root has tracked WIP. ship only FF-merges onto a clean master".into());
    }
    let branch = git_out(root, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch != "master" {
        return Err(format!("root is on '{branch}', not master"));
    }
    Ok(())
}

fn tracked_dirty(dir: &Path) -> Res<bool> {
    Ok(status(dir)?
        .lines()
        .any(|line| !line.is_empty() && !line.starts_with("??")))
}

fn untracked_rs(dir: &Path) -> Res<bool> {
    Ok(status(dir)?
        .lines()
        .any(|line| line.starts_with("??") && line.ends_with(".rs")))
}

fn status(dir: &Path) -> Res<String> {
    git_out(dir, &["status", "--porcelain=v1"])
}

fn print_status(dir: &Path) -> Res<()> {
    eprint!("{}", status(dir)?);
    Ok(())
}

// --- git ---------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> Res<()> {
    run(Command::new("git").arg("-C").arg(dir).args(args))
}

fn git_out(dir: &Path, args: &[&str]) -> Res<String> {
    Ok(capture(Command::new("git").arg("-C").arg(dir).args(args))?
        .trim_end()
        .to_string())
}

/// The exit status is the answer, not a failure.
fn is_ancestor(dir: &Path, ancestor: &str, descendant: &str) -> Res<bool> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .status()
        .map_err(|error| format!("cannot run git merge-base: {error}"))?;
    Ok(status.success())
}

/// git takes paths as arguments; a path that is not UTF-8 would be silently
/// mangled by `to_string_lossy`, so it is an error instead.
fn path_arg(path: &Path) -> Res<String> {
    path.to_str()
        .map(str::to_string)
        .ok_or_else(|| format!("path is not UTF-8: {}", path.display()))
}
