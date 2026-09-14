# World entities and the funnel

## There is one funnel

```text
human ─┐
bot    ├─→ TickInput ─→ sim::step ─→ Snapshot ─→ client / theater / demo
replay ┘

pub fn step(world: &mut SimWorld, tick: Tick, input: &TickInput,
            msec: i32, reason: StepReason) -> Snapshot;
```

Headless, prediction, replay and the determinism test call exactly this one.
`msec` is the step length; time enters **only** as an argument. The shape of
the funnel is [`SIM-STEP.md`](SIM-STEP.md), the code `crates/sim/src/lib.rs`.

## What can be an entity at all — fixed by the data (`entity_iw4`)

The wiring is ours, the taxonomy comes with the data. The crate neither
serializes nor orders fields "for the network": it pins the layout it reads,
and leaves transport to `net`.

| | |
|---|---|
| `EntityState` | replicated snapshot slot, stride `0x100` |
| `Centity` | client-side slot, stride `0x204` (pose plus snapshot anchors) |
| `ClientState` | per-client slot, stride `0x7c` (command, `name[16]`) |
| `CorpseInfo` | server-side corpse slot, stride `0x53c`, 8 of them |
| `ET_PLAYER` / `ET_PLAYER_CORPSE` / `ET_ITEM` / `ET_MISSILE` | |
| `Trajectory` | `bg_evaluate_trajectory`: gravity 400, delta 800 |
| events | the entity-event ring, an 11-bit wrapper |
| `ENTITYNUM_NONE` | `0x7FF` |
| `playerstate_iw4` | player and input: the two ends of the funnel |

## Where everything else lives

* `sim/` — `gentity spawn world snapshot damage bullet* missile item corpse
  match_state score rules world_objects identities` (`ScriptModelId` is the one
  identity of a script model; a raw `u32` is not a second one);
* `frame` — `AppScreen`, `HasWorld`, `RuntimeRole`, `LaunchIdentity`; no
  plugins, no systems;
* `session` — standing the match up and tearing it down (`MatchInstalled` /
  `MatchTornDown`); `net` — wire, deltas, prediction; `bots` — host-only
  controllers that observe through a sensor adapter, walk a ClipMap-baked graph
  and enter the same `TickInput`. Geometric `sim::step` scenes in `bots` tests
  check maplessly that a `UserCmd` moves.

Snapshots publish the **semantics** of DObj composition, never runtime trees or
posed vertices ([`ANIM.md`](ANIM.md)).
