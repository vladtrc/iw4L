# Map load: what lives where

Three floors: **disk** (`asset_transport`), **format** (`fastfile_*` → the
`asset_iw4` IR), **products** (`asset_*` + `assets`). `assets` knows the disk
through the transport and nothing of the format; `fastfile_*` knows the format
and opens no files. Installing the match is `session`, not here.

```text
console/ui `map` → session::lifecycle   SessionSwapRequest, tear the previous
                     occupancy down, then MapLoadApproved{request_id}
  → assets::match_load    approve_map_load / start_match_load onto load_pool()
                          (MatchLoadBusy) / poll_match_load → PreparedMatchReady
  → session::match_apply  apply_prepared_match → scene + SimWorld + MatchInstalled
```

Cancellation is `MatchLoadAbort(request_id)`: the walk returns
`MatchLoadOutcome::Canceled` at its next checkpoint, with no prepared match to
throw away. Failure is `MapLoadFailed`, from discovery *and* from install, and
every accepted request ends once — `MatchInstalled` or `MapLoadFailed`.

## The walk: `assets::session_load::load_prepared_match`

The single entry point; the composition root sees no `fastfile_*` and no
`AssetSink`. Inside are parallel walks on `load_pool()`: the archive, startup
materials, `common_mp`, the IW5/T5 weapon bundle, language zones. `common_mp`
goes **before** the map — its techset tables are needed before
`build_world_draw`. Output is `MatchLoadOutcome::Ready` with a `PreparedMatch`
(world, one `MatchMaterials` population, clip, weapons, catalogs and
`PreparedMap`, the single owner of spawns and map facts).

The per-game adapter is `assets::lane` (`ZoneGame` → iw4/iw5/t5), the only
exhaustive `match ZoneGame` in the project; the runtime stays game-blind and a
gap in a lane is a typed `LaneGap`, not silence.

Installing it is `session::match_apply`, in two halves. Preflight builds a
`MatchInstallPlan` — mode, doors, objectives, scene conversion, drawable world —
and publishes nothing until it hands one over, so a refusal leaves no
half-installed resources. Commit publishes it, boots the sim, writes
`MatchInstalled`.

## Cache, and poking it: `iw4l-artifacts/cache/<kind>/<key>`, content-addressed

Leaf in `asset_transport::artifact_cache` (`cache_get` / `cache_put`,
`fnv1a64`). A miss is silent — the caller computes the value anyway — and a hit
must be the **same bytes** a miss would have written. The key names every input;
if the encoder changed, bump the format word. Live kinds: `mips`, `wgsl`,
`localize` and `xwma_pcm`, whose miss is a batched external `ffmpeg`.
`IW4L_GAMES` is the root holding the game trees; no folder name is hardcoded and
the container version picks the decoder. Live it is `make map mp_boneyard`
([`RUN.md`](RUN.md)), with stages in `LoadProgress` and the `.pftrace`.
