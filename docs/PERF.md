# Native Perfetto

Runtime observation is `iw4l-artifacts/runs/<uuid>/trace.pftrace`. Recording
is off unless `IW4L_PERF=1` (`make bench` / `make bench-live` / `make scenario` /
`make chaos` / `make lifecycle-*` force it on). Where the *run* spent its time,
rather than one frame, is [`BENCH.md`](BENCH.md).

```bash
make bench-live                            # record + compact p50/p90/p95/p99/hot-path report
make bench-overhead                        # 10 alternating on/off pairs + paired 95% CI
make bench-overhead VAR=IW4L_BENCH         # the same, for load observations
make perf-compare fc3d8eaf acc57a5c        # build both commits, alternating heavy runs, report per hash
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

With `IW4L_PERF=1`, GSC diagnostics also appear directly in the trace:
`gsc_native_coverage` carries map, bound/unavailable counts and unavailable names;
`gsc_runtime_error` carries source site, function and fault for the first reported
occurrence of each site/fault pair. `gsc_terminal_fault` records the authority
refusal, and `gsc_install_refusal` records map, preparation stage and fault.
Query the entire trace as well as the marked gameplay window: compilation and
installation happen before the start mark. Missing coverage events are missing
evidence, not proof of zero gaps. Runtime diagnostics use the same duplicate
suppression as the runtime log; their count is not the number of native calls.
`gsc_platform_hint` records execution of the desktop stream-origin advisory
methods with `native` and `policy` fields. It is separate from runtime errors
and coverage; an eager-residency hint is not evidence of console streaming support.

`script_collision_checks` and `script_collision_reused` are cumulative settlement
request/reuse counts for the current script state (checkpoint restores rewind
them). They are sampled after the initial settlement in presence sync; later
queries in that tick appear in the next sample. Reuse requires exact source inputs and an unchanged collision
mutation revision, after actor animation-tree publication. When the revision and
source topology remain stable, a changed source updates only its collision row
and pending model operations. The existing presentation predicate still decides
whether to reapply a model, preserving transient DObj changes during small pose
updates. Turret controllers are refreshed at each settlement that changes state.
External collision mutations, topology changes and aliased presences use the
complete rebuild. `script_settle_incremental_collect` measures collection of the
changed inputs; `script_presence_collect` still includes full rebuilds and
network presentation. These spans do not imply that a simulation step was skipped.

`model_lighting_cadence` records reduced-frequency cache allocation decisions:
`render_frame`, `period`, `max_distance`, `reused` and `assigned`. Counts reset
per render frame and can be emitted after both lighting drains; take the maximum
per `render_frame` before summing. An assignment requests sampling but does not
prove that a light grid was available or that a tile reached the GPU. Missing
events mean no reduced-frequency allocation decisions were observed.

`script_entity_capacity_refused` reports the script entity class, live dynamic
count, limit and next logical number when that owner refuses a creation.
`gentity_capacity_refused` reports the gameplay slot high-water mark, limit and
free-slot count. These distinguish owners that can otherwise return the same
"no free entities" error. A failed script creation must not leave a gameplay
mover allocated, and a failed gameplay allocation must retire the unpublished
script entity.

`campaign_hud_controls` records the campaign stance/mask draw request after
visibility checks: `show_stance`, `mask`, `stance_alpha`, `requested`,
`missing_images` and `layout_gaps`. Missing layouts and images are refused.
These counts describe HUD publication; they do not prove pixels reached the GPU.

For short GSC diagnostics, `IW4L_PERF_GSC=1` with `IW4L_PERF=1` records
`gsc_native_cost` and `gsc_execution_cost` events. Group their `debug.cost_us`
by `debug.native`, or by `debug.module` and `debug.function`. Execution costs
include synchronous callbacks and native costs can include nested script work;
these totals overlap. This adds timing and event overhead to every native call,
so use it to locate a hotspot, then compare runs with the detailed switch off.
Long recordings can exhaust the trace buffer. The normal simulation phase
scopes and `likely_path_research` events do not require this switch.

Script-model presentation has body scopes `present_script_model_pose` and
`present_script_model_commit`. Their pose work is split into `present_pose_xanim`,
`present_pose_matrices`, `present_pose_vertices` and `present_pose_meshes`.
The vertex/mesh helpers can also be called by other model producers: use their
parent scopes when attributing totals. `script_model_pose_refused` carries the
base model and pose error for each refused pose; repeated frames can report the
same failure, so group by model/reason. Schedule-interval HUD counters include
interleaved systems and are not measurements of the HUD body itself.

`skin_dispatch_refused` records the first failed GPU skin dispatch per match,
with reason, job count, output vertex count, resident bytes and budget. The same
refusal appears in the runtime log before rendering readiness fails. This helps
distinguish invalid geometry ranges/palettes from residency limits.

`prediction_errors.sql` counts validated fields against the acked authoritative
state. Distance is absolute scalar error, the largest vector component error,
or the number of differing array elements. Nonfinite values remain explicit;
the percentile uses finite samples only. Reconciliation tolerances are unchanged.

`client_collision_prepare` measures rebuilding a client's snapshot-owned collision
projection. `client_collision_projection` reports `linked_brushes`,
`model_brushes`, `model_planes` and `body_volumes` after construction/adoption.
Queries borrow this prepared geometry until the next snapshot. These events
describe preparation and retained geometry, not collision hit correctness.
