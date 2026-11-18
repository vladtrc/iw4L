# Scene and simulation contracts

The architecture has three layers. The engine owns general mechanics and their
contracts. Repository utilities build, compile, validate and run arbitrary scene
data. A concrete scene owns map choices, positions, movement, inputs, bindings,
choreography and outcome checks outside the published repository, in the local
artifact or a caller-owned project. Scene-specific Rust code is still scene
content; implementing it as a tools module does not make it general infrastructure.

A composed map is data: a `SceneRecipe` Rust value describes worlds, regions,
placements, portal endpoints, bounded collision edits and scene dvars. JSON is
the recording/CLI transport; external Rust can pass a typed recipe directly.
YAML and Rust authoring must produce the same validated scene. Map names, filenames, actor names and weapon choices must never
select alternative physics or rendering code. Engine capabilities belong to the
engine; a recipe selects and configures them.

`MergedMap` owns its parsed scene and frozen source together. Its scene is
read-only. Change an authored recipe before parsing it; do not patch a loaded
scene while retaining an older recording layout. Validate at parsing and
installation. Invalid worlds, dimensions, placements and unsupported features
refuse the load before publication. Programmatic values obey the same rules.
Source-specific archive layouts belong in asset adapters; scene transforms and
collision policy depend on capabilities and `SceneSpace`, never a map key.

World 0 normally supplies scripts and match flow. An optional `multiplayer`
object separates IW4 world geometry from multiplayer content. Its `source`
names an installed IW4 MP zone; `gametype` currently accepts `dm` (FFA), and
`spawns` supplies 1..256 objects with finite `origin` and `angles` vectors in
the root world's coordinates. The recipe selects FFA regardless of the host's
previous mode. The MP source supplies player kits, weapons and animations;
the root supplies geometry, collision and lighting. A generated MP startup
uses the recipe's spawn entities. Campaign scripts, actors, triggers and path
nodes do not enter the match. Native SP maps keep their campaign runtime.
Large campaign worlds should use `space: "shared"` with an identity root
placement rather than the bounded portal atlas coordinate domain.

Optional `characters` entries bind a client to an explicitly captured donor
`source`, `body` and optional `head`. IW4 and T5 donor zones contribute selected
geometry and materials; their scripts, collision and animation trees are not
installed. Bodies normally use the multiplayer animation reference.
A `native_loop` selects a repeating original clip on the donor's native rig;
`native_loop_source` optionally names another installed donor zone containing it.
`native_loop_reference` may name another captured native body when a clip
uses a different rest rig; retargeting preserves the displayed mesh and bone IDs.
`native_actions` adds authored full-body actions over that idle. Each action has
`clip`, optional donor `source`, `start_ms`, `duration_ms`, and `blend_ms`.
Times use the actor command clock, retained by recording and replay. Actions
within one actor must be ordered and disjoint; different actors run independently
and may overlap. Blend intervals ease the action into and out of the continuing
idle. Render and collision sample the same deterministic track.

These performances use the same full-body track on the presentation and
collision clocks; player locomotion and aim controllers do not alter the pose.
Missing clips or clips without matching body tracks refuse preparation.
T5 donors also read their installed localized companion archive for material
definitions; named material references bind to real definitions in that family.
An optional `world` index scopes a binding to that simulation world. Client/world
pairs must be unique; a world-specific binding takes priority over the client's
unscoped binding, with faction kits as the fallback. Rendering and authority
collision select the same binding using the player's simulation origin, including
portal transitions. World indices are validated against the recipe.

Every world retains an explicit
`WorldId`. Native points, slab-tagged simulation points and scene points are
different domains. Convert points with translation, vectors without translation,
and normals with the appropriate rotation. An overlapping scene cannot derive
ownership from a scene point. Explicit world references must never be silently
clamped to another world. Legacy free-camera slab inference is a separate,
bounded presentation convenience and is not an actor ownership rule.

Movement, scripted movement, prediction, bullets, projectiles, pickup visibility
and navigation must query the same scene geometry. A caller chooses whether
its query is native or scene space. A trace returns contacts in its caller's
basis, preserves solid flags across all contributing worlds, and uses stable
ordering for equal distances. Portals preserve actor lifecycle and carry pose
and velocity through endpoint frames. Collision and presentation consume the
same installed recipe. Current shared placements require cardinal rotations
and an identity root; unsupported rotations fail validation.


Input producers: [INPUT-BOUNDARY.md](INPUT-BOUNDARY.md).
Headless design: [HEADLESS.md](HEADLESS.md).

A portal replacement may target a BSP door without a model leaf: omit its model
identity fields (`render_instance`, `model_name`, `model_origin`, `clip_models`).
`clip_brushes` carves the bounded leaf from selected brushes. Optional
`clip_triangles` disables exact collision triangles fully contained in that leaf;
triangles with separate partition borders refuse loading. Both kinds of edit
retain native IDs, and the surrounding geometry remains solid. A portal surface
can cover a native BSP door at its measured face while retaining its frame.
