# approved_tests

APPROVED TEST POLICY

Only explicitly owner-approved end-to-end scenarios may exist in this crate.
Agents may create temporary tests and probes during development, but those
must be deleted before the final squash/push unless specifically approved. A
useful test is not automatically an approved test.

A new permanent scenario is added here only after the owner has approved that
scenario by name. The same holds for a unit test inside this crate: being
useful does not make it approved. `make publish-check` refuses `#[test]`,
`#[cfg(test)]`, `mod tests` and `tests/` directories anywhere else in the
workspace.

## Approved scenarios

| name | what it does |
| --- | --- |
| `heavy_gameplay_lifecycle` | cold load `mp_overgrown` with 16 players, three input scenes, production disconnect, a watched menu, a second map with three scenes, production quit |

The scenario's steps are in `src/scenarios/heavy_gameplay_lifecycle.rs` and
nowhere else: maps, seed rule, scenes, positions, angles, weapons, durations
and input. To pin a scene to an owner-chosen position, replace its `Place`
there; the runner does not change.

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
Each run owns one directory, `iw4l-artifacts/approved-tests/<run-id>/`. The
game is started with that directory as its working directory, so everything
IW4L writes (`iw4l-artifacts/cache`, logs, perf run, dumps, screenshots) lands
under it. With `--cache cold` that cache starts empty; with `--cache shared`
it is a symlink to the repository's cache.

A seed resolves to maps and spawn picks once. `run.json` keeps the resolved
set — the positions and angles the game actually used — and `--replay` runs
that set again rather than the seed, so a change in catalog order or spawn
selection cannot move a replayed scene.

## What a run leaves

- `run.json` — build, platform, render, content, maps, seed, the resolved
  scenes and the exact command script, cache mode, bot target and actual,
  lifecycle timestamps, phases with start/end or a failure reason, liveness
  assertions, and paths to the large artifacts.
- `00_cold_load.png`, `01_overgrown_gameplay.{png,dump.txt}`,
  `02_after_disconnect.{png,dump.txt}`, `03_second_map_gameplay.{png,dump.txt}`
  when the run got that far.
- `child_stdout.txt`, `child_stderr.txt`, and the game's own
  `iw4l-artifacts/logs/latest.log` and `iw4l-artifacts/runs/<id>/` perf run.

Assertions are liveness only: the map loaded, the players exist, simulation
advanced, input moved the player or the player died, the lifecycle
boundaries happened in order, and `quit` ended the process on its own. There
is no frame-time assertion. A run the controller had to kill is a failure,
never a quit.
