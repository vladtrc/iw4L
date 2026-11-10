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

World 0 supplies scripts and match flow. Every world retains an explicit
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
