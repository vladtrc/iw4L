# `sim::step` — the one funnel

Replacing `SimContent` retires body caches and collision history/claims; reinstalling the same owner preserves them.

```rust
pub fn step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
) -> TickResult
```

`crates/sim/src/carrier.rs`. Everything able to change the authoritative world
enters through `TickInput`, including recorded shot-sample provenance. The tick
publishes a `TickResult` containing the snapshot, per-client action outcomes,
script effects and presentation metadata. `try_step` returns
`Result<TickResult, script::Fault>`;
`step` panics on script failure. Authority requires a loaded, started GSC program.

## Who steps the world

Only the authority: `net::authority::runtime` once per server frame and
`net::authority::inbox::run_fixed_authority_stream` for a fixed command stream.
The client keeps no `SimWorld`. It predicts its own player with the pure
`sim::bg::predict_player::predict_player`, over the match content and collision
from `ClientLevel`, and replays unacknowledged commands from each snapshot.

Humans and bots both arrive through `TickInput.cmds`.
* **State is explicit.** `&mut SimWorld` owns the `bevy_ecs` world.
* **Time is an argument.** `tick` and `msec` arrive per call instead of living
  in the world, so re-running a tick means passing it again.
* **Input is canonical.** `TickInput::canonicalize()` sorts commands and
  actions by `ClientId`, so the same set of inputs is the same input whatever
  order the network delivered it in.
* **Publication is closed.** Networking consumes the returned snapshot and
  effects without draining the simulation world. Each action reports applied,
  accepted for script execution, or refused; absence of a rejection is no verdict.

See [`ENTITIES.md`](ENTITIES.md) for snapshots and [`GSC-RUNTIME.md`](GSC-RUNTIME.md) for script execution.

Weapon timer expiry is captured before input-driven state changes. The delayed
pulse also dispatches a newly entered putaway in the same command; a reload
expiry can switch weapons immediately while the incoming raise still gates fire.

Locational collision retains every bone's classification, including bones without
collision bounds. Unclassified attachment bones inherit through the assembled DObj.
Bullet and rifle part priorities resolve within each player, bounded by solid
geometry and shields; they do not change movement bounds or weapon damage.
