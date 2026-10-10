# Weapon configuration products

`asset_game::WeaponCatalog` captures source rows. `WeaponBuild` links dependencies and publishes immutable effective rows, semantic policies and consumer projections.
`weapon_catalog/{capture_merge,catalog_linking,model_linking,publication}` own
those operations; `configuration/{iw4,iw5,t5,t6}` own source selection rules.

`WeaponFamilies` normalizes selections and checks attachments and host limits.
Frontend loadout publication defers map-selected hands admission; matches validate soldiers and skeletons. Saved class previews include unavailable captured families.
Resolution and UI toggles return a private registry-issued `WeaponHandle` plus read-only canonical selection. Console/UI use this resolver. Publication iteration
includes the last valid row; unarmed, unknown and unsupported remain distinct.

`WeaponRegistry::bind` rejects foreign revisions before row access. Exact clones
retain identity; republishing changes it. Raw projection getters are crate-private;
consumers use `BoundWeapon`. `PreparedWeapons` admits wire IDs only after checking
the installed snapshot epoch or original event generation. Local revisions never
enter wire IDs/content digests. Queued effects retain their producing generation.

Combat, equipment, penetration, FPV, HUD, world and event projections use one
effective row. Combat binds host rules and location damage before simulation.
T6 burst cooldown uses its authored delay; other sources use the host rule. Presentation facts carry
camera, alternate, dual, shield, overlay and event policies without reinterpreting
capture classifications. Loadout labels/archive hints are prepared by asset_game.

Underbarrels retain their native alternate inventory identity, reload stages and
switch clips. An absent empty-reload clip selects the ordinary reload with its
authored timing through an explicit reload policy; captured timing remains intact. Independent T6 underbarrels keep their own reload clips when
the rifle has fast magazines. Effective IW5 attachment reticles resolve their
material images after composition; T6 reticle materials enter the native HUD
image set with their authored sizes and minimum spread offset.

`BoundWeapon::preparation` retains source keys and per-component targets/refusals.
T6 models, clips, cues and materials retain T6 source and storage identity.
Missing native dependencies refuse their dependent capabilities; source weapon
rows never inherit another family's row. Publication retains captured family.

The map selects the soldier kit and hands family. Weapon-authored hands are
eligible only for that family. FPV composition validates typed native model
pairs; T6 guns additionally admit an explicit `T6WithIw4Hands` connection for
IW4 soldiers. Catalog owners must match. Unsupported family pairs refuse.

`FpvWeaponTable::bind` checks registry, mesh, clip, material, image and atlas owners
before accepting handles and alternate links. Retained compositions validate model
and hide identity; track mappings/rigs retain actual clips and meshes. Foreign clips
or rig poses refuse before writes. Optional bones/clips and preparation budget remain.
Material admission retains mesh/material owners; raw assembly construction is internal.
Items and remote kits share `ItemComposition` topology/hide and registry/world owners. Required model/pose failures are cached;
optional attachments and camo-to-base fallback preserve policy. Replacement clears
successes/refusals. Kits retain body/world catalogs; `models()` accepts no catalogs.
Prepared DObj identity controls reuse; heads use checked soldier capabilities.
`PlayerAnimationBinding` retains the character kit/rig, paired tree/script and clips.
Multiplayer body tracks validate leaves/tracks and soldier tree/script/clip family.
Missing native profiles refuse; persistent trees reuse only the same binding.
Body/tree/script/clip replacement resets animation and preserves corpse occupation; unchanged bindings preserve blend, phase and rate.

Publication compiles appearance plans once; `SelectedWeaponAppearance` binds models, overrides and UI metadata.
Raw hand/camouflage data stays on `WeaponBuild`; runtime consumers select appearances.
The editor reopens attachments/camouflage; previews retain namespace material/image bindings and fall back to authored HUD icons. IW4 scripts bridge foreign smoke, gas, sticky, knife and C4 equipment names.
