# Evidence

What makes a claim true, and what only helps ask a better question.

## Reinterpretation of game behavior

Implement the observable script contract and a similar gameplay effect using
this runtime's own design. Binary analysis can establish inputs, outputs,
errors, timing and gameplay constraints; it does not require reproducing the
original algorithm, memory layout or implementation.

Prefer the existing SP research, Xbox PDB types/symbols and corresponding
binary when recovering a missing capability's decision rules and state
transitions. Establish that behavior before inventing a substitute; implement
it through this runtime's own state and services. Xbox layout and build-specific
details remain hypotheses for PC and need checking against the PC script contract.

Do not translate decompiled functions into production code or transplant
unexplained lookup tables, address-dependent rules or collections of magic
constants. In particular, physics may use a different coherent model with
similar visible results. Give parameters meaningful names, units and reasons,
and verify the resulting behavior rather than instruction or table parity.
Documented script names, enum values and wire formats remain compatibility
contracts; authored asset data remains data. Neither is a justification for
copying an internal algorithm.

For GSC gap work, inventory and implement related capabilities in batches.
Use compilation, contract probes and headless simulation before playtesting.
Registered no-ops, unconditional success and fabricated world state do not
close a gap. Track unbound calls, partial implementations, missing fields and
engine services separately. Rendered playtests remain necessary for final
visual and mission acceptance, and should not be repeated after each small
implementation change.

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
