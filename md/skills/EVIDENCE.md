# Evidence

What makes a claim true, and what only helps ask a better question.

* **evidence** — a trace, a before/after probe, a log, a measurement you can
  rerun, an address + alias from the evidence base. A number copied from a
  neighbouring artifact is a duplicate, not evidence.
* **external reference** — other people's code. It proves **nothing** about
  this runtime. A value taken from a reference and written down as measured or
  as `retail` is defective work.
* **knowledge that is not in an artifact and not in the evidence base does not
  exist** for the next agent.

## Where it lives

Paths are relative to this repository root; nothing here reaches git.

```
context/
  re/          the evidence base: SQLite over retail plus its raw RE (re/README.md)
  externals/   third-party clones, the old project, pdb/ symbols (externals/README.md)
```

`re/`: query with `python3`, the `sqlite3` CLI is not installed here.

`externals/`: retail tells you **which** D3D9 call was made; it does not tell
you what the call means — the first is only `iw4mp.exe`, the second only the
specification and its implementations. `pdb/` (X360 MW2/MW3 symbols) is the one
source carrying IW's own field and function names, but it is a different build
target, so an offset from it is still a hypothesis. Grep its `.map` files for
real names before inferring engine semantics. Acceptance rules per class of
source are in `externals/README.md`.
