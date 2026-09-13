# Map load: what lives where

Three floors: **disk** (`asset_transport`), **format** (`fastfile_*` → the
`asset_iw4` IR), **products** (`asset_*` + `assets`). `assets` knows the disk
through the transport and knows nothing about the format; `fastfile_*` knows
the format and opens no files. Installing the match is not here — it is in
`session`.

## The transaction: who calls whom

```text
console/ui `map`  → session::lifecycle    SessionSwapRequest → tear down the previous
                                          occupancy, then MapLoadApproved{request_id}
  → assets::match_load  approve_map_load  find_zone_file / find_runtime_common_mp
                        start_match_load  spawn onto load_pool()  (MatchLoadBusy)
                        poll_match_load   → PreparedMatchReady
  → session::match_apply apply_prepared_match → scene + SimWorld + MatchInstalled
```

Cancellation is `MatchLoadAbort(request_id)`: the poll throws the result away
and apply refuses to install. Failure is `MapLoadFailed`; `LoadingScreen` holds
the failed install until teardown and cannot turn it into a success.

## The walk itself: `assets::session_load::load_prepared_match`

The single entry point; the composition root sees neither `fastfile_*` nor
`AssetSink`. Inside are several parallel walks on `load_pool()`: opening the
archive, startup materials, `common_mp`, the IW5/T5 weapon bundle, language
zones. `common_mp` goes **before** the map: its techset tables are needed
before `build_world_draw`. The output is a `PreparedMatch` (world, clip,
weapons, catalogs, `PreparedMap` — the single source of spawns, `report`).

The per-game adapter is `assets::lane` (`ZoneGame` → iw4/iw5/t5), the only
exhaustive `match ZoneGame` in the project; the runtime stays game-blind. A gap
in the lane is a typed `LaneGap`, not silence.

## Cache

`iw4l-artifacts/cache/<kind>/<key>`, content-addressed, leaf in
`asset_transport::artifact_cache` (`cache_get` / `cache_put`, `fnv1a64`). A
miss is silent: the caller computes the value anyway, and a hit is obliged to
be the **same bytes** a miss would have written (A2). The key names every
input; if the encoder changed, bump the format word at the caller. Live kinds:
`mips` (`asset_material/material_images.rs`), `wgsl`
(`render_frontend/.../wgsl_disk_cache.rs`).

## How to poke it

`IW4L_GAMES` is the root holding the game trees; no folder name is hardcoded
anywhere (T1); the container version picks the decoder, not the folder name
(T2). Live, it is `make map mp_boneyard` ([`RUN.md`](RUN.md)); the stages of the
walk are visible in `LoadProgress` and in the `.pftrace` ([`PERF.md`](PERF.md)).
