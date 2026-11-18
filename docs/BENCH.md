# Benchmark reports

`make bench` records a native Perfetto trace and a manifest. Analysis runs
outside the game through Trace Processor. Benchmarks keep running when
unfocused; `IW4L_BENCH=1` also enables load-job observations and a settled
screenshot. `IW4L_PERF=1` records the trace.

```bash
make bench demo0011
make bench ZONE=mp_boneyard
IW4L_BENCH_JOBS=assets make bench demo0011
cargo xtask bench iw4l-artifacts/runs/<uuid>
cargo xtask query <uuid> bench_spans.sql
cargo xtask query <uuid> bench_counters.sql
cargo xtask query <uuid> bench_frames.sql
cargo xtask query <uuid> bench_waterfall.sql
cargo xtask query <uuid> bench_load.sql
cargo xtask query <uuid> bench_jobs.sql
cargo xtask query <uuid> bench_audio.sql
make perf-compare <base> <candidate>
```

Span and counter tables include the complete recording and separate
`*.spawned` windows bounded by the next benchmark mark. They report exact
percentiles from native samples. Missing measurements remain absent.

The frame table intersects each wall interval with the union of the main
schedule and render intervals. `unclassified_ns` is wall minus that union;
concurrent work is counted once. GPU counters describe when results were
delivered; passes overlap and must not be summed.

Load stages and image jobs are raw observations captured before trace flush.
`sole_open_ms` measures when one stage was the only active stage. It does not
predict the time saved by removing that stage. Queue and decode waits stay
separate. Audio fields retain the decoder and process-memory measurements.

Each run keeps `manifest.json` and `trace.pftrace`. The manifest describes the
binary, workload, GPU, window, task pools and caches. Pin the run UUID when
reporting a measurement. `perf-compare` builds each commit and compares
alternating gameplay windows from the approved lifecycle scenario.
