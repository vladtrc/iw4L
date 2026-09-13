# Animation: what lives where

Three floors, deliberately different: the **retail fact** (pure mathematics),
the **runtime tree** (pose state) and **who picked which clip** (the game).

## `anim_iw4` — facts only, `#![no_std]`, no clock and no allocator

* `XAnimParts` — curve storage, unpacking of `half_quat` / `full_quat` and
  quantized translations (`XAnimCalcParts`); translations are **deltas**;
* span/sample/`time_to_frame` — a pure function of (clip, time);
* the blend rule `bind + scale * Δ`, rotation replaced (`DObjCompose`);
* `DObj` — how model, skeleton and attachments compose (0x9c, prefix mapped out);
* `XBoneInfo` — the OBBs for bone hits (`DObjGeom_RayBones`);
* `PartBits` — the six-word requested/ignore mask (`DObjCalcAnimInternal`);
* `PlayerAnimValue` — 10-bit `legsAnim` / `torsoAnim`, a 9-bit index and the
  `0x200` "restart" bit: a restart changes **only** that bit, not the index (A4);
* the leaf time step and the goal-weight ramp are measured IW4 constants (A5);
* script name tables: `ANIM_MT_NAMES`, `ANIM_BODY_PART_NAMES`, `ANIM_COND_NAMES`.

No `Vec`, no `AnimationClip`, no glam matrices and no playback clock here.

## `xmodel_runtime` — a runtime without Bevy and without ECS

A pure evaluator, used by **both** the authority **and** the client:

* `XAnimTreeDefinition` (the immutable hierarchy) + `XAnimTreeRuntime` (state;
  leaf time normalized into `[0,1]`, `update` is the per-leaf advance);
* `DObjAnimRuntime` — `update` / `pose_request` / `compose`; DObj reuse is
  equality of `(eType, model*)`, not "always a fresh one";
* `RetainedModelCapability` — pose, `XBoneInfo`, collSurf/collTris for hitscan;
* `build_body_head_weapon_dobj` — body + head on `j_spine4` + weapon on its tag.

A missing clip **never** turns into the bind pose; additive nodes are a typed
gap, not "ordinary blending".

## Who picks the clip

* `sim` — `player_anim_script.rs`, `mantle_xanim.rs`, `anim_script_gap.rs`:
  what animation the player is in, and it travels into the snapshot as
  semantics, not as a tree (S11);
* `assets` — `XAnimParts` → owning clips;
* `render_frontend/adapters/anim/` + `render_anim` — the viewmodel (`fpv*.rs`,
  `viewmodel_controller.rs`), third person (`third_person.rs`, `remote_body.rs`),
  kick and sway (`view_kick.rs`, `view_sway.rs`), items and projectiles.
  FPV actions come from the presented retail `weapAnim`, **never** from the
  keyboard; the identity of the FPV mesh is `BG_GetViewmodelWeaponIndex`.

To watch it live: `IW4L_PERF=1` / [`PERF.md`](PERF.md).
