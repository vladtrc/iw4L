Workflow, artifacts and the glossary: `CONTEXT.md`. Do not touch other agents'
`context/mrs/*` without being asked.

What is published is the runtime and the documentation needed to use and
develop it. The local research base, dumps, third-party checkouts and the work
journal are not. That is a rule about *what* ships, not about what may be said:
the use of reverse engineering, the names of the games studied and the credit
owed to the projects that were read are not hidden — they are in `README.md`
and `NOTICE`, and belong there.

Retail offsets stay out of the code for their own reason: a standalone runtime
resolves nothing against a retail image, so a hardcoded address or a decompiler
placeholder name (`FUN_…`, `DAT_…`) is dead weight wherever it appears.

`make publish-check` walks the tracked tree for both. It is a grep over a fixed
set of shapes — it proves no claim about origin or licensing, and passing it is
not an argument for anything beyond the absence of those shapes.
