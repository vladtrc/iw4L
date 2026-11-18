# `sim::step` — the authority frame

Replacing `SimContent` retires body caches and collision history/claims; reinstalling the same owner preserves them.

```rust
pub fn step(
    world: &mut SimWorld,
    tick: Tick,
    input: &TickInput,
    msec: i32,
) -> TickResult
```

`crates/sim/src/carrier.rs`. `TickInput` carries commands, actions and recorded
shot-sample provenance into the authoritative frame. The tick
publishes a `TickResult` containing the snapshot, per-client action outcomes,
script effects and presentation metadata. `try_step` returns
`Result<TickResult, script::Fault>`;
`step` panics on script failure. Authority requires a loaded, started GSC program.
An error does not roll back mutations already made during the frame.

This is the main gameplay pipeline and publication boundary. World setup and
service preparation also have mutable access to `SimWorld`; the step API does
not make all world mutations exclusive to `TickInput`. An installed input
program can also supply commands during the step.

## Who steps the world

Only the authority: `net::authority::runtime` once per server frame and
`net::authority::inbox::run_fixed_authority_stream` for a fixed command stream.
The client keeps no `SimWorld`. It predicts its own player with the pure
`sim::bg::predict_player::predict_player`, over the match content and collision
from `ClientLevel`, and replays unacknowledged commands from each snapshot.
Authority and prediction share player movement and weapon primitives; prediction
does not execute the full authoritative frame. Demo playback reads recorded
frames and snapshots rather than rerunning the world step.

`crates/sim/src/step.rs::run_frame` calls the phases directly: time, scripts,
actions, players, collision history, entity updates, touches, finalization and
publication. `SimWorld` still owns a Bevy ECS world; these phases do not use an
ECS schedule.

Humans and bots both arrive through `TickInput.cmds`.
* **State is explicit.** `&mut SimWorld` owns the `bevy_ecs` world.
* **Time is explicit.** `tick` and `msec` arrive per call. Player commands carry
  their own timestamps; a frame can process several commands for one client,
  using each command's delta from the player's command time.
* **Input order is defined.** `TickInput::canonicalize()` stably sorts commands
  and actions by `ClientId`, preserving their order within each client. It does
  not make arbitrary network delivery orders equivalent.
* **Publication is closed.** Networking consumes the returned snapshot and
  effects without draining the simulation world. Each action reports applied,
  accepted for script execution, or refused; absence of a rejection is no verdict.

The interface makes frame execution and publication explicit. Reproducing a
frame also requires the same initial world state, content, seeds and ordered
inputs; passing the same tick again does not restore the previous state.
The interface alone establishes neither full determinism nor a performance gain.

See [`ENTITIES.md`](ENTITIES.md) for snapshots and [`GSC-RUNTIME.md`](GSC-RUNTIME.md) for script execution.

Weapon timer expiry is captured before input-driven state changes. The delayed
pulse also dispatches a newly entered putaway in the same command; a reload
expiry can switch weapons immediately while the incoming raise still gates fire.

Locational collision retains every bone's classification, including bones without
collision bounds. Unclassified attachment bones inherit through the assembled DObj.
Bullet and rifle part priorities resolve within each player, bounded by solid
geometry and shields; they do not change movement bounds or weapon damage.
