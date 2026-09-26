# GSC → IR → Bevy

Gameplay policy belongs to loaded GSC; Rust supplies language and engine primitives.
Every native the IW4 mode programs link is bound. The scripts own the match flow,
player connect/spawn/damage/killed, the in-game menus, and the sounds they play.

## Sources and installation

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
syntax, unresolved names and unbound natives fail the transaction.

Map entities follow the engine: `worldspawn` is kept as level settings (`getnorthyaw`),
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

Natives are linked against a `Catalog` of the game's builtins (generated into
`gsc_ir/iw4_builtins.rs`, split into function and method namespaces), not against the
registry. An unqualified call resolves to a function in the same file, then a builtin
in the call's namespace, then a unique include. Qualified names and `thread` calls
never resolve to builtins. A developer builtin used as a statement compiles to nothing
(arguments are not evaluated); using its value or referencing it fails the load.
`prof_begin`/`prof_end` statements compile to nothing. SHA-256 covers decompressed source bytes
after terminator removal. Fingerprints hash IR version 3 and each module's
`(site, realm)`; only `(server, iw4)` is instantiated. Calls, `::f` references (script
`Function` or native `Builtin`), locals (per-function slots) and field names (symbol ids)
are resolved at load, and `install` binds native slots once.
UTF-8 and single-byte sources are supported; developer blocks are excluded.

## Language and scheduling

IR supports calls/references/indirect calls, receiver threads, locals, object fields,
arrays/index lvalues, constants, for/foreach/while, switch, conditional expressions,
waits and receiver-bound events. Localized strings and animation literals retain
separate value types. Values are `Int(i32)` and `Float(f32)`. The operator, cast, truth and
string-conversion rules are:
- int/int `/` yields a float.
- `% & | ^ << >>` accept ints only.
- A type mismatch in `==` or a comparison is a fault.
- Only ints and floats have truth.
- Floats print with MSVC `%g`.
- Overlong int literals wrap.

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

Events use one waiter list per program:
- Notify walks the waiters oldest-registration first and never runs script.
- Woken threads go to the head of the current bucket, so they resume newest first.
- `waittillmatch` skips non-matching payloads silently.
- `endon` is scoped to the registering frame. Firing it kills that frame and the frames above
  it. The caller resumes with `undefined`, or the thread ends if the root frame was killed.
- An endon registered before a waittill on the same event wins.

Deleting an entity ends the threads that `waittill` or `endon` on it, checked at the
start of each tick and before a thread resumes. A thread whose `self` is deleted keeps
running. Deletion sends no notify.
The per-frame instruction budget (16M, because level startup runs in one frame and the
larger maps need over a million) faults rather than delaying work. Array copies have bounds.
Heap collection follows globals and live threads, including cycles; natives must
keep persistent script values in script-owned roots rather than retain raw handles.

## Engine boundary and restoration

Natives live in three modules:
- `iw4_natives`: dvars (`getdvar*` with an optional default for a missing dvar,
  `setdvar` storing a localized value as its reference, `setdvarifuninitialized`,
  `makedvarserverinfo`), precache and `loadfx` (allowed only while the first tick is
  loading), string helpers, presented client state (fog, vision, ambient).
- `natives_math`: math in degrees, vectors, deterministic randomness (a runtime-owned
  xorshift seeded from the program fingerprint, so clones and replays draw the same
  numbers), substrings, and string tables (case-insensitive lookups, `""` or `-1` when
  missing).
- `natives_engine`: entity queries and `spawn`, entity methods (origin, model,
  visibility, contents, link offsets, attachments), timed moves and rotations that
  notify `movedone`/`rotatedone`, radius-trigger `istouching`, `placespawnpoint`,
  hud elements, world traces against the map's clip, team scores and radar, match data,
  level settings, and weapon facts from the captured weapon table.

Sounds the scripts play (`playLocalSound`, `playSoundToPlayer`, `playSoundToTeam`)
reach the client. Effects, rumble and earthquakes validate their receiver and are
kept in `Runtime.presented` or dropped. In-game menus (team, class, escape, leave-game,
scoreboard header) run from their menuDefs; the frontend shell is still Rust.

The scripts drive the match phase. Every `level` notify is recorded as a signal, and
the simulation reads them after the scheduler each tick: `prematch_over` finishes the
prematch (movement unlocks), `exitlevel` moves to intermission. `map_restart` is
recorded and not acted on.
Scripts run and are checked only on authority frames (`try_step`/`step` with
`StepReason::AuthorityFrame`); prediction of new and replayed commands steps a
script-less world.
Clones preserve program, heap, threads and waits. Shutdown resets them. Restoring
snapshots into an installed authority VM rejects absent script state. Persistent
replay pinning/restoration and client-role lifecycle evidence are still missing.
Faults are sticky and gate later engine phases; earlier writes are not rolled back.

## Remaining

Killstreak natives (turrets, `beginLocationSelection`, `spawnPlane`), `stunPlayer`,
and a live bomb plant/defuse check. The frontend menus (main menu, create-a-class,
lobby, server browser) are still the Rust shell. A true dedicated server still runs
on the listen runtime.
