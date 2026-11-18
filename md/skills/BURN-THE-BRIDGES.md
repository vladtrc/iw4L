# Burn the bridges

Not "a second path next to the first", but:

1. **physically delete the path that lies** — the one that looks like it works;
2. **put the target end-goal structures in place**, even if nothing can connect
   to them yet;
3. **then build the bridge.**

While the lie is in the code the right path will not assemble. Every adapter,
shim, `#[deprecated]` alias, feature flag or "temporary" fallback that keeps the
old path compiling is a reason for the next edit to go through the old path.

## How to apply

* Name the lie first. Write down which type, enum, table, flag or dvar pretends
  the old shape still holds. That list is the delete list.
* Delete before building. The tree going red is the expected state, not a
  failure. Compiler errors are the to-do list — they point at every place that
  used the lie, which a grep never fully does.
* The target structure lands in its final home with its final name, not in a
  staging module that will "move later". Later it looks load-bearing.
* No compatibility layer for the old shape: no conversion `From<Old>`, no
  re-export under the old name, no dual-read of old and new formats, no
  protocol negotiation for old peers. Bump the version by one and move on
  (there are no users to keep; old builds are a reference, not a contract).
* Bridges are built from the target side: each caller is rewritten to the new
  structure, not the new structure bent to fit the caller.
* Keep a way back as **a git tag**, not as code. Rollback is
  `git checkout <tag>`, never a branch of the runtime.

## What it does not mean

* Not "delete without looking". The delete list is written down in the
  artifact before the delete, with the evidence of what used each item.
* Not "skip verification". The bridge is done when the target works, measured
  the usual way (approved tests, a map per game, a screenshot) — just not
  against a promise that the intermediate states keep working.
* Not "rewrite history". Pushed history stays; burning is about the tree.
