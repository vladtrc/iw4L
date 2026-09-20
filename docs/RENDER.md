# Render: what lives where

Nine crates. `render_frontend` reads the presented snapshot, `render_gpu` writes
the pixels and solely owns `wgpu`. The island decides no game logic, never
writes `AuthorityWorld`, decodes no assets.

| crate | owns |
|---|---|
| `render_frontend` | `prepare/` scene, view, cull (`scene/{world,spawn,cull,camera}.rs`, `scene/gfx_scene.rs` anim-submission bridge, `worker_cmds.rs`); `adapters/` occupancy out of anim and FX; `assemble/` frame products (`drawsurf/{retained_list,frame_products,material_runtime}.rs`, `drawsurf/tess/`) |
| `render_scene` | occupancy storage: lighting request/result, frustum planes, camera pose. Static light lookup and BSP cells publish on `MatchInstalled` and clear on teardown; frame culling does not republish them. `render_anim` / `render_fx` produce the poses and FX |
| `render_frame` | the immutable frame-product snapshot at the frontend ↔ GPU border (`SourceRevisions`, `PackedSegments`, pure packing) |
| `render_material`, `render_backend` | the material tables; the list views |
| `render` | composition, `extract.rs`, `diag/` |

## The frame path

```text
zone → asset_iw4 IR → prepare/scene/world.rs (our own types) → spawn.rs (GPU)
     camera.rs (look/WASD), frustum.rs + cull.rs + dpvs_iw4
     → assemble/drawsurf/list.rs   ONE sorted GfxDrawSurf[]
       → setup.rs  techType → technique      → state.rs  GfxPassState / blend
       → tess/{world,smodel,xmodel,fx,mark,glass,particle_cloud}.rs
       → material_runtime.rs   N-pass execution or a typed refusal
       → sm3*.rs   SM3 tokens (`d3d9_sm3`) → WGSL → gpu_contract.rs → wgpu
```

**One** drawsurf machine; only the tess emitters fork per surface type. World,
static models and the viewmodel are three emitters into one list, not three
renderers. Lighting is `lighting_iw4` (light grid) with
`prepare/scene/smodel_lighting.rs` and `model_lighting_atlas.rs`, shadows
`assemble/drawsurf/sun_shadow.rs`.

## GPU-side ownership: `render_gpu/src/drawsurf/colour_submit/`, three owners

* `geometry.rs` + `residency.rs` — residency. Capacity, live length, allocation
  generation and uploaded revision are four quantities: a shrunk frame keeps its
  buffer and a changed `PackedSegments` segment uploads alone;
* `prepare_camera.rs` — shared installation resets generations and publishes
  shadow views; sun, spot and camera then prepare in order, on pass-local
  executors. A second `Camera3d` is the typed refusal `MultipleCameraViews`;
* `record.rs` + `indirect.rs` — recording. Direct path `draw_indexed`; with
  `IW4L_MULTI_DRAW` and the indirect downlevel flags, adjacent compatible runs
  merge into one `multi_draw_indexed_indirect`, order preserved.

`extract.rs` publishes `InstalledRenderWorld` when static geometry, ports or SMC
maps change, and `PublishedRenderFrame` every frame. `d3d9_decl` / `d3d9_sm3` /
`d3d9_state` carry D3D9 semantics with no word about IW4; the only D3D9 → `wgpu`
crossing is the adapter inside `render`, while `session` stands the match up and
`assets` owns the bytes. No live `r_*` dvars: env vars (`IW4L_SINGLE_CELL`,
`IW4L_SUN_SHADOW_*`, …) and Perfetto (`IW4L_PERF=1`, [`PERF.md`](PERF.md)).
