# approved_tests

APPROVED TEST POLICY

Only explicitly owner-approved end-to-end scenarios may exist in this crate.
Agents may create temporary tests and probes during development, but those
must be deleted before the final squash/push unless specifically approved. A
useful test is not automatically an approved test.

A new permanent scenario is added here only after the owner has approved that
scenario by name, as a scenario the runner drives, never as a `cargo test`.
`make publish-check` refuses `#[test]`, `#[cfg(test)]`, `mod tests` and `tests/`
directories everywhere in the workspace, this crate included.

## Approved scenarios

| name | what it does |
| --- | --- |
| `master_duo_chaos` | local master, maximal room adverts through QUIC and GUI, two clients moving and fighting on `mp_boneyard`; optional composed-scene admission and environment boundaries |
| `heavy_gameplay_lifecycle` | cold load `mp_overgrown` with 16 players, three input scenes, production disconnect, a watched menu, a second map with three scenes, production quit |

Heavy gameplay steps live in `src/scenarios/heavy_gameplay_lifecycle.rs`: maps, seed rule, scenes, positions, angles, weapons, durations and input. Master steps live in `src/scenarios/master_duo_chaos.rs` and its boundary module.

The heavy scenario uses a 2560×1440 window with vsync off, recorded alongside the observed render target in `run.json`.

The heavy scenario also plants a claymore, drops a weapon through player death,
restarts the script round in place, submits 8192 sound aliases to exercise explicit pressure refusal, and captures entity and resource ownership at gameplay and teardown boundaries. The runner checks simulation advancement under audio pressure and the actual uploaded static-model index owner on both maps, including replacement across the menu boundary.
Both scenarios use isolated accounts and wait for native progression content and run `unlock all` from the menu before
loading their map, so random showcase classes are admitted without altering a
user account. Dumps record slot, payload, script-object and remote-animation counts; the runner
checks entity consistency and complete teardown of these match owners.

The final `quit` phase sends `finish_run`, which uses the normal process-exit
boundary after the scenario's pending screenshots finish writing. Interactive
`quit` intentionally has only a short write grace period and can leave a
scenario-owned PNG incomplete under load.

## Running

```sh
make approved                                  # cold cache, fresh seed
make approved ARGS='--seed 1234'               # fixed seed
make approved ARGS='--replay iw4l-artifacts/approved-tests/<run>/run.json'
make approved ARGS='--cache shared'            # reuse the repo cache (not cold)
```

The runner builds nothing; `make approved` builds `target/play/iw4l` first.
Each run owns one directory, `iw4l-artifacts/approved-tests/<run-id>/`.
`heavy_gameplay_lifecycle` starts the game with that directory as its working
directory, so everything
IW4L writes (`iw4l-artifacts/cache`, logs, perf run, dumps, screenshots) lands
under it. On Windows the launcher enters the folder holding its executable, so
the runner hard-links (or copies) the binary into the run directory and starts
that one. With `--cache cold` that cache starts empty; with `--cache shared`
it is a symlink to the repository's cache.

A seed resolves to maps and spawn picks once. `run.json` keeps the resolved
set — the positions and angles the game actually used — and `--replay` runs
that set again rather than the seed, so a change in catalog order or spawn
selection cannot move a replayed scene. Replay also restores the source run’s
`iw4l-artifacts/profile/classes.txt` so class indices keep the same loadouts.
The source profile must exist; its contents and path are recorded under
`class_profile` in the new report. Combat outcomes and frame timing can still
vary between runs.

## What a run leaves

- `run.json` — build, platform, render, content, maps, seed, the resolved
  scenes and the exact command script, cache mode, bot target and actual,
  lifecycle timestamps, phases with start/end or a failure reason, the GSC
  program each map installed (or the refusal that stopped it), liveness
  assertions, and paths to the large artifacts.
- `00_cold_load.png`, `01_overgrown_gameplay.{png,dump.txt}`,
  `02_after_disconnect.{png,dump.txt}`, `03_second_map_gameplay.{png,dump.txt}`
  when the run got that far.
- `child_stdout.txt`, `child_stderr.txt`, and the game's own
  `iw4l-artifacts/logs/latest.log` and `iw4l-artifacts/runs/<id>/` perf run.

Gameplay assertions check that the map loaded, its GSC program installed, the
players exist, simulation advanced, the authority applied the scene's movement
commands to the local player, the player covered a path of at least 32 units or
died, the lifecycle boundaries happened in order, no script fault ended the
process, and `quit` ended the process on its own.

Every match runs a GSC program, so the game announces it on stdout the way it
announces lifecycle boundaries: `gsc: installed map=… gametype=… fingerprint=…
modules=… functions=… natives=… entries=…` when the world is published, or
`gsc: refused map=… stage=compile|install|entry fault="…"` when the match load
stopped at the script. `run.json` keeps both under `gsc.a` / `gsc.b` and asserts
`a.gsc_installed`, `b.gsc_installed` and `gsc.no_execution_fault`. A phase that
fails after a refusal carries the refusal as its `failure_reason`, and a script
fault that ends the process (`GSC execution failed: …` in `child_stderr.txt`)
becomes the run's `failure`.

Movement is three facts, not one. `mark` reports the local player's input
receipt — commands the authority applied, how many of them carried a move, and
the summed length of every applied move — so a scene that turns while it walks
is judged by the path it walked, not by where it ended up. Each scene records an
`outcome`: `moved`, `died`, `died_before_input`, `held_in_place` (commands
applied, alive, no path: collision or a frozen player) or `input_not_applied`
(the break the scenario exists to catch). There
is no frame-time assertion. A run the controller had to kill is a failure,
never a quit.

## Two clients and master boundaries

```sh
make approved SCENARIO=master_duo_chaos
make approved SCENARIO=master_duo_chaos ARGS='--scene /path/to/realized-scene.json'
# Already built binaries:
target/play/approved_tests master_duo_chaos --bin target/play/iw4l --master-bin target/play/iw4l-master
```

This Linux scenario requires OpenSSL. It starts its own master on a temporary
localhost UDP port with a fresh CA and server certificate. `make approved`
builds both binaries. Games, accounts, settings, certificates, descriptors and
artifacts belong to the run directory. The runner stages a private game binary
and selects its local descriptor for each process. It starts at most two game
processes at once.

Two protocol hosts publish legal maximal inventories: 64 families, including
63 tokens of 64 bytes, in both `requires` and `available`. A protocol reader
checks two bounded pages, complete inventories and a following Status RPC on
the same connection. A real GUI browser refreshes twice; its dumps must contain
both exact inventories and both native row IDs. Display cells stay bounded.
Those rooms are closed before gameplay begins.

The host then creates a lobby on `mp_boneyard`, a second client joins, and both
load it once. Native Intervention classes avoid dependence on random showcase
class slots and enabled foreign families. Four seconds of movement exercise
the remote command path, followed by sixteen seconds of RPG chaos with seven
bots. Assertions check distinct seats, replicated player count, simulation and
remote movement, one map installation, script health and process exit.

`--scene` extends that same run with a realized scene recipe. Generate it with
the content tooling for the installed maps; the runner uses the runtime's typed
SceneRecipe parser. Environment coverage needs an IW4 root connected directly
to two T5 maps with different sun, fog and exposure, plus a two-sided portal
whose flip can be toggled while retaining a valid scene.

The host visits each T5 map directly and through its root portal. Dumps retain
physical WorldId, authored environments and actual shader constant rows;
assertions compare sun/exposure to the physical world, fog/sun/exposure between
direct and portal views, and distinct same-family banks. Two independent
clients then join the running host sequentially, with equal map fingerprints
and live seats. A third client receives the same recipe with only one portal
flip changed and must be rejected for map-content mismatch. One held bot keeps
the host match running between clients. The host must survive the rejection.
Without `--scene`, the report contains only the browser and native-map cases.

Each role keeps commands, stdout/stderr, game logs, dumps, screenshots and
native Perfetto traces. `run.json` records binary facts, exact scene input,
pages, inventories, UI dvars, shader banks and boundary assertions. Every game
exits through `finish_run`; a timeout or forced termination fails the run.
