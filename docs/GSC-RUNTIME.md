# GSC → IR → Bevy

Rust language, IR and host integration live under `sim::script`.
Gameplay policy belongs to loaded GSC; Rust supplies language and engine primitives.
The scripts own the match flow,
player connect/spawn/damage/killed, the in-game menus, and the sounds they play.

## Sources and installation

Multiplayer matches support IW4 modes (dm, war, dom, sd, sab, ctf, koth, dd).
Installed map-script policies supply native entities and declarations and
select packaged or projected host-rule sources. Menu/HUD layout and match flow stay IW4, with weapon and team icons
from the corresponding game. Objective models, materials, shaders and effects
come from the map's game, including the plant/defuse weapon viewmodels and animations.
`ObjectiveVisuals` binds IW4 names to those assets at
install; missing bindings or foreign objective assets refuse the transaction.
Common scene models are selected from retained IW4 source strings, table cells,
map entities and objective bindings, without a per-model capture allowlist.

`Program::load` compiles a `SourceResolver`. `FileSources` reads explicit files.
IW4 asset walks retain GSC RawFiles from common_mp and the map; map files override
common files. `ScriptSources` also keeps every string table (code_post_gfx_mp and
patch_mp, then common_mp, then the map, later zones winning) and the map's entity
string. Sources travel through the common/resident caches into PreparedMatch.
Session preflight compiles the mode and map modules and builds `LevelData` (parsed
entities, tables). The install transaction installs the program, runs
`codescripts/struct::initstructs` synchronously (it must not wait), spawns the map
entities, sets the `mapname` and `g_gametype` dvars and schedules the `Iw4Startup`
entries before authority ticks: the gametype `main`, the map `main` (if present), then
`_callbacksetup`'s `codecallback_startgametype`. Missing dependencies, unsupported
syntax and unresolved names fail the transaction. Multiplayer also rejects unbound
natives before installation. Campaign logs the full missing-native inventory and
keeps unavailable slots in the program: executing one terminates the runtime with
its source location and script call stack. Uncalled declarations do not prevent
campaign startup; missing calls never return a fabricated script value.

Map entities: `worldspawn` is kept as level settings (`getnorthyaw`),
`script_struct` blocks become plain objects appended to `level.struct`, and every other
block, known classname or not, becomes a script entity with typed keys (`origin`,
`angles`, `angle` as yaw, integer `spawnflags`/`count`/`health`/`dmg`/`maxhealth`,
float `speed`/`radius`/`height`, strings otherwise). Every entity except a hud element
carries `classname`, `code_classname` (`script_vehicle` for any `script_vehicle*`) and a
zero `origin` and `angles` until something sets them. Entity numbers start at the first
non-client slot.
Names are normalized; traversal is rejected. The transaction announces its script
outcome on stdout for a controller: `gsc: installed …` with the program fingerprint
and module/function/native/entry counts once the world is published, `gsc: refused …
stage=compile|install|entry fault="…"` when the load stopped at the script.
`crates/approved_tests` records and asserts both.

`Catalog::iw4_campaign()` supplies the separate single-player builtin namespace.
IW4 maps with single-player world metadata select `common.ff`,
`code_post_gfx.ff`, `patch.ff` and optional localized companions. The common cache
key includes this profile; campaign content uses SP penetration data and excludes
MP donor catalogs, callbacks, rank tables and account preparation.

Campaign installation supplies local client0 through `LevelData`, finishes the
authored `info_player_start` points, creates the player and allied sentient, then
starts `maps/<mission>::main`. Local input waits for presentation admission.
Engine startup loads species animation hooks, authored actor types and traversal
scripts from path nodes, verifies required labels, and runs type precaches and
spawn hooks in entity context. Unimplemented natives remain explicit execution
faults when reached; compiling the mission does not establish complete support.

Actor slots, sentients, spawners, path nodes and animation controllers have typed
owners in the simulation. Actor and sentient allocation is bounded; failure
retires partially allocated ownership. Physical actors have collision bounds
independent of a visual model. Model, body, thermal and hero-lighting state reach
presentation through snapshots. Entity deletion retires these owners and their
references; round restart rebuilds them from retained level data.

Path-node native fields are read-only; script fields remain writable. Node objects
remain rooted across heap collection. ConnectPaths/DisconnectPaths maintain
per-brush blocker references, so deleting one blocker preserves other authored
and dynamic disconnections. The route planner, nearest-node selection and
moving-blocker refresh remain incomplete.

Actors own separate script and active goals, volume/entity/node references,
contact checks, stance constraints and typed orientation/physics controllers.
AnimScripted enters scripted state and runs the species hook; AnimCustom starts
its supplied script callback and retains the optional end callback. Replacement,
stop, death and natural completion retire the corresponding controllers.
Unlinked placement, body yaw, look angles, floor contact, sliding, stepping and
player push-away have consumers. Full AI transitions, navigation, perception,
linked placement, death planting and automatic base-state resume remain incomplete.
Suppression owns its meter, timed line entries and danger/overlap markings;
unimplemented nearest-node branches fail explicitly.

Campaign preflight retains ATR trees selected by IR animation references and
binds every retained leaf from the asset catalog, including startup clips.
Per-entity clocks drive DObj poses, collision and notetracks. Sync groups share
weighted normalized clocks; notifications subdivide a frame so script handlers
can replace or seek playback before its remaining time advances. Restart fades
retain old instances without changing script indices. Translation and planar/full
rotation delta channels are decoded with the clip. Delta queries and unlinked
scripted motion consume those channels; malformed channels fail decoding.

Configured keyboard/mouse controls publish `LocalKeyBindings` at startup, match
installation and remapping. GetCommandFromKey/GetKeyBinding use script key
identities and return at most two localized bindings. Command notifications use a
bounded registration table and reliable named input actions. Controller binding
queries and some locale-specific labels remain incomplete. Stance permissions
travel in player state and affect both authority and prediction.

SP audio supports ambient rates, master/slave ducking, room processing and
per-channel EQ. Reliable commands drive room priorities, sample-clock fades and
two tracks of three biquads. Entity EQ bypass identities travel in snapshots.
Tracked sound completion returns from the owning client to the VM. Room processing
uses IW4L's procedural stereo convolution. Waveform parity remains unverified.

Script-selected hands and map weapons are prepared from authored assets.
SetLookAtText publishes a per-viewer locational selection and localized HUD text.
Hero lights and character grid tweaks affect dynamic lighting; thermal state
selects actor materials when thermal vision is active. Full overlay, lighting,
attachment and animation visual parity remains unaccepted.

Trainer's range can progress through hip fire, crouched ADS and plywood
penetration to grenade pickup. Brush rendering uses live snapshot identity and
pose in camera admission and shader matrices. The full mission, Pit and all
assets/animations still require interactive acceptance; startup vehicle and
saved-dvar gaps remain.

Natives are linked against a `Catalog` of the game's builtins (generated into
`script/profile/iw4_catalog.rs`, split into function and method namespaces), not against the
registry. An unqualified call resolves to a function in the same file, then a builtin
in the call's namespace, then a unique include. Qualified names and `thread` calls
never resolve to builtins. A developer builtin used as a statement compiles to nothing
(arguments are not evaluated); using its value or referencing it fails the load.
`prof_begin`/`prof_end` statements compile to nothing. SHA-256 covers decompressed source bytes
after terminator removal. Fingerprints hash IR version 6 and each module's
`(site, realm, origin)`; server modules of installed script families are instantiated. Calls, `::f` references (script
`Function` or native `Builtin`), locals (per-function slots) and field names (symbol ids)
are resolved at load, and `install` binds native slots once.
UTF-8 and single-byte sources are supported; developer blocks are excluded.

## Language and scheduling

IR supports calls/references/indirect calls, receiver threads, locals, object fields,
arrays/index lvalues, constants, for/foreach/while, switch, conditional expressions,
waits and receiver-bound events. Localized strings and animation literals retain
separate value types. Values are `Int(i32)` and `Float(f32)`. The operator, cast, truth and
string-conversion rules are:
- int/int `/` yields a float. Division or `%` by zero yields zero and does not fault.
- `% & | ^ << >>` accept ints only.
- A type mismatch in `==` or a comparison is a runtime error.
- Only ints and floats have truth.
- Floats print with MSVC `%g`.
- Overlong int literals wrap.

File-scope constants support scalar and vector literals, including numeric
expressions in vector components. Vector components require integers or floats,
with the same conversion as runtime vector expressions.


Ordinary strings retain their original bytes. UTF-8 strings and legacy byte
strings use the same GSC string type; equality and array keys compare bytes.
String size, indexing, `getSubStr`, `isSubStr`, `strTok` and `toLower` operate
on bytes, with ASCII-only case conversion. Concatenation preserves the bytes.
Single-byte source literals retain the source encoding. The public
`ScriptString::as_bytes()` and `Value::byte_string()` APIs carry non-UTF-8
values without converting them into overlong UTF-8 strings. Text adapters for
asset names, dvars and presentation remain separate from this byte storage.

Arrays copy on assignment and argument/event transfer; objects preserve aliases.
Foreach snapshots sorted keys.

Scheduling uses time buckets:
- The run loop pops the head of the current bucket.
- `wait` pushes to the head of its target bucket.
- `waittillframeend` pushes to the tail of the current bucket.
- `wait` counts server frames: an int is 20 frames per second, a float rounds
  `f * 20 + 0.5` down in f32, and a nonzero wait is at least one frame. A negative or
  too-long wait faults. `wait 0` resumes in the same frame, after the current thread yields.
- `thread f()` runs `f` inline until its first yield, then the spawner continues.
  Calls and thread calls share a limit of 31 frames across the running thread and the
  spawners suspended under it. An endon that fires on a suspended spawner unwinds it
  when control returns to it.
- `childthread f()` schedules like `thread` and copies the caller's existing
  `endon` handlers, including those in enclosing function calls. The copies end
  the entire child and remain armed after the caller returns. Later handlers in
  the caller are not inherited. Receiver, qualified and indirect calls support
  both thread modes; neither mode resolves a native as a script function.

Events use one waiter list per program:
- Notify walks the waiters oldest-registration first and never runs script.
- Woken threads go to the head of the current bucket, so they resume newest first.
- `waittillmatch` skips non-matching payloads silently.
- `endon` is scoped to the registering frame. Firing it kills that frame and the frames above
  it. The caller resumes with `undefined`, or the thread ends if the root frame was killed.
- An endon registered before a waittill on the same event wins.

Deleting an entity ends the threads that `waittill` or `endon` on it, checked at the
start of each tick and before a thread resumes. A thread whose `self` is deleted keeps
running. The `delete` native sends `death` before retiring the entity. A deleted entity
keeps its fields until the frame ends, so `death` handlers can still read them; after that, references point at a
fieldless dead entity: field reads give `undefined`, writes land, `isdefined` is false
and the id is reclaimed by heap collection.

Runtime errors recover: the failed
operation leaves `undefined` in place of its results and the thread carries on. A failed
condition falls into the body, a failed switch takes the default, and a failed `foreach`
operand iterates nothing. Each site is logged once (`gsc: runtime_error at=… fault="…"`)
and counted. A thread that runs 16M instructions in one resumption is killed as a runaway
loop (level startup runs in one frame and needs over a million on the larger maps). A
panicking builtin is a runtime error of its caller. Array copies have bounds.
Heap collection follows globals and live threads, including cycles; natives must
keep persistent script values in script-owned roots rather than retain raw handles.

## Engine boundary and restoration

Natives are grouped by what they touch; the core three:
- `script/host/natives/iw4.rs`: dvars (`getdvar*` with an optional default for a missing dvar,
  `setdvar` storing a localized value as its reference, `setdvarifuninitialized`,
  `makedvarserverinfo`), precache and `loadfx` (allowed only while the first tick is
  loading), string helpers, presented client state (fog, vision, ambient).
- `script/host/natives/math.rs`: math in degrees, vectors, deterministic randomness (a runtime-owned
  xorshift seeded from the program fingerprint, so clones and replays draw the same
  numbers), substrings, and string tables (case-insensitive lookups, `""` or `-1` when
  missing).
- `script/host/natives/engine.rs`: entity queries and `spawn`, entity methods (origin, model,
  visibility, contents, link offsets, attachments), timed moves and rotations that
  notify `movedone`/`rotatedone`, radius-trigger `istouching`, `placespawnpoint`,
  hud elements, world traces against the map's clip, team scores and radar, match data,
  level settings, and weapon facts from the captured weapon table.

Argument reads live in `script/host/args.rs`, array helpers in `script/host/arrays.rs`,
and table lookup in `script/host/tables.rs`. All three use `RoundScript` with explicit access to `MatchScript`
(`arrays`, `next_object`, `tables`, `rng`). The compiler is `sim::script::compiler`:
`Token`/`lex` and the call, expression, assignment, and statement methods sit in child
modules, and `Parser` stays in the parent.

`SoundExists(alias)` checks the composed match sound-alias catalog, installed
before script entry points run even when audio output is disabled. Host aliases
resolve case-insensitively; imported aliases use an explicit installed-family token
prefix. A missing catalog reports an unavailable native error. Arguments use the usual
script string conversion.

`Objective_Team(index, player)` is an extension: given a player instead of a team
name, the objective shows to that player alone, in team and free-for-all modes
(for example, a family-owned sensor grenade script). The compass draws objectives in
free-for-all only when they are addressed this way.

`AmbientPlay(alias, [fadeSeconds])`, `AmbientStop([fadeSeconds])` and
`SetAC130Ambience(alias, [fadeSeconds])` update persistent sound state in
snapshots. The client prepares changed aliases, selects authored variants and
applies voice fades. The player AC130 flag selects the separate AC130 track;
leaving AC130 restores the normal track. Stops affect global ambience, while
CreateFX emitters retain their own playback. Joining clients use the remaining
fade deadline.

`PlayRumbleOnEntity(name)` and `StopRumble(name)` are entity methods;
`PlayRumbleOnPosition(name, position)` is a global function. Names must be
registered by `PrecacheRumble` during script loading. These commands send
ordered events to the client controller mixer. Authored rumble graphs determine
motor intensity and duration; `broadcast`, `range` and `fadeWithDistance`
determine spatial eligibility and attenuation. Represented moving entities use
their presented positions. Named stops affect matching entity rumbles, including
local weapon notetracks. Vibration preferences, focus, pad selection and player
lifecycle still govern controller output. Missing definitions or graphs are
diagnosed by the client.

`PlaySound(alias)` requires a live entity and emits a sound event carrying that
entity's number. Campaign `PlaySound(alias, notifyName, [replaceAllowed])` sends
reliable tracked playback, retaining one pending notify/alias on the entity.
The command carries its alias name because reliable audio can precede the
snapshot's alias table. Only the designated campaign audio client can acknowledge
completion. The client follows the actual primary voice, including failure and
stop; secondary layers do not delay that acknowledgement. A full action queue
retains completion until it can be sent. Playback acknowledgements remain separate
from movement input and input locks. The server marks matching entity/alias
completion, then releases the GSC wait during entity processing before thinking.
`IsWaitingOnSound` reads the pending wait until that phase. Replacing a pending
wait first sends new playback and raises the previous notify, then requires both
replacement flags; refusal retains the old wait and those partial effects.
Entity deletion discards its wait. Notify names retain GSC byte-string identity.
This transport uses the 255-entry runtime alias table.

Missing/looping aliases, unknown looping flags and a full alias table error
before playback. Invalid argument counts do not emit a sound. Use `PlayLoopSound`
for looping aliases. Tracked playback currently requires the campaign audio owner;
co-op expansion, restore/resync and complete native timing parity remain open.
`PlaySoundAsMaster` now uses the same entity/wait contract with a distinct master
mode. Its one-argument form emits native entity event4; the frontend carries the
forced-master property into actual playback, including secondary layers. Normal
aliases flagged master also participate. IW4 alias capture retains the authored
slave percentage; only non-master slave voices receive the native gain formula.
Concurrent masters contribute the maximum percentage among aliases flagged
master; a forced ordinary alias activates ducking without adding its percentage.
`snd_slaveFadeTime` defaults to500ms, accepts0..5000 and ramps both the phase and
maximum percentage; zero switches immediately. The existing presentation dvar
path carries script changes to the callback. Missing/nonfinite percentages for
flagged aliases produce a typed audio failure. Other families retain unknown
master/slave policy rather than interpreting their flag words as IW4. Menu audio
is isolated, and match/timeline effect reset clears the mixer phase. PCM tests
verify actual output gains and completion for both GSC entry points. Full scene/
speech/lip acceptance, virtual-voice admission and exact native update timing
remain open.

Sounds the scripts play (`playLocalSound`, `playSoundToPlayer`, `playSoundToTeam`)
reach the client. Effects, rumble and earthquakes validate their receiver and are
kept in `RoundScript.presented` or dropped. In-game menus (team, class, escape, leave-game,
scoreboard header) and the IW4L frontend run from menuDefs through the same ordered
GPU menu pass. `crates/ui/menus/frontend.json` defines IW4L navigation and native
menu styling; frontend service commands connect it to session and master APIs.

`MoveSlide(center, radius, velocity)` starts continuous script-mover motion with
an offset collision sphere, gravity and collision-plane sliding plus an 18-unit
step attempt. `slideVelocity` reads the current velocity and accepts finite vector
writes while sliding; it reads zero when inactive. `StopMoveSlide()` freezes the
current pose without emitting `movedone`. Stop sliding before a timed origin move.
Linked players follow the mover and are excluded from its collision query.

`CanMantle()` probes the current player's facing direction against mantle
surfaces, landing space and clearance using the movement collision backend.
It leaves player state unchanged. `ForceMantle()` repeats that probe and enters
the existing mantle root-motion controller without requiring jump input.
Dead or linked players and unavailable ledges cannot start a mantle; a failed
force request reports a native error. Authored mantle animations are used when
loaded, with the existing movement fallback otherwise.

`AllowADS(bool)` gates ADS through the replicated player weapon flags.
`AllowSprint(bool)` gates new sprints and ends an active sprint when disabled.
`IsReloading()` and `IsSwitchingWeapon()` inspect both weapon hands;
`IsDualWielding()` inspects the held inventory weapon's dual-wield latch.
`GetCurrentWeaponClipAmmo()` reads the held weapon's primary-hand clip, and
`GetOffhandPrimaryClass()` reads the current offhand class.
`SwitchToWeaponImmediate(weapon)` arms an owned weapon immediately, cancels its
pending switch/reload state and preserves the player's ammo inventory.

`MoveX/Y/Z(distance, seconds, accelSeconds = 0, decelSeconds = 0)` move an
non-player entity by a relative distance along the corresponding world axis,
with the same ramp and `movedone` behavior as `MoveTo`. `IsLinked()` reads the active
entity or player link. `LocalToWorldCoords(vector)` rotates a local vector by
the entity's angles and adds its origin; players use their current view angles.

`GetFirstArrayKey(array)` and `GetNextArrayKey(array, previousKey)` traverse
keys in the same deterministic descending order as `GetArrayKeys`, preserving
integer and string key types. Empty arrays and the end return undefined. A
previous key must still exist; mutation during iteration is not stabilized.

`Kick(clientNumber, reason = "EXE_PLAYERKICKED")` requests client retirement.
The reason must contain 1–256 bytes and no control characters. Repeated requests
before retirement preserve the first reason. The client receives a terminal
control failure through the existing reliable route; transport membership is
retired, and the next authority tick runs the script disconnect callback and
removes the player, owned HUD, trigger claims and command state.

`SetSlowMotion(startScale, endScale = 1, seconds = 1)` changes the rate of the
50 ms gameplay ticks and the client game clock. Script waits, entity mechanics,
weapons and player simulation follow that rate together. Scales must be finite
and positive; zero duration applies the end scale immediately. The transition
is linear over real time: its phase is recovered from the integrated game clock
so snapshots and late joins share the same transition. Bevy frame quantization
and its time clamp still apply. Raw input timing and connection clocks use real
time. Removing the match's time policy restores normal speed.
Killcam snapshots retain the current server time policy while replaying the
archived world. Their slow-motion transition stays on the live server clock,
so recorded final killcams preserve the script's pacing during playback.

`PhysicsExplosionSphere(origin, outerRadius, innerRadius, magnitude)` applies an
outward impulse with an upward bias, full strength inside the inner radius and
linear falloff to zero at the outer radius. It wakes server bodies registered by
`PhysicsLaunchServer/Client`, including bodies that have settled, and sends the
sphere to clients for map props with a physics preset. Client props use their
mass and explosive-force scale; server script bodies use unit mass. Timed scripted movement or vehicle teleport cancels a
server body's physical motion.

Killstreak hardware:
- `spawnHelicopter` arms the vehicle with its VehicleDef `turretWeaponName` (captured
  from the zone, including weapon bodies loaded inline through the vehicle), so the
  gunner's `turret_fire` → `fireWeapon` loop needs no `setVehWeapon`. Turret and vehicle
  weapons with clip 0 or zero fire/raise times are valid combat rows.
- Sentry placement traces a ray to 42 + 5 units ahead,
  a ±30 box drop, normal ≥ 0.7, then four feet (17, 20, 10) traced 20 down that tilt the
  seat, failing when two miss. A failed placement keeps the fallback pose.
  `setContents(0)` makes the carried turret non-solid, so the traces skip it;
  `setContents` returns the previous contents for the restore pattern.
- `notifyOnPlayerCommand` notifies go out for every bound command, including
  `+actionslot N`; `+breath_sprint`, `+stance` (tap crouch, 300 ms hold prone) and
  `centerview` act on the client.
- The laptop map (`setminimap` frame, location selection with direction and yaw) draws
  from the map's compass corners.
- Entity allocation is shared by movers, grenades, missiles and dropped items. A full
  pool drops the new entity with a warning instead of ending the match. `radiusDamage`
  with an invalid area and `glassRadiusDamage` are no-ops.
- A dying player gets `death` (with the attacker) before `CodeCallback_PlayerKilled`;
  the stock scripts send it themselves only for faux death, so every
  player `endon( "death" )` thread depends on it.

The client draws a carried sentry at the authority's 20 Hz pose, not re-placed per
frame from the predicted player state.

The scripts drive the match phase. Every `level` notify is recorded as a signal, and
the simulation reads them after the scheduler each tick: `prematch_over` finishes the
prematch (movement unlocks), `exitlevel` moves to intermission. `map_restart` frees
the level's entities and hud elements, reloads the scripts and keeps the primitive
part of `game` (and `pers` when asked to persist).
Disconnect requests from transport are queued for an authority frame. The player
callback runs before Rust removes the player state and metadata; it can still read
the name, score and team while the scripts remove the player from their lists.
Scripts run only in the authority world (`try_step`/`step`); the client predicts
without a world. Per-client script control constraints travel in snapshots and are
restored before prediction and command replay. Button-query natives sample raw commands
from the current authority frame, including presses between its first and last commands.
Clones preserve program, heap, threads and waits. Shutdown resets them.
Only terminal faults (exhausted identifiers, inconsistent IR) are sticky; they end the
match and return the host to the lobby. Earlier
writes are not rolled back.

`GetMissileOwner(missile)` returns the original player entity for an adopted missile,
including a lingering grenade after explosion. It returns undefined after that player
disconnects; reconnecting in the same client slot does not transfer ownership.

Damage triggers support `EnableGrenadeTouchDamage()` and `DisableGrenadeTouchDamage()`.
Enabled triggers test the grenade's actual swept movement through their hulls and emit
`damage` (inner explosion damage, grenade entity, direction, zero point, MOD_GRENADE),
then `trigger` when threshold, accumulate and response flags permit activation.
Bullet segments and radius damage use the same activation policy without requiring
grenade touch to be enabled. Splash uses bounds distance, cone/visibility samples and
per-entity radius eligibility. Single-use triggers retire after notification delivery.
Melee tests its central trace with base damage, including physical misses. Terminal
missile impacts test their collision point; duds report MOD_IMPACT. Adopted projectile
impact/splash attribution uses the captured owner object across client-slot reuse.

`IsUsingTurret()` reads the player's authoritative turret-active prone/duck flags.


Player 0 can read and write `percentcompletesp`, `percentcompletemp` and
`percentcompleteso` through `GetLocalPlayerProfileData(name)` and
`SetLocalPlayerProfileData(name, value)`. Names ignore ASCII case; unknown fields
fail. The setter requires an integer and stores its low byte, including values
outside 0–100. Get needs at least one argument and set at least two; extra
arguments are ignored. Other players return undefined before validating arguments.
These fields are separate from `Get/SetPlayerData` and survive script resets and
simulation copies. New completion fields start at zero. The signed integer
`takeCoverWarnings` starts at -1 and preserves all 32 bits through the same
script/profile-file lifecycle.

`GetPlayerSetting("gameskill")` reads the current per-player difficulty dvar and
returns its decimal string. Its PC prefix comparison ignores case and accepts
suffixes after the nine-character name. `g_gameskill_player_1/2` start at1 with
domain0..3; script writes become current on level restart, while internal writes
apply immediately. Unknown settings fail. `scr_dof_enable` is registered true
with save eligibility, so the existing art/DOF script path can read and change it.

Listen hosts load the local profile before scripts run and save changes at the
end of each application frame, before normal console exit. Map replacement
inherits the current authority profile. `profile.cfg` sits beside `settings.cfg`;
`IW4L_PROFILE_PATH` selects a separate file. Client, Replay and Dedicated roles
do not load or save this local file. This storage does not implement multiplayer
account stats, unlocks, rested time or platform identities.


Listen hosts and clients load `account.dat` beside `settings.cfg`, with
`IW4L_ACCOUNT_PATH` as its override. It holds a signing key, derived account ID and
an optional schema-stamped player-data buffer. Listen binds existing buffers to
the local client before level entry scripts start and saves authority changes
at the end of the frame. Schema installation rejects recursive layouts, overflowing
storage spans and fields or array elements that exceed their containers. Packed
booleans use bit offsets; zero-size struct metadata derives its span from fields.
Schema mismatches refuse import. Client frame-end saves
persist supplied snapshots. IW4 relay admission verifies a signed account profile
and binds it before accepting gameplay or releasing Enter. Host changes return
to that owner over control; saved ACKs require an exact successful disk receipt. Replay and
Dedicated roles do not read or write this file. Version 1 files upgrade to keyed
identities while retaining all stat bytes and revisions. Writes use a file lock
and reject stale disk revisions. On Unix the key file is owner-readable/writable.
A new Listen account initializes an 8,188-byte buffer with the first authored schema stamp, ten raw localized
class names and the captured `mp/stats_init.cfg` assignments before level entry
scripts start. Missing or invalid defaults refuse initialization without
publishing a partial record. Existing account records are retained.
`Get/SetPlayerData` use the bound account buffer with exact schema keys and
scalar types. Struct and enum-array names use GSC string casts; indexed arrays
require integer keys. Name matching stops at the first NUL byte. Strings retain
original bytes; failed writes preserve the record.
Missing accounts fail rather than return guessed stats. `developer_script`
blocks script writes. Explicit class selection writes the schema atomically;
reconnecting does not reseed saved classes. Loading any external or unclassified
script blocks persistent-data writes for the entire program, including packaged
callers. Captured zone scripts and runtime-generated scripts carry explicit
packaged/built-in origins. File and plain-map resolvers default to external;
custom resolvers must declare their source origin. Clones and map restarts keep
the loaded program's policy; a fresh program load derives a new policy. Origin
records source delivery, not a signature or platform identity. Full connection
state parity, schema migration and platform identity remain incomplete.
Remote profiles bind a signing key to member, connection, session, epoch and
a fresh host challenge. Duplicate owners and stale contexts refuse admission.
The current game-wire version is141; peers must use matching builds.
Host bots bind distinct temporary accounts before their join actions are queued.
Their 8,188-byte buffers start at zero and use the first installed player schema
without human profile defaults. Script writes and map restarts retain these
records; departure or a new match discards them. Temporary records are excluded
from durable saves. Occupied clients or accounts refuse admission before any
record is imported or initialized.

## Remaining

A live bomb plant/defuse check. Native IW4L menus can create a private lobby,
select a map/mode and start a match. When the match scripts end it, or the host
picks End Game from the pause menu, the host returns to the same private lobby with
map, mode and rules kept, as it does when the map fails to load. A disconnect
returns to the main menu. Browser/public-lobby
actions are connected but still need multiplayer validation. Display mode/resolution, brightness, volume, VSync, shadows, depth of field and
bloom are bound to runtime settings and persisted locally. Surround output, voice/chat
settings, third-person camera and spectator restrictions remain incomplete. A true
dedicated server still runs on the listen runtime.


Campaign map vehicles with spawnflags bit2 have an owned vehicle-spawner kind.
`IsSpawner` reads that kind or actor spawner state, returning0 for other values.
`Vehicle_GetSpawnerArray` selects live vehicle spawners and rejects arguments.
Collision-map templates retain their separate code_classname and do not appear
in script_vehicle queries. Absent campaign entity spawnflags expose integer0.
Active SP vehicle spawning/physics remains incomplete; this classification does
not claim vehicle motion or authored animation readiness.


Campaign map walks capture their Weapon/Vehicle definitions and related world,
projectile and tracer models. LoadedWorld carries them to final match assembly;
map rows replace matching common rows in place and retain their native script
aliases. Source-zone effect pointers are resolved before merging; final linking
uses owned names and leaves missing authored edges unresolved.

`GetPlayerIntelIsFound(number)` and `SetPlayerIntelFound(number)` operate on64
persistent local-profile bits numbered1..64. Invalid numbers read0 and do not
change state. Profile files retain the unsigned bitset; legacy files start empty.
`IsThrowingGrenade` reads primary weapon states OffhandInit through OffhandEnd.
It does not infer throwing from a button or the secondary weapon state.


Campaign first-person hands are prepared from literal script `SetViewModel`
declarations that resolve to loaded, posed models with the required weapon tags.
They own the published mesh catalog independently of multiplayer body kits. Each
hand variant retains strict mount/skeleton/clip ownership checks. The native
stores the selected name in per-client snapshot metadata, and FPV
selection changes rebuild the composition even when the weapon stays the same.
Inventory admission permits the authored GiveWeapon-before-SetViewModel order
when a valid composition exists; presentation still requires the selected model.
Dynamic undeclared model names fail explicitly.

Threat group creation/lookup and sentient assignment use16 slots including the
unnamed default. Names match exactly; duplicates do not consume slots and excess
creation is ignored. SetThreatBiasGroup selects a known name with one argument,
otherwise resets to default; unknown names and non-sentients fail. Threat bias
matrix and AI target evaluation remain incomplete.

`hasEverPlayed_SP` is an owned boolean profile field, initiallyfalse.
`UpdateGamerProfile`/`UpdateGamerProfileAll` request persistence of the owned local
profile through a save generation observed by the existing application writer.
Successful writes acknowledge that generation; explicit requests can save an
unchanged profile. The generation itself is not serialized. Normal Trainer now
persists the first-play flag. This does not supply platform/account profiles.


Campaign map weapon classes are installed as real item entities before map main.
Suspended placements retain their authored transforms; floor placement remains
unsupported. Captured weapon-item ammo bounds seed primary and alternate slots;
ItemWeaponSetAmmo clamps clips and validates before changing an item. Authored
items remain outside the dropped-weapon eviction pool. Acquisition uses the real
use/trigger path. Reserve transfer currently covers ordinary single-weapon caps;
shared-cap, clip-only and multiple compatible-weapon rules remain incomplete.

WeaponHasThermalScope reads the captured weapon property independently of combat
admission and rejects unknown/sentinel weapons. Campaign EnableAimAssist and
DisableAimAssist set/clear the native script-mover flag and replicate it even on
hidden stationary helpers. Controller selection of these helpers remains pending.
Script-selected first-person viewmodels use their own prepared clip-track tables
for every rig variant, alongside their own assemblies and hands.


Campaign SetBlur broadcasts through the existing client view-effect snapshots and
film renderer. Both campaign blur entry points require blur and duration, validate
nonnegative finite values, and truncate seconds to milliseconds. Script-only
transitions preserve interpolation continuity; full native priority arbitration
with other blur producers remains incomplete. In-game script menus select the
match-owned campaign catalog; frontend menus retain their shell catalog. HUD text
directives read actual command bindings for the selected input device.


Campaign snapshots publish the active shared script-dvar namespace separately
from server-info variables. Menu and presentation queries retain
targeted-client precedence. MP does not publish this campaign field. Campaign
player sync delivers queued menu answers to the existing script player, then
returns before multiplayer connection/death callbacks; a popup response can resume
waittill and restore the script's controls and blur.


Campaign ammo fractions query actual weapon ownership and ammunition. Start uses
reserve/start ammo; Max uses reserve/max ammo, or the right clip/clip size for
captured clip-only weapons. Unowned weapons and nonpositive denominators return1;
nonpositive ammo returns0, and positive fractions can exceed1. Invalid/sentinel
weapon names fail. Campaign IsMeleeing reads primary melee phases only; IsFiring
reads firing/rechambering/melee phases in either hand.

Campaign SaveGameNoCommit queues a checkpoint for the completed simulation frame;
CommitWouldBeValid and CommitSave validate its ID and publish the captured image.
SaveGame queues an immediate memory commit. IsSaveSuccessful reports capture
failure state, and IsSaveRecentlyLoaded covers the first two seconds after a
restore. The image owns VM threads, arrays, entity/kernel payloads, simulation
RNG, animation/collision state and mechanics. Restoring rebuilds ECS payload
indexes, restores registered savegame dvars, and retains live account/preferences
and unsaved dvars. Checkpoints are restricted to the same map session.

SimWorld exposes in-process capture, committed-checkpoint access and restore;
restore returns the saved authority tick. Direct restore requires a caller to
coordinate its clock, client histories, input and presentation; it refuses an
active input-program timeline. `checkpoint_load` performs that coordination for
a local campaign through session teardown and fresh map installation. It validates
program/content and authored level data before applying the image, preserves live
profile/account/key bindings and unsaved dvars, installs the saved clock before
releasing load hold, and retains the committed checkpoint for repeated loads.
An old input program retires with its match. Active script menus are rebuilt from
VM state; answered menus are not reopened from old command delivery history.
Active tracked audio and reliable-only EQ/reverb commands are not reconstructed
by this load path. A checkpoint captured while a script awaits sound completion
can therefore retain an unresolved wait after presentation teardown. Such loads
remain incomplete until audio state has a durable restore representation.

Campaign death emits the SP death notification (attacker, cause, optional weapon)
without invoking MP killed callbacks. After the native strict1700ms level-time
deadline, it requests a real-time reload countdown using registered g_deathDelay
(default3000ms). MissionFailed requests that countdown immediately. The host uses
Time<Real>, so the countdown continues during the native slow-motion ramp; it
loads the committed checkpoint or restarts the map when no memory save exists.
Restore clears the engine's reload latch while retaining saved gameplay dvars.

Campaign player hits use SP damage processing rather than the MP damage callback.
Bullet, melee and radius scaling use the registered SP dvars; client damageMultiplier,
criticalBulletDamageDist and deathInvulnerableTime are native player fields. Health
shield activation/expiry and death shield state survive checkpoints. EnableHealthShield,
EnableDeathShield and Enable/DisableInvulnerability control their native state.
Damage and deathshield notifications carry the PC ten-argument form, and their
waiters run before the engine tests health for death. Knockback precedes debug god
mode; blood feedback accumulates and publishes once per frame. SP player hits skip
the MP hit-location multiplier and last-stand processing.

Native death-screen/blur/audio fade effects, retail continue commands and disk
persistence (including PC CommitSave's device-write attempt) remain incomplete.
Damage still needs actor attacker identity/target selection, flagged friendly-grenade
protection and attachment/part hit metadata. The state dump reports saves and capture
failures under entity_lifetimes.campaign_save.

Campaign presentation selects the SP fullscreen blood pass `splatter_alt_sp` and
its `blood_defocus_color_sp`/`blood_defocus_mask_sp` images from SP common. MP
retains `splatter_alt` and its own images. Both paths validate the captured texture,
sampler and state contract and execute the admitted material shader. A scoped
MatchRuntimeProfile carries this choice to HUD and renderer during map installation.

SP PainVisionOn/Off mutate player-state other_flags bit0x800. Campaign pain vision
fades from that script flag, using registered painVisionLerpInRate/OutRate defaults
10/0.3 (range0..30); it does not infer activation from health. The authored damage
blur pulse remains separate. Strong hurt-frame highlights still need native
post-processing comparison; full damage-visual parity is unproven.
