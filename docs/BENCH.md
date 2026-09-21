# `make bench` — three reports, one run, one package

`IW4L_BENCH=1` turns on an in-process recorder. At exit it prints three
independent reports to `iw4l-artifacts/bench/<stamp>.txt`. None needs a trace
file; any can be MISS while the others stand. Benchmarks keep running when
unfocused; the window runner does not impose its background 60 Hz sleep.

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
minus the **union** of the frame's declared top-level spans, so two threads
inside one nanosecond cover it once and the remainder is never clamped. A span
is charged by intersection: a task spanning four frames shows its part in each
and the rest as `carried in`. The frame clock closes and reopens at the top of
`First`, before anything else runs, so every main-phase interval falls wholly
inside one frame; the frame the process exits from is closed at exit and its
row carries `partial`. Which spans are roots is declared, not read off a
thread's stack — an executor is free to run a schedule's `begin` and its `end`
on different workers, and the report counts those as `migrated`. Nesting is
still observed, and the report counts frames where a span outlives its parent.

`main/render overlap` is the part of the wall a main schedule and the render
thread were both inside, from their intervals. It is zero whenever rendering is
serialised (`IW4L_PIPELINED_RENDERING=0`). By default the render world runs
one frame behind on its own thread. Extraction remains a synchronised boundary;
`scheduling` in the manifest records the mode. Compare completed render cadence
and `presented_state_age` alongside main-loop wall time.

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
  `covered_ns`, `main_covered_ns`, `carried_in_ns`, flags, thread slots. GPU
  counters are `delivered_*`: a timestamp arrives frames after the work that
  queued it;
* `load_jobs.csv` — one row per load job: `discovered → plan_ready → enqueued →
  started → decode_started → finished → joined`. `budget_wait_ms` is a worker
  held by the image memory threshold, `decode_ms` the work; `prepared_bytes` is
  what the job decoded itself and `reused_bytes` what it served out of another
  job's payload without decoding anything, against what the merge kept and threw
  away. Only `prepared_bytes` is work.

A diagnostic build with `--features bevy-trace` writes Bevy's own `tracing`
spans — the ones inside the render graph, `queue_submit` among them — as a
Chrome trace. It costs the frame it measures, so it is never a timed run;
`scheduling.bevy_tracing` in the manifest says whether a run was one.
