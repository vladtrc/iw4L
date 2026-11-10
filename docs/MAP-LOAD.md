# Map load: what lives where

Three floors: **disk** (`asset_transport`), **format** (`fastfile_*` → the
`asset_iw4` IR), **products** (`asset_*` + `assets`). `assets` uses transport for disk access and its source lanes to compile
format captures into products; `fastfile_*` knows the format and opens no files. Installing the match is `session`, not here.

```text
console/ui `map` → session::lifecycle   SessionSwapRequest, tear the previous
                     occupancy down, then MapLoadApproved{request_id}
  → assets::match_load    start_match_load onto load_pool() (MatchLoadBusy),
                          poll_match_load → PreparedMatchReady
  → session::match_apply  prepare install → Staging → OnEnter(Live) → scene + SimWorld
```

Cancellation is `MatchLoadAbort(request_id)`: the walk returns
`MatchLoadOutcome::Canceled` at its next checkpoint, throwing nothing away.
Failure is `MapLoadFailed`, from discovery *and* install; a request ends once.
Startup and later load failures return to the main menu with a map error dialog;
OK or Escape dismisses it. The log keeps the full error.

The main world has three match states: `Absent`, `Loading`, and `Live`.
`ScopeControl` resolves requests once in `Last`, with teardown taking priority over
install and install over begin-load. Forced transitions also retire a replaced
`Loading` scope. Load tasks and sound-bank composition belong to `Loading`;
prepared catalogs, simulation, animation, scene plans and replay buffers belong
to `Live`. Fallible preparation completes before requesting `Live`.

`OnEnter` installs resources in registration order, derives publications, then
announces the installed identity. `WorldGeneration` is a nonzero scoped resource;
`WorldStamp` is its copied value in diagnostics and render frames. On exit,
release systems and entity removal hooks run while resources still exist. The
transition schedule then removes registered resources in reverse order and hands
them to `Retiring`. Match systems use `InMatch` on their main/fixed schedules;
render extraction reads optional main-world resources and clears missing frames.

`RoundPhase` exists only under `Live`. A changed server round serial forces a
fresh `Playing` scope even when the phase stays `Playing`. Simulation separately
owns `MatchScript` and `RoundScript`; only `RoundScript::new(&MatchScript)` creates
round state. Frontend menu locals and menu stacks have app owners separate from
their scoped in-match equivalents.

## The walk: `assets::session_load::load_prepared_match`

The single entry point; the composition root sees no `fastfile_*` and no
`AssetSink`. Inside are parallel walks on `load_pool()`: the archive, startup
materials, the profile's common zone, installed-family weapon bundles, language zones.
The IW4 map's ClipMap/GameWorld asset types select the runtime profile before
common preparation. Campaign uses `common` and SP startup/sound/localization
sources, without multiplayer donor bundles; other maps keep the multiplayer
profile. Mixed SP/MP world metadata is refused. The common cache key includes
this profile. The common walk goes **before** the map — its techset tables are needed before
`build_world_draw`. Output is `MatchLoadOutcome::Ready` with a `PreparedMatch`
(world, one material population, clip, weapons, catalogs and `PreparedMap`).

A walk enqueues its own image plan the moment it is ready, and the catalog keeps
one row per image name. Two plans resolving the *same* archive entry into the
same payload share one decode; a name three games spell alike but fill
differently is an override, not a duplicate, and `load_jobs.csv`
([`BENCH.md`](BENCH.md)) counts the two apart. The per-game adapter is
`assets::ZoneLanes` (installed `FamilyId` → family-owned lane); a lane gap is a typed `LaneGap`, never
silence.

`ZoneLane::load_common_mp` returns owned common builders and diagnostics. A lane whose preparation needs already assembled donors can retain a private, consumed-once `CommonFamilyCompiler` inside that result. The shared loader supplies owned weapon, material, FPV, world-model, projectile, animation and effect builders; compilation returns them together with dependency refusals. This stage finishes before the common set is published or cloned for a match. It is not invoked by frame execution.

T6 model/material/effect captures stay private to the T6 lane. Its compiler owns stand-in donor lookup, native technique linking, named texture adaptations, animation admission, weapon dressing and projectile binding. Missing model/effect donors return typed refusals, while accepted contributions and existing builders remain available; source conversion diagnostics remain in the load report. Other lanes finish these products within their existing common walk. All lanes publish through the same common-set and match installation path.

`ZoneLane::load_world` borrows the common `FilmVisionCatalog` immutably. The map's vision overrides a common vision, including a map parse error; a failed map parse does not silently fall back. Selection and the merged vision catalog belong to `LoadedWorld` output. Loading a map does not consume a caller's common vision entries.

These are CPU compilation contracts. They do not establish shader-port admission, decoded-media readiness, GPU residency or playable readiness. The existing common cache owns compiled builders under its path-based `CommonKey`; a retained key does not detect edits to source files in place. Such content needs a rebuilt common set. No per-family decoder payload escapes the deferred compiler, and changing source content must not reuse its captured payload with unrelated donor builders.

Installing it is `session::match_apply`. Preflight builds a `MatchInstallPlan` — mode, doors, objectives, scene conversion, drawable world —
and publishes nothing until it hands one over. Commit publishes it, boots the
sim and writes `MatchInstalled`. The authority then prepares bot navigation on
`load_pool()` from a world snapshot; it reports `BotNavigationReady` with the
world generation. Session owns readiness policy: dedicated advancement needs
navigation; listen advancement also needs rendering unless headless. Graphical
presentation and admission require generation-matched rendering and audio;
headless policy bypasses those presentation services.
Explicit silent audio satisfies that policy; missing or failed audio does not.
Old completion reports cannot release a newer world, and renderer systems do
not write the authority hold. Session also owns the local input predicate.
The walk graph is cached (`nav`), keyed by the content digest with the bake's
schema and hull, so the second start of a map reads it back instead of walking
the grid again; a match teardown drops the in-memory copy, not the file.

`MatchLoadOutcome::Ready` is the CPU package, not render-ready. The loading
screen also holds for GPU images, every queued pipeline (the HUD blood film is
queued as soon as its material exists) and the `first_person` stage: every
weapon's first-person materials admitted and compositions laid out
(`render_anim::PreparedFpv`), and every body, world-weapon, map-model and
projectile material plus the single-model DObjs (`PreparedModelMaterials`).
Remote kits and dropped-item compositions are built at install.
`load ledger:` in the log is one line per load of
what was handed over or compiled new against what an earlier load left behind.

## Cache, and poking it: `iw4l-artifacts/cache/<kind>/<prefix>/<key>`

Content-addressed leaf in `asset_transport::artifact_cache` (`cache_get` /
`cache_put`, `fnv1a64`). A miss is silent — the caller computes the value anyway
— and a hit must be the **same bytes** a miss would have written. The key names
every input; if the encoder changed, bump the format word. Live kinds: `mips`,
`wgsl`, `localize`, `nav` and `xwma_pcm`, whose miss is decoded by the native Rust T5 WMA2 decoder.
`IW4L_GAMES` holds the game trees; no folder name is hardcoded. Live it is `make
map mp_boneyard` ([`RUN.md`](RUN.md)), with stages in `LoadProgress`.
