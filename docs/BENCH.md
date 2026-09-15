# `make bench` — three reports, one run, one package

`IW4L_BENCH=1` turns on an in-process recorder. At exit it prints three
independent reports to `iw4l-artifacts/bench/<stamp>.txt`. None needs a trace
file; any can be MISS while the others stand.

```bash
make bench demo0011              # play a demo   (a goal is always a demo name)
make bench ZONE=mp_boneyard      # a live map instead
IW4L_BENCH_JOBS=assets make bench demo0011   # a load_jobs row per asset
IW4L_BENCH_FRAMES=0 make bench demo0011      # no per-frame table
```

**[1/3] Map load** — the waterfall from the shell command to a playable map, the
load-pool stages, then each image plan's wait split from its decode. A stage's
own duration says nothing on a parallel walk, so the table carries `sole_open`:
how much of the window that stage was the only thing running. It is **not** a
bound on what deleting it would save. A stage's RSS delta is the whole process,
so the deltas do not add up. Match audio is split by decoder, in worker time.

**[2/3] Frame time** — every typed span from [`PERF.md`](PERF.md) as a tree, for
the frames *after* the first playable one, then the rows: first seconds against
steady state (exact percentiles), the worst frames, and `unclassified` — wall
minus the **union** of the parentless spans, so two threads inside one
nanosecond cover it once and the remainder is never clamped. A span is charged
by intersection: a task spanning four frames shows its part in each and the rest
as `carried in`. Nesting comes from each thread's own stack, and the report
counts frames where a span still outlives its parent.

**[3/3] Counters** — render stages, the bodies inside `Present` and `Ui`, the
HUD schedule gaps, GPU passes, per-frame work. A `*_schedule_interval` is a gap
*between* two systems, which the executor is free to fill: compare it against
`hud_tess_body`, never add the two. A counter nobody sampled prints MISS, never
`0.00`; GPU passes overlap and resolve late, so they are never summed.

The run's own directory, beside the `trace.pftrace`:

* `report.txt` — the three reports above;
* `manifest.json` — what the run *was*: revision and dirty-patch hash, binary
  and demo SHA-256, toolchain, adapter, window, render target and the viewport
  the passes were sized by, pools, affinity, env toggles, cache state by kind.
  An absent fact is `null`, never a default;
* `summary.json` — spans, counters and audio totals as data;
* `frames.csv` — one row per frame: each span's overlap with it, every counter,
  `covered_ns`, `carried_in_ns`, flags, thread slots. GPU counters are
  `delivered_*`: a timestamp arrives frames after the work that queued it;
* `load_jobs.csv` — one row per load job: `discovered → plan_ready → enqueued →
  started → decode_started → finished → joined`. `budget_wait_ms` is a worker
  held by the image memory threshold, `decode_ms` the work; `produced_bytes` is
  all the job prepared, against what the merge kept and threw away.
