# Native Perfetto

Runtime observation is `iw4l-artifacts/runs/<uuid>/trace.pftrace`. Recording
is off unless `IW4L_PERF=1` (`make bench` / `make bench-live` / `make scenario` /
`make chaos` / `make lifecycle-*` force it on). Where the *run* spent its time,
rather than one frame, is [`BENCH.md`](BENCH.md).

```bash
make bench-live                            # record + compact p50/p90/p95/p99/hot-path report
make bench-overhead                        # 10 alternating on/off pairs + paired 95% CI
cargo xtask perf-overhead --pairs 10 --bin target/play/iw4l --zone mp_highrise --cmds 'wait world; wait 8s; quit' --focus script_model:192
make scenario                              # scripted match: IW4L_PERF=1, then cargo xtask scenario
make chaos                                 # RPG splash run: IW4L_PERF=1, then cargo xtask chaos
make lifecycle-swap                        # occupancy: IW4L_PERF=1, then cargo xtask live swap
make lifecycle-all                         # run all five occupancy recipes and read them back
cargo xtask bench                          # rerun the report on the newest run
cargo xtask bench iw4l-artifacts/runs/<uuid>
cargo xtask scenario                       # scripted-match report on newest .pftrace
cargo xtask chaos                          # RPG-splash report on newest .pftrace
cargo xtask net-feel                       # input-to-photon report on newest .pftrace
cargo xtask live swap                      # lifecycle occupancy on newest .pftrace
cargo xtask query slices.sql
cargo xtask query <uuid> render.sql         # render topic pack
IW4L_PERF_FOCUS=script_model:192 make map mp_highrise CMDS='wait world; wait 5s; quit'
cargo xtask query <uuid> render_owner.sql   # selected owner plan → DrawIndexed
cargo xtask query "SELECT name, COUNT(*) FROM slice GROUP BY name"
```

Each run has `trace.pftrace` + `manifest.json`. Optional reports are
**offline** from Trace Processor (`xtask/perfetto/*.sql`). The runner fetches
https://get.perfetto.dev/trace_processor if `TRACE_PROCESSOR` / PATH are empty.
The in-process ring is 32 MiB. Every reader rejects Perfetto errors, drops, and
overwrites; buffer exhaustion fails the read rather than reporting a partial trace.

Pin the run UUID and verify the manifest printed by every reader: zone, role,
workload, and git must match the claim. The implicit newest run is a convenience,
not a citation. A state-only question may use console `dump [name]`,
which writes one `iw4l-artifacts/dumps/*.txt`; it contains no history or timing.

Per-item overlay/draw spans are forbidden. `IW4L_PERF_FOCUS` admits one owner;
without it there is no owner census. Semantic events stay rare (`player_tick`,
death, feel, …).
