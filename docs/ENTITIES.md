# World entities and the funnel

## There is one funnel

```text
human  ─┐
bot     ├─→ TickInput ─→ sim::step ─→ Snapshot ─→ client / theater / demo
replay ─┘
```

```rust
pub fn step(world: &mut SimWorld, tick: Tick, input: &TickInput, msec: i32) -> Snapshot;
```

Headless, prediction, replay and the determinism test call exactly this one.
`msec` is the length of the step; time enters **only** as an argument. Details
in `crates/sim/src/lib.rs`. `step` ≈ `G_RunFrame`; the single funnel is ours.
The comparison is [`GROUNDED.md`](GROUNDED.md).

## What can be an entity at all — a retail fact (`entity_iw4`)

The wiring is ours, the taxonomy is not. The crate neither serializes nor
orders fields "for the network" (E2); every value carries the address it was
read from (E3).

| | |
|---|---|
| `EntityState` | replicated snapshot slot, stride `0x100` |
| `Centity` | client-side slot, stride `0x204` (pose plus snapshot anchors) |
| `ClientState` | per-client slot, stride `0x7c` (command, `name[16]`) |
| `CorpseInfo` | server-side corpse slot, stride `0x53c`, 8 of them |
| `ET_PLAYER` / `ET_PLAYER_CORPSE` / `ET_ITEM` / `ET_MISSILE` |  |
| `Trajectory` | `BG_EvaluateTrajectory`: gravity 400, delta 800 |
| events | the `CG_EntityEvent` ring, an 11-bit wrapper |
| `ENTITYNUM_NONE` | `0x7FF` |

The player and the input are `playerstate_iw4` (`playerState_t`, `usercmd_s`):
the two ends of the funnel.

## Where everything else lives

* `sim/` — `gentity.rs`, `spawn.rs`, `world.rs`, `snapshot.rs`, `damage.rs`,
  `bullet*.rs`, `missile.rs`, `item.rs`, `corpse.rs`, `match_state.rs`,
  `score.rs`, `rules.rs`, `world_objects.rs`, `identities.rs` (`ScriptModelId`
  is the one identity of a script model; a raw `u32` does not get to be a second);
* `frame` — frame and session markers: `AppScreen`, `HasWorld`, `RuntimeRole`,
  `LaunchIdentity`. No plugins, no systems;
* `session` — standing the match up and tearing it down (`MatchInstalled` /
  `MatchTornDown`); `net` — the wire, deltas, prediction; `bots` — they see
  exactly their own `Snapshot`.

The snapshot publishes the **semantics** of DObj composition, not runtime trees
and not posed vertices — see [`ANIM.md`](ANIM.md).
