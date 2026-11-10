# Artifacts

"Артефакт" / "отпиши артефакт" means a folder on disk under the repository-local
`context/artifacts/` ([CONTEXT.md](CONTEXT.md)), never a claude.ai page or Doc.

**Knowledge that is not in an artifact does not exist** for the next agent —
you can `/compact` and forget, and chat is not a record.

## Words

* **artifact** — `context/artifacts/YYYY-MM-DD-<slug>/`: one task, its README,
  its iterations, its evidence, its verdict.
* **iteration** — one slice of work inside an artifact, by one agent, in one
  sitting. An artifact is a stack of iterations.
* **evidence** — what makes a claim true. What counts and what does not:
  [EVIDENCE.md](EVIDENCE.md).

Existing artifacts stay in `context/artifacts/`. They are not moved or rewritten.

## The folder

```
context/artifacts/2026-01-20-example/
  README.md                    living surface: header, one line, pointer to the current slice
  1-OWNERSHIP-AUDIT-FINAL.md   a slice that needed no extra files
  2-REVISIONS-PART/            a slice that did
    README.md
    before.log
    after.log
    probe.rs.txt
```

Artifacts **nest**. A task that grows its own sub-task gets a sub-folder with
its own `README.md`, and every rule here applies to it recursively.

A slice that needs logs, probes, patches or dumps does not scatter them into
the artifact root — it becomes a folder with `README.md` inside. The root stays
readable: a README and a column of iterations.

## Naming iterations

```
<N>-<MEANINGFUL-NAME>-<SUFFIX>
```

`<N>` is the iteration number within this artifact. `<MEANINGFUL-NAME>` says
what the slice was about — `CLIENT-UX`, `REVISIONS`, `PLAYTEST` — not `WORK`,
not `FIXES`, not the date again.

`<SUFFIX>` answers one question without opening anything: **whose move is it
now?**

| suffix | the agent is saying | whose move |
|---|---|---|
| `-FINAL` | the goal of this slice is reached | nobody's. Open the next slice, or close the artifact |
| `-PART` | from the code's point of view something is still left | **another agent**, from where this one stopped |
| `-READY` | done on my side and handed outward, but the next step is not code | **the human**: a playtest, a two-machine round, a decision |

`-FINAL` on work that still needs an agent means the task quietly stops;
`-PART` on work that waits for a playtest sends an agent to redo it. When
unsure, keep the responsibility on your side: `-PART` over `-FINAL`, `-READY`
over `-FINAL`.

Whose move it is right now is the suffix on the **newest** iteration. Earlier
suffixes are not re-stamped.

## The README header

```markdown
---
status: active        # planned | active | blocked | landed | closed | superseded
updated: 2026-01-20
branch: master        # repo + branch or clone, or "—" if not about code
next: what comes next # required for planned / active / blocked
---

# Title

One line (≤200 characters) for someone who never opened the folder.

Current slice: [12-CLIENT-UX-READY](12-CLIENT-UX-READY/README.md).
```

`status` is the artifact's lifecycle; the suffix is whose move. An artifact
stays `active` while its last iteration is `-FINAL` if more slices are coming.
There is no generated index: the artifact READMEs are the index.

## The journal is not rewritten

Closed `<N>-*` files are a **journal, not a wiki.**

* **do not touch** previous iterations or their evidence — not facts, wording,
  verdicts or logs, and do not paste today's correction into them;
* **do update** the artifact `README.md`: header, the one line, the pointer;
* new work is a **new file** `<N+1>-<NAME>-<SUFFIX>`; a correction to an
  earlier slice lives there, as "what the previous slice got wrong";
* living maps (`docs/*` in a repo, a `PLAN.md`) describe the present and are
  updated. Iterations are not.

The one owner-approved exception is deleting disposable raw output:
[CLEANUP.md](CLEANUP.md).

## Handing work over

The slice's report contains:

* what it is based on — trace, probe output, measurement, address from the
  evidence base;
* **what was not done** and where the gaps are. A report without this section
  is incomplete, and this section decides between `-FINAL`, `-PART`, `-READY`;
* whether you fixed a symptom or the disease;
* what scaffolding came back out of the tree and what stayed on purpose. A slice
  that added none says so in one line.

## Finished video renders

Put finished video renders in `context/artifacts/final-renders/` at the
repository root and use that location in delivery links. Keep source recordings,
intermediate renders and verification evidence in their task directories.
Final renders are local outputs and are not committed.
