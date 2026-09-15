# `make bench` — two reports, one run

`IW4L_BENCH=1` turns on an in-process recorder. At exit it prints two
independent reports and writes them to `iw4l-artifacts/bench/<stamp>.txt`.
Neither needs a trace file or Trace Processor; either can be MISS while the
other stands.

```bash
make bench demo0011              # play a demo   (a goal is always a demo name)
make bench DEMO=demo0011         # the same, spelled out
make bench ZONE=mp_boneyard      # a live map instead
make bench demo0011 PROFILE=release        # the fat-LTO binary
make bench ZONE=mp_rust CMDS='wait world; spawn; wait 20s; quit'
```

**[1/2] Map load** — the waterfall from the shell command to a playable map
(`make` stamps the command time, so cargo and process startup are charged to
the load, not hidden before it), then the load-pool stages. A stage's own
duration says nothing on a parallel walk, so the table also carries
`exclusive`: the time that stage was the *only* one running, which is the part
of the load it alone lengthened. Plus stage memory deltas, the process RSS
peak, the frames drawn behind the loading screen, and a settled screenshot of
the spawned world under `screenshots/bench/<zone>.png`; eyeball it for geometry
that never arrived. A demo gets one too: the overlay comes down as soon as the
first recorded snapshot is presented, so the capture settles during the load.

**[2/2] Frame time** — every typed span from [`PERF.md`](PERF.md), as a tree,
for the frames *after* the first playable one. `n / avg / p50 / p95 / p99 /
max / total / %frame / self`, where `self` is a span's total minus the spans
that nested inside it. The tree is not a table anyone wrote down: a span's
parent is whichever span was open when it began, so moving a system between
sets moves it here. `wall` is the frame clock and encloses nothing — it opens
in one frame and closes in the next — so it is printed as the root everything
else is a share of. A `*` after a name means that span was seen under more than
one parent.

Percentiles come from a log-scale histogram, 16 buckets per octave: exact to
±3.1% of their own value. Counts, sums, minima and maxima are exact. Recording
costs two clock reads and a few relaxed atomic adds per span instance; off, it
is one relaxed load.

A demo ends in `std::process::exit`, so the report is written from an `atexit`
hook rather than after `App::run` returns — the same way the Perfetto flush
survives that exit. On a platform without `atexit` the report needs the process
to return normally, which a live map does and a demo does not.

The bench is not the trace. It answers "where did the run spend its time";
`.pftrace` answers "what happened in this one frame", and `make bench` records
one too, so [`PERF.md`](PERF.md) still applies to the same run.
