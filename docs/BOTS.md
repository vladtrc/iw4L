# `bots` — host AI

One pipeline per bot per think tick, all of it on the authority:

```
observation → memory → utility → task → route → motor → UserCmd
```

A bot is an entry in `TickInput.cmds` like a player, so nothing downstream in
`sim` can tell the two apart — [`SIM-STEP.md`](SIM-STEP.md).

| file | owns |
|---|---|
| `observation.rs` / `memory.rs` | what a bot may know, and for how long |
| `sensor.rs` | visibility probes and the rotation that spends them |
| `controller.rs` / `task.rs` / `intent.rs` | scores, task choice, stages |
| `nav.rs` | the baked graph, A\*, resumable route requests |
| `query.rs` | the shared collision budget every subsystem draws on |
| `weapon.rs` / `motor.rs` | the weapon channel; aim and movement |

## The three rules that shape the rest

**A negative needs a probe that ran.** Visibility answers seen, evaluated and
not seen, or not evaluated. Only the second ends a contact; the third holds it,
briefly and without coordinates. Firing needs a positive observation of that
client, fresh on the tick the command goes out.

**A denied query is not a collision.** Perception, connectors, execution and
combat share one 96-query quota. A query that ran reports what it found even on
the last token; one that never ran comes back as denial, so no refusal is ever
fabricated from an empty budget.

**Work survives a tick that is granted nothing.** A route request owns its
phase — approach, attachments, search, validation — and every cursor inside it
is resumable, so a zero allocation repeats nothing and A\* keeps its frontier
under the shared 2048-expansion quota. Answers that follow from graph topology
alone are cached per request key until the graph changes; collision-dependent
refusals are rechecked.

## Movement and combat

The graph bakes world-aligned tiles at 48 units, refined to 24 near missing
support or a height change, with directed Walk, Drop and BreakGlass edges that
keep their entry and exit through execution. A fight moves to a *position*: one
baked support per sector around the threat, checked for line of fire and
exposure, then reached through the ordinary route executor — reachability is
never inferred from a component label.

`HostController::decision()` exposes scores, stage and objective; the periodic
aggregate adds per-subsystem query counts and stuck bots. Both read live from a
running match, which is where bot behaviour is judged: `make map mp_boneyard`
([`RUN.md`](RUN.md)) and watch the aggregate.
