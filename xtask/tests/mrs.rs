//! Fixture gates for `cargo xtask mr`: explicit-path fmt, fail-closed ship,
//! and the pre-commit hook. Every test builds a throwaway git repo in the
//! system temp dir — none of them touch this repo's `context/mrs/` or the
//! engine.
//!
//! What is gated here is the refusals. A ship that lands the work is easy to
//! write; a ship that leaves the clone alone when the root is dirty, aborts a
//! conflicted rebase, and never follows a symlink out into the shared tree is
//! the reason this code exists in the first place.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use xtask::mrs;

/// A git repo shaped like the real root — `.env`, `context/artifacts`, two
/// committed `.rs` files — removed when the test ends.
struct Fixture {
    root: PathBuf,
}

static SEQ: AtomicUsize = AtomicUsize::new(0);

impl Fixture {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("iw4l-mrs-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("fixture root");
        let fixture = Self { root };

        run(&fixture.root, "git", &["init", "-b", "master", "."]);
        identify(&fixture.root);
        std::fs::create_dir_all(fixture.root.join("context/artifacts")).expect("context");
        std::fs::create_dir_all(fixture.root.join("crates/foo/src")).expect("crates");
        write(&fixture.root.join("context/artifacts/keep-me"), "keep\n");
        write(&fixture.root.join(".env"), "IW4L_GAMES=/nowhere\n");
        write(
            &fixture.root.join("crates/foo/src/a.rs"),
            "fn a(){ let x=1; x }\n",
        );
        write(
            &fixture.root.join("crates/foo/src/b.rs"),
            "fn b(){ let y=2; y }\n",
        );
        run(
            &fixture.root,
            "git",
            &["add", "crates/foo/src/a.rs", "crates/foo/src/b.rs"],
        );
        run(&fixture.root, "git", &["commit", "-qm", "init"]);
        fixture
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn clone_path(&self, name: &str) -> PathBuf {
        self.path("context/mrs").join(name)
    }

    /// `cargo xtask mr <args>` against this fixture.
    fn mr(&self, args: &[&str]) -> Result<(), String> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        mrs::run_cli(&self.root, &owned)
    }

    /// A clone with a git identity, ready to commit in.
    fn new_clone(&self, name: &str) -> PathBuf {
        self.mr(&["new", name]).expect("mr new");
        let clone = self.clone_path(name);
        identify(&clone);
        clone
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // `remove_dir_all` never follows a symlink, so the clones' links into
        // the fixture's own shared trees cannot widen this.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn identify(repo: &Path) {
    run(
        repo,
        "git",
        &["config", "user.email", "mrs-test@iw4l.local"],
    );
    run(repo, "git", &["config", "user.name", "mrs-test"]);
    run(repo, "git", &["config", "commit.gpgsign", "false"]);
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("parent");
    }
    std::fs::write(path, text).expect("write");
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn run(dir: &Path, program: &str, args: &[&str]) {
    let status = Command::new(program)
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap_or_else(|error| panic!("{program}: {error}"));
    assert!(
        status.success(),
        "{program} {args:?} failed in {}",
        dir.display()
    );
}

/// Exit status as the answer — the caller expects the command to fail.
fn try_run(dir: &Path, program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap_or_else(|error| panic!("{program}: {error}"))
        .success()
}

fn commit(repo: &Path, rel: &str, body: &str, message: &str) {
    write(&repo.join(rel), body);
    run(repo, "git", &["add", rel]);
    run(repo, "git", &["commit", "-qm", message]);
}

fn subject(repo: &Path) -> String {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["log", "-1", "--format=%s"])
        .output()
        .expect("git log");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

// --- fmt ---------------------------------------------------------------------

#[test]
fn fmt_refuses_everything_it_would_have_to_guess() {
    let fixture = Fixture::new();
    std::fs::create_dir_all(fixture.path("dir.rs")).expect("dir.rs");
    write(&fixture.path("foo.toml"), "x = 1\n");
    let outside = std::env::temp_dir().join(format!("iw4l-outside-{}.rs", std::process::id()));
    write(&outside, "fn z(){}\n");

    assert!(fixture.mr(&["fmt"]).is_err(), "empty list");
    assert!(fixture.mr(&["fmt", "dir.rs"]).is_err(), "directory");
    assert!(fixture.mr(&["fmt", "foo.toml"]).is_err(), "non-rs");
    assert!(fixture.mr(&["fmt", "no-such.rs"]).is_err(), "missing");
    assert!(fixture.mr(&["fmt", "--all"]).is_err(), "flag");
    assert!(
        fixture
            .mr(&["fmt", outside.to_str().expect("utf-8")])
            .is_err(),
        "outside the repo"
    );

    let _ = std::fs::remove_file(&outside);
}

#[test]
fn fmt_writes_only_the_files_it_was_given() {
    let fixture = Fixture::new();
    write(
        &fixture.path("crates/foo/src/a.rs"),
        "fn a( ) { let    x = 1 ; x }\n",
    );
    write(
        &fixture.path("crates/foo/src/b.rs"),
        "fn b( ) { let    y = 2 ; y }\n",
    );

    fixture.mr(&["fmt", "crates/foo/src/a.rs"]).expect("fmt a");

    assert!(
        !read(&fixture.path("crates/foo/src/a.rs")).contains("let    x"),
        "the listed file was not formatted"
    );
    assert!(
        read(&fixture.path("crates/foo/src/b.rs")).contains("let    y"),
        "an unlisted file was rewritten"
    );
}

// --- the pre-commit hook -----------------------------------------------------

#[test]
fn hook_refuses_unstaged_format_only_leftover() {
    let fixture = Fixture::new();
    // HEAD holds the noisy form, so rustfmt(index) differs from it.
    write(
        &fixture.path("crates/foo/src/a.rs"),
        "fn a( ) { let    x = 1 ; x }\n",
    );
    write(
        &fixture.path("crates/foo/src/b.rs"),
        "fn b( ) { let    y = 2 ; y }\n",
    );
    run(&fixture.root, "git", &["add", "crates/foo/src"]);
    run(&fixture.root, "git", &["commit", "-qm", "ugly"]);

    // What `cargo fmt --all` does: rewrite both, stage only the owned one.
    fixture
        .mr(&["fmt", "crates/foo/src/a.rs", "crates/foo/src/b.rs"])
        .expect("fmt both");
    run(&fixture.root, "git", &["add", "crates/foo/src/a.rs"]);

    assert!(
        !try_run(&fixture.root, "git", &["commit", "-qm", "only a"]),
        "hook allowed a commit with an unstaged format-only leftover"
    );

    run(
        &fixture.root,
        "git",
        &["restore", "--", "crates/foo/src/b.rs"],
    );
    assert!(
        try_run(&fixture.root, "git", &["commit", "-qm", "only a"]),
        "hook refused the owned staged file after the leftover was restored"
    );
}

#[test]
fn hook_refuses_a_forced_add_under_context() {
    let fixture = Fixture::new();
    fixture
        .mr(&["fmt", "crates/foo/src/a.rs"])
        .expect("installs the hook");
    write(&fixture.path("context/artifacts/note.md"), "journal\n");

    run(
        &fixture.root,
        "git",
        &["add", "-f", "context/artifacts/note.md"],
    );
    assert!(
        !try_run(
            &fixture.root,
            "git",
            &["commit", "-qm", "sneak the journal in"]
        ),
        "hook allowed a forced add under context/"
    );
}

// --- ship --------------------------------------------------------------------

#[test]
fn ship_refuses_before_it_touches_anything() {
    let fixture = Fixture::new();
    assert!(fixture.mr(&["ship"]).is_err(), "missing name");
    assert!(fixture.mr(&["ship", "no-such"]).is_err(), "unknown clone");

    std::fs::create_dir_all(fixture.clone_path("leftover")).expect("leftover");
    assert!(
        fixture.mr(&["ship", "leftover"]).is_err(),
        "a directory without .git is a leftover, not a clone"
    );
    assert!(
        fixture.clone_path("leftover").is_dir(),
        "the leftover was deleted instead of refused"
    );

    let clone = fixture.new_clone("demo");
    write(&clone.join("crates/foo/src/c.rs"), "fn c(){}\n");
    run(&clone, "git", &["add", "crates/foo/src/c.rs"]);
    assert!(
        fixture.mr(&["ship", "demo"]).is_err(),
        "uncommitted tracked WIP must refuse"
    );
    assert!(clone.is_dir(), "a refusal left the clone on disk");
}

#[test]
fn ship_formats_lands_and_removes() {
    let fixture = Fixture::new();
    let clone = fixture.new_clone("demo");
    commit(&clone, "crates/foo/src/c.rs", "fn c(){}\n", "c");

    fixture.mr(&["ship", "demo"]).expect("ship");

    assert!(!clone.exists(), "the clone is still on disk");
    let landed = read(&fixture.path("crates/foo/src/c.rs"));
    assert_eq!(landed, "fn c() {}\n", "the shipped file was not formatted");
    assert!(
        subject(&fixture.root).contains("rustfmt (cargo xtask mr ship demo)"),
        "the fmt diff was not committed as its own commit: {}",
        subject(&fixture.root)
    );
    assert!(
        read(&fixture.path("crates/foo/src/a.rs")).contains("fn a(){ let x=1; x }"),
        "ship formatted a file the branch never touched"
    );
    assert_eq!(
        read(&fixture.path("context/artifacts/keep-me")),
        "keep\n",
        "the shared tree did not survive the clone's removal"
    );
}

#[test]
fn ship_rebases_a_clone_whose_master_moved() {
    let fixture = Fixture::new();
    let clone = fixture.new_clone("demo");
    commit(
        &fixture.root,
        "crates/foo/src/root.rs",
        "fn root_only() {}\n",
        "root-only",
    );
    commit(
        &clone,
        "crates/foo/src/clone.rs",
        "fn clone_only() {}\n",
        "clone-only",
    );

    fixture.mr(&["ship", "demo"]).expect("ship rebases");

    assert!(!clone.exists(), "the clone is still on disk");
    assert!(
        fixture.path("crates/foo/src/root.rs").is_file(),
        "root commit lost"
    );
    assert!(
        fixture.path("crates/foo/src/clone.rs").is_file(),
        "clone commit lost"
    );
}

#[test]
fn ship_aborts_a_conflicted_rebase_and_keeps_the_clone() {
    let fixture = Fixture::new();
    let clone = fixture.new_clone("demo");
    commit(
        &fixture.root,
        "crates/foo/src/a.rs",
        "fn a() {\n    1\n}\n",
        "root-a",
    );
    commit(
        &clone,
        "crates/foo/src/a.rs",
        "fn a() {\n    2\n}\n",
        "clone-a",
    );

    assert!(
        fixture.mr(&["ship", "demo"]).is_err(),
        "a conflict must refuse"
    );

    assert!(
        clone.join(".git").is_dir(),
        "the clone was removed on a conflict"
    );
    assert!(
        !clone.join(".git/rebase-merge").exists() && !clone.join(".git/rebase-apply").exists(),
        "a half-finished rebase was left behind"
    );
    assert!(
        read(&fixture.path("crates/foo/src/a.rs")).contains("    1\n"),
        "the root was changed by a failed ship"
    );
}

#[test]
fn ship_refuses_to_delete_a_real_directory_where_a_link_belongs() {
    let fixture = Fixture::new();
    let clone = fixture.new_clone("demo");
    commit(&clone, "crates/foo/src/c.rs", "fn c() {}\n", "c");

    // Someone replaced the symlink with a copy: removing the clone by path
    // would now be indistinguishable from removing the shared journal.
    let link = clone.join("context/artifacts");
    std::fs::remove_file(&link).expect("unlink");
    std::fs::create_dir_all(&link).expect("real dir");
    write(&link.join("keep-me"), "not the shared one\n");

    assert!(
        fixture.mr(&["ship", "demo"]).is_err(),
        "a real directory in a shared name must refuse"
    );
    assert_eq!(
        read(&fixture.path("context/artifacts/keep-me")),
        "keep\n",
        "the shared journal was damaged by the refusal path"
    );
}

#[test]
fn ship_refuses_a_cargo_target_dir_inside_the_clone() {
    let fixture = Fixture::new();
    let clone = fixture.clone_path("demo");
    assert!(
        mrs::refuse_target_dir(&clone, Some(clone.join("target"))).is_err(),
        "a target dir inside the clone must refuse"
    );
    assert!(
        mrs::refuse_target_dir(&clone, Some(clone.clone())).is_err(),
        "the clone itself as target dir must refuse"
    );
    assert!(
        mrs::refuse_target_dir(&clone, Some(fixture.path("target"))).is_ok(),
        "a target dir outside the clone is fine"
    );
    assert!(
        mrs::refuse_target_dir(&clone, None).is_ok(),
        "unset is fine"
    );
}

// --- new / ls ----------------------------------------------------------------

#[test]
fn new_refuses_a_name_that_is_not_a_slug() {
    let fixture = Fixture::new();
    assert!(fixture.mr(&["new"]).is_err(), "missing name");
    assert!(fixture.mr(&["new", "../evil"]).is_err(), "traversal");
    assert!(fixture.mr(&["new", "a/b"]).is_err(), "path separator");
    assert!(fixture.mr(&["new", "-flag"]).is_err(), "flag-shaped");
    assert!(fixture.mr(&["bogus"]).is_err(), "unknown subcommand");
}

#[test]
fn new_links_the_shared_trees_and_copies_env() {
    let fixture = Fixture::new();
    let clone = fixture.new_clone("demo");

    let artifacts = clone.join("context/artifacts");
    assert!(artifacts.is_symlink(), "the journal was copied, not linked");
    assert_eq!(
        artifacts.canonicalize().expect("link target"),
        fixture
            .path("context/artifacts")
            .canonicalize()
            .expect("shared"),
        "the link does not resolve to the shared tree"
    );

    let env = clone.join(".env");
    assert!(!env.is_symlink(), ".env must be a real file in the clone");
    assert!(
        read(&env).contains("IW4L_GAMES="),
        ".env did not carry over"
    );
    assert!(
        clone.join(".git/hooks/pre-commit").is_file(),
        "the clone has no pre-commit hook"
    );
    assert!(
        fixture.mr(&["new", "demo"]).is_err(),
        "a second clone of the same name must refuse"
    );
    // The clone is git-clean on arrival: the links are ignored, not WIP.
    let out = Command::new("git")
        .current_dir(&clone)
        .args(["status", "--porcelain=v1"])
        .output()
        .expect("git status");
    let dirty: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|line| !line.starts_with("??"))
        .map(str::to_string)
        .collect();
    assert!(
        dirty.is_empty(),
        "a fresh clone is already dirty: {dirty:?}"
    );
}
