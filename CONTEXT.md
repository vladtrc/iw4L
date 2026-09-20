# `context/` — working memory outside git

This repository is the runtime. Everything an agent needs that is *not* the
product — the journal of how a change got made, third-party clones kept for
reference, per-agent clones of the repo — lives under `context/`, which is
ignored in full. Nothing there is ever committed.

This file is the contract for that folder: it is tracked, and `context/` is not.
A fresh clone therefore has these rules and an empty `context/`, which is the
intended state — the folder fills up as work happens on a machine.

This is how the **maintainer** works, and it is tracked so that an agent
picking up the repository finds it. It is not the project's contribution
interface: fixing the runtime needs none of it — no `make mr`, no iteration
naming, no `context/` at all. `CONTRIBUTING.md` is that door.

Read in this order: this file → `AGENT.md` → `docs/INDEX.md`.

## Layout

```
context/
  artifacts/     one folder per task; the journal of how we got here
  externals/     third-party clones, kept for reference. Never a source of truth.
  mrs/           agent clones of the repo, one per parallel agent
```

## Glossary

* **artifact** — a folder `context/artifacts/YYYY-MM-DD-<slug>/` holding one
  task: its README, its iterations, its evidence, its verdict. On disk, not in
  git. **Knowledge that is not in an artifact does not exist** for the next
  agent — you can `/compact` and forget, and chat is not a record.
* **iteration** — one slice of work inside an artifact, by one agent, in one
  sitting. It is the unit that gets a name and a suffix; see
  [naming](#naming-iterations). An artifact is a stack of iterations.
* **evidence** — what makes a claim true: a trace, a before/after probe, a log,
  a measurement you can rerun. A number copied from a neighbouring artifact is a
  duplicate, not evidence.
* **external reference** — a clone under `context/externals/`. Other people's
  code. It proves **nothing** about this runtime; at most it helps you ask a
  better question. A value taken from a reference and written down as measured
  is defective work.
* **mrs** — `context/mrs/<name>/`: a clone of the repo for one agent. A single
  agent on the root needs **no** clone; a clone exists only when someone else is
  already working on the root. The agent's territory is code and `git commit`
  *inside the clone*; landing is a single command from the root (rebase onto
  master → rustfmt the touched `.rs` → fast-forward → delete the clone), never
  reproduced by hand. **Other agents' `mrs/*` are not to be touched without
  being asked.** One verb drives the lifecycle, `cargo xtask mr` underneath:
  `make mr new <name>` (clone: branch `agent/<name>`, `.env` copied, the shared
  trees symlinked in), `make mr ship <name>` (land it), `make mr ls` (what is on
  disk and whose move it is), and `make mr fmt FILES='…'` (rustfmt exactly those
  paths — `cargo fmt --all` rewrites files the branch does not own). They refuse
  rather than repair: uncommitted WIP, an untracked `.rs`, a dirty root or a
  rebase conflict leaves the clone where it is.
* **burn the bridges** — not "a second path next to the first", but: physically
  delete the path that lies (the one that looks like it works), put the target
  end-goal structures in place even if nothing can connect to them yet, then
  build the bridge. While the lie is in the code the right path will not
  assemble.

## Artifacts

### The folder

```
context/artifacts/2026-01-20-example/
  README.md                              living surface: header, one line, pointer to the current slice
  1-OWNERSHIP-AUDIT-FINAL.md   a slice that needed no extra files
  2-REVISIONS-PART/            a slice that did
    README.md
    before.log
    after.log
    probe.rs.txt
```

Artifacts **nest**. A task that grows its own sub-task gets a sub-folder with
its own `README.md`, and every rule here applies to it recursively.

### Naming iterations

Every file or folder an agent produces inside an artifact is named

```
<N>-<MEANINGFUL-NAME>-<SUFFIX>
```

`<N>` is the iteration number within this artifact. `<MEANINGFUL-NAME>` says
what the slice was about — `CLIENT-UX`, `REVISIONS`, `PLAYTEST` — not `WORK`,
not `FIXES`, not the date again.

`<SUFFIX>` is the handover signal. It exists to answer one question without
opening anything: **whose move is it now?** The human reads it to decide between
launching another agent, going to verify, and moving on — so it says where the
responsibility sits, not how the agent feels about its work.

| suffix | the agent is saying | whose move |
|---|---|---|
| `-FINAL` | the goal of this slice is reached | nobody's. Open the next slice, or close the artifact |
| `-PART` | from the code's point of view something is still left | **another agent**, from where this one stopped |
| `-READY` | done on my side and handed outward — code written, gameplay run, builds green — but the next step is not code | **the human**: a real playtest, a two-machine round, a decision |

Getting this wrong is the expensive failure. `-FINAL` on work that still needs
an agent means the task quietly stops; `-PART` on work that is actually waiting
for a playtest sends an agent to redo what was done. When unsure between two,
pick the one that keeps the responsibility on your side of the fence — `-PART`
over `-FINAL`, `-READY` over `-FINAL`.

A slice that needs no extra files is a single `.md`. A slice that needs logs,
probes, patches or dumps does **not** scatter them into the artifact root — it
becomes a folder with `README.md` inside and everything else next to it. The
artifact root stays readable: a README and a column of iterations.

Whose move it is on an artifact right now is the suffix on its **newest**
iteration. Earlier suffixes are not re-stamped: a `-PART` does not become
`-FINAL` when the next agent finishes the job — the later `-FINAL` already says
the work is closed, and the `-PART` above it is the record of why it took two
passes. Going back to rename it would delete that and change nothing about where
the responsibility sits today.

### The README header

Each artifact carries its own header and one line under the title. That line
(≤200 characters) is what a reader sees in a listing, so write it for someone
who has never opened the folder.

```markdown
---
status: active        # planned | active | blocked | landed | closed | superseded
updated: 2026-01-20
branch: master        # branch or clone, or "—" if the artifact is not about code
next: what comes next # required for planned / active / blocked
---

# Title

One line about what this is.

Current slice: [12-CLIENT-UX-READY](12-CLIENT-UX-READY/README.md).
```

`status` is the artifact's lifecycle; the suffix is whose move it is. They
answer different questions, so an artifact stays `active` — more slices are
coming — while its last iteration is `-FINAL` and nobody is blocked. There is no
generated index: the artifact READMEs are the index.

### The journal is not rewritten

Closed `<N>-*` files are a **journal, not a wiki.** New understanding does
not fix an old file. An intermediate formulation that turned out to be wrong
stays exactly where it is, so the next agent sees how the understanding moved
instead of smooth text pretending we always knew.

* **do not touch** previous iterations or their evidence. Do not tidy up facts,
  wording, verdicts or logs, and do not paste today's correction into them;
* **do update** the artifact `README.md`: header (`status` / `updated` /
  `next`), the one line, the pointer to the current slice. The README is the
  living surface;
* new work is a **new file** — `<N+1>-<NAME>-<SUFFIX>`. A correction to
  an earlier slice lives there, as "what the previous slice got wrong";
* living maps (`docs/*`, a `PLAN.md`, a `WHATS-LEFT.md`) describe the present
  state, not the journal. Those are updated. The iterations are not.

## Before shipping: the scaffolding goes back out

A probe is how a claim was proved. It is not part of the runtime, and the
moment it stops being read it is a file the next agent has to understand before
they may ignore it.

So the last step of a slice, before `make mr ship` — or before the commit that
lands on master when the agent works on the root and has no clone — is to read
the branch's **own** diff:

```
git diff origin/master...HEAD --stat      # what the branch adds at all
git diff origin/master...HEAD             # then the hunks, for real
```

and to ask of every added file, test and print: **what breaks tomorrow if this
is gone?** "Nothing" means it goes out now, while the context to judge it is
still in your head. Nobody deletes it later — later it looks load-bearing.

Out:

* **probe code** — a `main` that prints a struct, a temporary `pub fn dump_*`,
  an `examples/` binary that already answered its question, a `#[test]` written
  to call one function once and eyeball the output;
* **a test that only asserts the code ran** — no expected value, or an expected
  value copied from what the code printed today. It cannot fail for a reason
  anyone cares about; it buys a false green and costs the run time forever;
* **debug leftovers** — `dbg!`, an `eprintln!` behind no flag, a commented-out
  block kept just in case, a feature flag whose only user was the probe.

Stays: a test that would have caught the bug this iteration fixed, and a probe
somebody will rerun to reproduce a measurement — and that one lives in the
artifact (`<N>-…/probe.rs.txt`), not in `crates/`.

Deleting a probe does not delete the evidence. The artifact keeps the output,
the log and the verdict, which is what the next agent reads anyway; the
disposable half of it does not need to be in git to have been true.

## Making a public push

The slice rule above is this branch. This pass is everything about to leave
the machine. It adds no behaviour. It takes the tree the agents built and
leaves only the runtime: names, comments and tests that a reader who never
opened `context/` can live with.

**History.** Unpushed commits are the research path. Soft-reset them onto
`origin/master` and make one commit whose message is the effect, not the
investigation. Fixes, probes, reverts and "try this" do not survive as
separate objects. History that has already been pushed is not rewritten —
[`docs/DEPLOY.md`](docs/DEPLOY.md).

**Tests.** Disposable by default. A test stays when rewriting it would cost
more than keeping it: cross-system behaviour, a network contract, a state
machine, a bug that took a week to see. Helper tests, agent self-checks,
one-shot snapshots go out. Doubt deletes.

**Comments.** Current architecture only. No address, no offset, no function
number, no "as in the original", no diary of how it was found. If deleting
the comment loses nothing, it was already noise.

**Names.** IW4L domain. An original identifier that does not mean anything
here is a citation, not a name, and it does not land. The structure of the
code is free to differ; resemblance is not a metric.

**Scaffolding.** Temporary stubs, confidence tables, research scars, dead
code. Out.

The last look at the diff is one question: *does this read as a standalone
runtime, or as the traces of a dig?* The second answer is not a push.

```
1. ship every mrs/ clone that is ready
2. on the resulting master: delete, rename, strip
   — probes go without asking
   — a test you are not sure about goes too
3. stop. the human reads the diff
4. one commit, short message, the effect
5. make publish-check
6. push
```

## Handing work over

What the report has to contain: what it is based on (the trace, the probe
output, the measurement); what was **not** done and where the gaps are; whether
you fixed a symptom or the disease. **A report without the "what was not done"
section is incomplete** — and that section is what decides between `-FINAL`,
`-PART` and `-READY`.

It also says what came back out of the tree and what stayed on purpose — which
probes were removed, which test is there because it would have caught this bug.
A slice that added no scaffolding says that in one line; it is an answer, not an
omission.

## git

```gitignore
/context/
```

The whole folder, with no exceptions. `git add -f` is not an escape hatch: the
pre-commit hook that every `make mr` command installs refuses a staged path
under `context/` a second time. This file is the contract; the rest is working
memory.
