# Headless execution

`utils/scenario_runtime` is a library for caller-owned Rust scenes. It prepares
real map content/GSC and advances the normal `sim::try_step` authority path.
It creates no window, GPU device, audio device, subprocess or console FIFO.
Preparation currently links the existing asset/session dependencies.

The external `.rs` reads its own YAML/JSON file with `read`, constructs a
`RuntimeConfig`, `SceneRecipe`, `SimulationOptions` and input producer, then calls:

```rust
let runtime = Runtime::new(config, &families)?;
let mut session = runtime.prepare("scene-name", recipe, options)?;
session.admit_actor(client, temporary_account)?;
// Join/select classes through ordinary TickInput actions with session.step().
// Or compile actor tracks and session.install_program(primary, program).
let summary = run(RunRequest { build_id, session, input, limits }, &mut log)?;
```

`read_recipe(path)` accepts YAML/JSON; `MergedMap::from_scene` validates typed
recipes without installing a maps/<name>.json file. Scene coordinates, map keys,
actor setup, choreography and input remain outside the published repository.
Paths/rules/input are API values; no env variables forward those values,
and the library changes no working directory. Runtime owns the existing process registries:
paths and family selection are immutable after initialization. Reopening the
same configuration is allowed; an incompatible configuration is refused.

`InputSource` receives an `InputContext`: current tick, read-only world and a
native bot query adapter. `BotInputs` uses the ordinary sensor/HostController,
with explicit query budgets and an optional navigation graph. Bots yield to
installed tapes. `install_program` accepts generic compiler output and uses
native all-actor validation/ownership. Setup mutations belong before capture.

`RunLimits` bounds duration, per-tick entries (including tape-generated commands)
and log bytes. A tick is 50 ms, independent of rendering or wall time. Finish,
cancellation, script faults, incomplete tapes, ownership failures and sink errors
are explicit. Dropping the session shuts down gameplay and releases its owners.
A script-failed session cannot resume stepping.

The sink receives versioned JSONL header/tick/end/failure records: caller build
identity, platform, realized recipe, configuration, seed/content identity, submitted
command bytes, native tape receipts, player/event/shot summaries and complete
protocol-versioned native frames. Action bytes are requests, not individual acceptance acknowledgements.
Frames retain native events, players, projectiles and snapshot metadata. This is
observability, not a full script/world checkpoint. Comparisons require matching
build/content/initial preparation; cross-platform float identity is not promised.
A sink/budget failure can prevent even the failure record from being written;
`Err` is authoritative and a truncated log is never a successful run.
