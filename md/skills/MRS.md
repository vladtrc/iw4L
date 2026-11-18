# MRs: agent clones and landing

* **internal MR** — a clone of a repo for one agent. In this repository:
  `context/mrs/<name>/`. A single agent on a repo root needs **no** clone;
  a clone exists only when someone else is already working on that root.
* **external MR** — a GitHub PR, handled with `gh`.

**Other agents' clones are not touched without being asked.**

## Lifecycle

One verb, `cargo xtask mr` underneath, run from the repo root:

```
make mr new <name>          clone: branch agent/<name>, .env copied, shared trees symlinked
make mr ship <name>         land it: rebase onto master → rustfmt touched .rs → fast-forward → delete the clone
make mr ls                  what is on disk and whose move it is
make mr fmt FILES='…'       rustfmt exactly those paths
```

Never bare `cargo fmt` / `cargo fmt --all`: it rewrites files the branch does
not own. Landing is never reproduced by hand. The commands refuse rather than
repair: uncommitted WIP, an untracked `.rs`, a dirty root or a rebase conflict
leaves the clone where it is.

After ship the clone is gone. No copies of code stay in `mrs/` once their value
has landed; at the end of every train `mrs/` is empty.

## Before `make mr ship`: the scaffolding goes back out

A probe is how a claim was proved, not part of the runtime. The last step of a
slice — before `make mr ship`, or before the commit when working on the root —
is to read the branch's **own** diff:

```
git diff origin/master...HEAD --stat
git diff origin/master...HEAD
```

and ask of every added file, test and print: **what breaks tomorrow if this is
gone?** "Nothing" means it goes out now. Nobody deletes it later.

Out:

* **probe code** — a `main` that prints a struct, a temporary `pub fn dump_*`,
  an `examples/` binary that answered its question, a `#[test]` written to call
  one function once;
* **debug leftovers** — `dbg!`, an unflagged `eprintln!`, a commented-out block,
  a feature flag whose only user was the probe.

A probe somebody will rerun lives in the artifact (`<N>-…/probe.rs.txt`), not in
`crates/`. Deleting it does not delete the evidence: the artifact keeps the
output and the verdict. Which tests may stay at all: [PUBLIC-PUSH.md](PUBLIC-PUSH.md).

The pre-commit hook `make mr` installs refuses staged paths under `context/`.
