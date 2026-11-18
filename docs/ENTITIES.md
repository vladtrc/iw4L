# World entities and the authority frame

## Authoritative publication

```text
human ─┐
bot    ┴─→ TickInput ─→ sim::step ─→ Snapshot ─→ client / theater / demo

pub fn step(world: &mut SimWorld, tick: Tick, input: &TickInput,
            msec: i32) -> TickResult;
```

Only the authority steps the world; the client predicts its player without one.
`msec` is the world step length; player commands carry their own timestamps.
The frame interface and its boundaries are described in
[`SIM-STEP.md`](SIM-STEP.md); the entry point is `crates/sim/src/carrier.rs`.

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
| `Trajectory` | `evaluate_trajectory`: gravity 400, delta 800 |
| events | the entity-event ring, an 11-bit wrapper |
| `ENTITYNUM_NONE` | `0x7FF` |
| `playerstate_iw4` | player and input: the two ends of the funnel |

## Where everything else lives

* `sim/` — `gentity spawn world snapshot damage bullet* missile item corpse
  match_state score rules world_objects identities` (`ScriptModelId` is the one
  identity of a script model; a raw `u32` is not a second one);
* `frame` — `AppScreen`, `RuntimeRole`, `LaunchIdentity` and `ScopePlugin`: match/round states, scoped resource registration and entity retirement;
* `session` — standing the match up and tearing it down through `ScopeControl`; `net` — wire, deltas, prediction; `bots` — host-only
  controllers that observe through a sensor adapter, walk a ClipMap-baked graph
  and enter the same `TickInput`.

`net/transport/meta_wire.rs` encodes and parses metadata messages. The pure
`world_object_wire.rs` codec validates complete world-object updates;
`world_sync.rs` owns mutable baselines and periodic full synchronization.
Frame decoding commits its prepared world update after reliable and service
sections succeed, so a refused frame leaves the world baseline intact.

Snapshots publish the **semantics** of DObj composition, never runtime trees or
posed vertices ([`ANIM.md`](ANIM.md)).
