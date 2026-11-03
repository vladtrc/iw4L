# Content generation

`utils/content_gen` owns scene authoring and shot search. `utils/content_tool`
provides `iw4l-content`. The game runtime does not depend on either package.

```sh
cargo build --profile play -p content_tool
set -a; . ./.env; set +a
 target/play/iw4l-content inspect-doors iw4:mp_terminal
 target/play/iw4l-content inspect-projectile-openings iw5:mp_seatown
 target/play/iw4l-content connect-maps iw4:mp_terminal t5:mp_nuked
 target/play/iw4l-content solve-shot route.json
 target/play/iw4l-content validate iw4l-artifacts/maps/random-door.json
 target/play/iw4l-content migrate old.json migrated.json
```

The tool supports `--no-default-features` and individual `t5`, `t6`, `iw5`
features. Door catalogs live under `<artifacts>/content/doors/`. Scenes live
under `<artifacts>/maps/`; `iw4l map <scene-name>` loads a generated scene.
Shot trajectories live under `<artifacts>/content/shots/<name>/trajectory.json`.
Routes can require collision-free intervals around portals with
`min_portal_entry_clear_ms` and `min_portal_exit_clear_ms` (default: zero).
Solved `portal_entries` report conservative `clear_before_ms` / `clear_after_ms`.
`gravity_scale` and `speed_scale` tune debug projectile flight; both default to 1.
Scenes carry them as `projectile_gravity_scale` / `projectile_speed_scale` dvars.

Current authored scenes use `schema_version: 1`. Legacy projectile settings
require explicit migration; migration writes a new file and refuses to
replace an existing one. It preserves geometry and physical coefficients.
Measured aperture geometry must still match the runtime's supported version;
renaming settings cannot reconstruct missing geometry.

`sim::world_graph` owns runtime worlds and portals. `sim::projectile_flight`
implements the flight step shared by the solver, match and killcam.
A normal map has no extra worlds or portals. A scene may connect two locations
of its single world by assigning both endpoint world indices to zero.

`launch: {feet, owner_velocity, eye_height}` fixes a shot search to a measured
in-flight release pose. Such scenes require an input program; their `shot.cmds`
only prepares the victim. `max_flight_ms` bounds the total flight duration;
`max_bounces_per_world: [a,b,c]` caps ricochets in each world.

`inspect-projectile-openings` writes a separate catalog under
`<artifacts>/content/projectile-openings/`. Set `catalog: "projectile-openings"`
in a shot route to use it. These openings retain native visible frames and
collision checks, but do not require a walkable floor on the far side.
Endpoint `clearance: "flight"` validates a swept projectile corridor at load;
`clearance: "walk"` and omitted clearance require the normal player exit.
Both modes preserve surrounding collision and native visibility sidedness.

`assets::SceneRecipe` validates realized endpoints once for collision, rendering
and simulation. Assets approve the request and walk each world once, retaining
render products, collision and environment under its physical WorldId. The
session's original background load task then trims collision, validates exits
and builds sim/killcam atlas products before publishing PreparedMatchReady.
Installation moves that prepared atlas into live owners; it performs no atlas
load, trim, validation or conversion. Cancellation uses the request's progress
handle throughout preparation. Match map fingerprints include all worlds' collision geometry,
portal geometry, sidedness, flip, thickness, clearance, scene dvars and schema /
generator versions. Immutable geometry is hashed once when `SimContent` is
published.

Starting a random-door template selects its seed at the authority and freezes
`<template>-<seed16>-<template-digest16>.json`. That key is advertised to peers.
A client without the frozen file reconstructs it from the same installed
`<template>.json`, seed and generator; a different template or generator is
refused. Clients never select another seed while resolving a shared key.
Portable recordings carry the frozen recipe and do not need the template.
Template names must fit within the 64-byte shared map key after its two hex
fields are appended (at most 30 bytes for the template name).

Recording format 1 has a separate game-protocol field and stores the complete
64-byte map key plus its terminator. Recordings with the earlier header layout
are rejected by the format check; they are not reinterpreted as truncated scene
names. Scene recordings retain their frozen `.layout.json` sidecar.
