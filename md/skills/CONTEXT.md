# Folders and working context

This repository is the IW4L runtime. These tracked instructions describe the
maintainer's workflow; contributors start with `CONTRIBUTING.md` and need no
local artifacts or agent clones to fix the runtime.

Read this file first, then the skill for the task below. Paths are relative to
the repository root unless a link explicitly says otherwise. Commands run from
the repository root.

## Layout

```
AGENT.md               agent entry point
md/skills/             tracked workflow instructions, organized by topic
context/               local working memory, ignored in full
  artifacts/           one folder per task
    final-renders/     finished video renders for delivery
  re/                  evidence base: SQLite over retail and raw RE
  externals/           third-party reference checkouts and symbols
  mrs/                 agent clones, when parallel work needs them
  archive/             historical source documents; not current rules
  cleanup-artifacts.py local artifact cleanup helper
```

**Nothing under `context/` is ever committed**, including through `git add -f`
or a symlink into the tracked tree. A fresh clone has the instructions and no
local working memory. The pre-commit hook installed by `make mr` refuses staged
paths under `context/`; `make publish-check` also checks the tracked tree.

The temporary split workspace in `../iw4l` supplied this topic structure.
This repository keeps its existing monolith layout: artifacts, RE, externals
and agent clones stay here. No sibling repo is needed to read these rules.

## Skills

| skill | owns |
|---|---|
| [CONTEXT](CONTEXT.md) | layout; tracked instructions versus local working memory |
| [ARTIFACTS](ARTIFACTS.md) | artifact folders, iterations, suffixes, journal, handover, final renders |
| [EVIDENCE](EVIDENCE.md) | evidence versus reference; RE base and externals |
| [EXPERT-PRACTICE](expert-practice/SKILL.md) | research experienced practitioners, production outcomes, failures and applicable decision criteria |
| [BURN-THE-BRIDGES](BURN-THE-BRIDGES.md) | delete the lying path, build the target structure, then the bridge |
| [MRS](MRS.md) | agent clones, `make mr`, landing, scaffolding removal |
| [PUBLIC-PUSH](PUBLIC-PUSH.md) | what ships; history, tests, comments, names; publish-check |
| [SHIP-TRAIN](SHIP-TRAIN.md) | PRs, MRs, squash, checks, handover, push, release |
| [PERF-REGRESSION](PERF-REGRESSION.md) | `make perf-compare` and interpreting results |
| [CLEANUP](CLEANUP.md) | retention and removal of disposable artifact output |

One rule lives in one file; other files link to it. `CONTEXT.md` at the root
and the old local instruction paths are entry points to this directory.
