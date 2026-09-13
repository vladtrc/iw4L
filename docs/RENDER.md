# Render: what lives where

An island of nine crates: `render` (composition and diagnostics only),
`render_frontend`, `render_gpu`, `render_frame`,
`render_backend`, `render_material`, `render_anim`, `render_fx`,
`render_scene`. `render_frontend` reads the presented snapshot, `render_gpu`
writes the pixels. The island decides no game logic and never writes
`AuthorityWorld`. No asset decoding happens here.

## What lives where: `render_frontend` (three seams) + neighbours

* `prepare/` — scene/view/cull (`scene/world.rs`, `scene/spawn.rs`,
  `scene/cull.rs`, `scene/camera.rs`, `scene/gfx_scene.rs` anim-submission
  bridge, `worker_cmds.rs`);   occupancy Resources, lighting request/result, frustum planes, and camera
  pose — `render_scene`;
* `adapters/` — occupancy out of animation and FX (`adapters/anim/*`,
  `adapters/fx/*`);
* `assemble/` — the immutable products of a frame (`drawsurf/retained_list.rs`,
  `drawsurf/frame_products.rs`, `drawsurf/material_runtime.rs`,
  `drawsurf/tess/`);
* `render_anim` / `render_fx` — producers of poses and FX; `render_scene` —
  occupancy storage; `render_frame` — the immutable snapshot of frame products
  (`SourceRevisions`, `PackedSegments`, pure packing) at the frontend ↔ GPU border;
  `render_material` — administrator of the material tables; `render_backend` —
  the list views; `render_gpu` — the sole owner of `wgpu`;
  `render::diag/` — observation.

## The frame path

```text
zone → asset_iw4 IR → prepare/scene/world.rs      the world in our own types
                       → prepare/scene/spawn.rs      GPU spawn
     prepare/scene/camera.rs (look/WASD), frustum.rs + cull.rs + dpvs_iw4
     → assemble/drawsurf/list.rs      ONE sorted GfxDrawSurf[]
       → setup.rs   R_SetupMaterial — techType → technique
       → state.rs   R_ChangeState  — GfxPassState / blend
       → tess/{world,smodel,xmodel,fx,mark,glass,particle_cloud}.rs
       → material_runtime.rs  N-pass execution or a typed refusal
       → sm3*.rs    SM3 tokens (`d3d9_sm3`) → WGSL → gpu_contract.rs → wgpu
```

The key point: retail keeps **one** drawsurf machine, only the tess emitters
fork; the three "renderers" (world / smodel / viewmodel) are a lie, and it has
been physically burned out. Lighting: `lighting_iw4` (light grid) plus
`prepare/scene/smodel_lighting.rs` and the `model_lighting_atlas.rs` atlas;
shadows are `assemble/drawsurf/sun_shadow.rs`.

## GPU-side ownership

`render_gpu/src/drawsurf/colour_submit/` — one list, three owners:

* `geometry.rs` + `residency.rs` — stream residency. Allocation capacity, live
  length, allocation generation and the uploaded producer revision are four
  different quantities: a frame that shrank does not recreate the buffer, and a
  producer segment that changed (`render_frame::PackedSegments`) is uploaded
  alone rather than the whole stream;
* `prepare_camera.rs` — shared installation resets generations and publishes
  shadow views, then sun, spot and camera prepare in order. Executors and arenas
  are pass-local; binding slots and the shadow texture table still share mutable
  ownership. The camera's pretess cache is `CameraWorldPretess`; shadows do not
  hold it. There is exactly one camera: a second `Camera3d` is the typed
  refusal `MultipleCameraViews`;
* `record.rs` + `indirect.rs` — recording. The direct path is `draw_indexed`;
  with `IW4L_MULTI_DRAW` plus `INDIRECT_FIRST_INSTANCE` and
  `DownlevelFlags::INDIRECT_EXECUTION`, adjacent compatible runs of the same
  plan are written as a single `multi_draw_indexed_indirect` (the `multi_draw` /
  `multi_draw_cmds` counters). Order and per-command `first_instance` do not
  change.

The `render/src/extract.rs` bridge publishes `InstalledRenderWorld` when static
geometry, ports or SMC maps change, and `PublishedRenderFrame` every frame,
checking frame/world/material identity.
CPU image handles share the `assets::image_handles` Arc with the upload registry.
The loader's decisions are made in
`render_frontend/src/prepare/scene/world_gpu.rs::consume_gpu_load_progress`.

## Neighbours

`d3d9_decl` / `d3d9_sm3` / `d3d9_state` carry D3D9 semantics without a single
word about IW4; the only D3D9 → `wgpu` crossing is the adapter inside `render`.
`session` stands the match up and tears it down, `assets` owns the bytes.

## How to poke it

The retail `r_*` names are not wired up as live dvars yet. Live, it is env
vars (`IW4L_SINGLE_CELL`, `IW4L_SUN_SHADOW_*`, …) and native Perfetto
(`IW4L_PERF=1`, [`PERF.md`](PERF.md)).
