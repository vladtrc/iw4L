# Did performance regress?

Run from this repository root (`make perf-compare`).

One command per question. The first hash is the baseline; every later hash is a candidate.

```
make perf-compare <base> <candidate>...        # ROUNDS=3 by default
make perf-compare fc3d8eaf acc57a5c ROUNDS=5
make perf-compare HASHES='fc3d8eaf HEAD' REPLAY=iw4l-artifacts/approved-tests/<run>/run.json
```

What it does (`xtask/src/perf_compare.rs`):

1. Resolves each hash. It builds `--profile play -p launcher --locked`, offline first, in one fixed worktree `target/perf-compare/tree` with its own target dir. The binary is cached at `iw4l-artifacts/perf-compare/bin/<sha>/iw4l`; a cached hash is never rebuilt.
2. Runs `heavy_gameplay_lifecycle` with the root tree's `approved_tests` runner on every binary, with `IW4L_BENCH=1`, under `gnome-session-inhibit`.
3. The first warmup run on the baseline uses a fixed seed. Its `run.json` becomes the `--replay` for every measured run, which pins map B, spawns and classes. The warmup is discarded.
4. Rounds alternate the order (A B, B A, …), so drift hits both arms.
5. Each run samples `/proc/<pid>/status` every 0.1 s for peak RSS+swap. It then reads the trace to get frame stats for gameplay only: `wall`, `render_thread`, `Present` and `FixedUpdate` inside the six `*.spawned` windows. It also records `b.load`, and RSS at `a.scene_1.spawned` and `b.scene_3.end`.

Output: `iw4l-artifacts/perf-compare/<stamp>/`.

- `report.md` has a mean [min–max] per hash and Δ vs the baseline. A metric is marked `separated` when the ranges do not overlap. A verdict line closes the report.
- `hashes/<short>.md` has the per-run rows for one hash.
- `runs/<NN>-<short>/` holds `runner.log`, `memory.tsv` and `metrics.json`. Each names its approved-tests run, which holds the trace.
- `series.json` holds everything in machine form.
- One line per series is appended to `iw4l-artifacts/perf-compare/index.md`.

## Reading it

- A regression is wall mean/p50/p90 ≥ 5 % with separated ranges, or RSS ≥ 256 MiB. Anything below that is noise, or not worth a change.
- Do not use `cargo xtask bench` p50 for this scenario. Its active window is half menu/load frames, so the median is bimodal and jumps on menu fps alone ([2026-10-16 slice 5](../../context/artifacts/2026-10-16-ship-train/5-MEDIAN-CAUSE-READY/README.md)).
- Only arms inside one series compare. To compare against an older measurement, put the old hash in the new series; never reuse the old report.
- Discard a series where frames sit at 16.7 ms on every scene. That is the display idling at 60 Hz.
- With ROUNDS=1, ranges are single points. Use it for smoke runs only; 3 is the minimum for a verdict.

## Digging into a delta

The trace for a run is under `<approved run>/iw4l-artifacts/runs/<uuid>/trace.pftrace`. Group spans by phase as in slice 5's `sql/gameplay.sql`, then compare child spans between arms:

```
cargo xtask query <trace> context/artifacts/2026-10-16-ship-train/5-MEDIAN-CAUSE-READY/sql/gameplay.sql
cargo xtask query <trace> "SELECT name, COUNT(*), AVG(dur)/1e6 FROM slice GROUP BY name ORDER BY 3 DESC LIMIT 30"
```

A bisect is the same command with more hashes. Each hash is built once, so a rerun with a different set only builds the new ones.

## Limits

- The runner and the scenario come from the root tree, not from the hash. A hash too old to understand the current scenario's console commands fails its runs, and those runs show as FAILED in the report.
- Hashes resolve in the root repo, so an unpushed commit from an `mrs/` clone has to be fetched first.
