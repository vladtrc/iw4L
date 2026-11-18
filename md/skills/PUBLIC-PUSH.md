# Public push

Everything about to leave the machine. This pass adds no behaviour: it takes the
tree the agents built and leaves only the runtime — names, comments and tests a
reader who never opened `context/` can live with.

## What ships

The runtime and the documentation needed to use and develop it. The research
base, dumps, third-party checkouts and the work journal do not. That is a rule
about *what* ships, not about what may be said: the use of reverse engineering,
the names of the games studied and the credit owed to projects that were read
are in `README.md` and `NOTICE`, and belong there.

Retail offsets stay out of the code: a standalone runtime resolves nothing
against a retail image, so a hardcoded address or a decompiler name
(`FUN_…`, `DAT_…`) is dead weight wherever it appears.

## The pass

**History.** Unpushed commits are the research path. Soft-reset them onto
`origin/master` and make one commit whose message is the effect, not the
investigation. Fixes, probes, reverts and "try this" do not survive as separate
objects. External contributors' commits are not squashed away — see
[SHIP-TRAIN.md](SHIP-TRAIN.md). Pushed history is never rewritten.

**Tests.** Only `crates/approved_tests`, and only scenarios the owner approved
by name. Everything else goes, however useful — "be strict about tests" means
delete the suite, not trim it. A test you are not sure about goes too.

**Comments.** None by default. A comment stays only if, without it, the next
edit would break something the code cannot show: an ordering, a race, a
lifetime, an upgrade hazard. Everything else is **deleted, not shortened** —
restating the name, narrating what the code does, who calls it, what it
replaced, why it changed, how it was found. Doubt deletes. Never, in any form:
an address, an offset, a function number, a source `file:line`, an engine or
script function name, an iteration tag (`I28.2`, `P08`), "as in the original",
"retail does", "used to", "no longer", "this replaces".

**Names.** IW4L domain. An original identifier that means nothing here is a
citation, not a name. The code's structure is free to differ; resemblance is not
a metric.

**Scaffolding.** Temporary stubs, confidence tables, research scars, dead code.
Out.

## Sequence

```
1. ship every mrs/ clone that is ready
2. on the resulting master: delete, rename, strip — probes and unapproved tests go without asking
3. stop. the human reads the diff
4. one commit, short message, the effect
5. make publish-check
6. push
```

`make publish-check` walks the tracked tree for a fixed set of shapes
(`context/`, `.env`, key material, game-install pieces, offsets, decompiler
names). It proves nothing about origin or licensing, cannot see history or a
release archive (both checked by hand), and misses citations in comments —
passing it is not an argument for anything beyond the absence of those shapes.

The last look at the diff: *does this read as a standalone runtime, or as the
traces of a dig?* The second answer is not a push.
