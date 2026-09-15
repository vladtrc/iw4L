# `make bench` — three reports, one run

`IW4L_BENCH=1` turns on an in-process recorder. At exit it prints three
independent reports and writes them to `iw4l-artifacts/bench/<stamp>.txt`.
Neither needs a trace file or Trace Processor; any can be MISS while the others
stand.

```bash
make bench demo0011              # play a demo   (a goal is always a demo name)
make bench ZONE=mp_boneyard      # a live map instead
make bench demo0011 PROFILE=release        # the fat-LTO binary
make bench ZONE=mp_rust CMDS='wait world; spawn; wait 20s; quit'
```

**[1/3] Map load** — the waterfall from the shell command to a playable map
(`make` stamps the command time, so cargo and process startup are charged to the
load), then the load-pool stages. A stage's own duration says nothing on a
parallel walk, so the table carries `exclusive`: the time that stage was the
only one open, an *upper bound* on what deleting it could save and not the
saving. Mean occupancy is how many stages overlapped, not a speed-up and not
busy cores. A stage's RSS delta is the whole process across its window, so the
deltas do not add up and none is that stage's ownership. Match audio is split by
the decoder each clip took, in worker time: one stage row cannot tell a cheap
byte loop from an external `ffmpeg`. Plus the frames drawn behind the loading
screen and a settled screenshot under `screenshots/bench/<zone>.png`; eyeball it
for geometry that never arrived.

**[2/3] Frame time** — every typed span from [`PERF.md`](PERF.md), as a tree,
for the frames *after* the first playable one. `avg` is one call and `per frame`
is what one frame paid for all of them; only the second belongs next to a frame
budget. `self` is a span's total minus the spans nested inside it — on a thread
that waits, that can be the wait. The ranking below the tree is by `self` and
includes parents: a parent with cheap children and an expensive body is in no
leaf. A span's parent is whichever span was open when it began; `*` means more
than one was seen. `wall` is the frame clock and encloses nothing.

**[3/3] Counters** — the render stages (`submit_prepare/gather/arena/record`,
`graph_render/submit/present`), the GPU passes, and the per-frame work census
(draws, batches, multi-draw, bind groups). A counter nobody sampled prints MISS,
never `0.00`. GPU passes overlap and resolve some frames late, so they are never
summed; `gpu_frame` is the measured interval. Counters this build declares and
this run never sampled are named at the end.

Percentiles come from a log-scale histogram, 16 buckets per octave: exact to
±3.1% of their own value. Counts, sums, minima and maxima are exact.

The run's own directory holds `report.txt`, `manifest.json` (the revision,
toolchain, adapter, window, pools, env toggles and cache state — absent facts
are `null`, never a plausible default) and `summary.json` (the same spans,
counters and audio totals as data), beside the `trace.pftrace` the run recorded.
