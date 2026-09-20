# Contributing

IW4L is a personal, experimental project. It is open so that it can be read,
built and fixed — not because it comes with an obligation to serve every
request. Decisions are the maintainer's.

## Bug reports

Reports are welcome, with no promised fix date. A report is prioritised against
what the project actually claims:

> A failure in a scenario the project says works outweighs a missing feature it
> never promised.

`README.md` says what is claimed. "Add everything the original had" is not a
roadmap item.

What a useful report contains, and nothing more:

* the release tag or commit
* OS and GPU
* which game's data you pointed IW4L at, and which map
* the steps, and what happened instead

That is the whole list. Do not attach your `.env`, a memory dump, or an archive
of the game — none of those will be read, and the last one cannot be accepted.

## Changes

* **Small and self-contained** — a fix, a crash, a wrong constant, a doc
  correction: open it directly.
* **Architectural** — a new crate, a new subsystem, a change to how data flows
  between `sim`, `render_frontend` and `net`: open an issue first. A large
  branch that arrives unannounced is likely to be turned down for reasons that
  have nothing to do with its quality.

Before opening anything:

```bash
make publish-check   # the tracked tree is the product and nothing else
cargo fmt --all
cargo clippy --workspace --all-targets
```

`CONTEXT.md` documents how the maintainer works — artifacts, iterations, agent
clones. It is a maintainer's workflow, not a requirement for contributing: you
do not need `make mr`, the iteration naming, or anything under `context/` to
change the runtime.

## Game data

Never attach, commit or link original game files — maps, weapons, sounds,
executables. That holds for issues, pull requests and release assets alike.
IW4L reads data from an installation each user already owns, and the project is
not a distribution channel for it.

## Written by an LLM

This whole project is written by an LLM.
