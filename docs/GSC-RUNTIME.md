# GSC → IR → Bevy

Gameplay policy belongs to loaded GSC; Rust supplies language and engine primitives.
The cutover is incomplete: ordinary matches currently refuse unsupported native
calls during loading. FFA/DD/DOM gameplay through this VM is not verified.

## Sources and installation

`Program::load` compiles a `SourceResolver`. `FileSources` reads explicit files.
IW4 asset walks retain GSC RawFiles from common_mp and the map; map files override
common files. Sources travel through the common/resident caches into PreparedMatch.
Session preflight compiles the mode and map modules, then the install transaction
installs the program, sets the `mapname` and `g_gametype` dvars and schedules the
`Iw4Startup` entries before authority ticks: `codescripts/struct::initstructs`, the
gametype `main`, the map `main` (if present), then `_callbacksetup`'s
`codecallback_startgametype`. Missing dependencies, unsupported syntax, unresolved
names and unbound natives fail the transaction.
Names are normalized; traversal is rejected.

Natives are linked against a `Catalog` of the game's builtins (generated into
`gsc_ir/iw4_builtins.rs`, split into function and method namespaces), not against the
registry. An unqualified call resolves to a function in the same file, then a builtin
in the call's namespace, then a unique include. Qualified names and `thread` calls
never resolve to builtins. A developer builtin used as a statement compiles to nothing
(arguments are not evaluated); using its value or referencing it fails the load.
`prof_begin`/`prof_end` statements compile to nothing. `Program::native_ledger`
reports call sites, reachable sites and binding for every linked native; the
`gsc_inventory` example prints it per mode. SHA-256 covers decompressed source bytes
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
string-conversion rules are in `context/artifacts/2026-09-25-mp-gsc-semantic-port/11-IR-V3-PART/`:
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

Entity deletion ends threads whose frames or waiters use the entity.
The instruction budget faults rather than delaying work. Array copies have bounds.
Heap collection follows globals and live threads, including cycles; natives must
keep persistent script values in script-owned roots rather than retain raw handles.

## Engine boundary and restoration

Natives currently cover isdefined, gettime, spawnstruct, mover setorigin/delete,
dvars (`getdvar*` with an optional default for a missing dvar, `setdvar`,
`setdvarifuninitialized`, `makedvarserverinfo`), precache and `loadfx` (allowed only
while the first tick is loading), string helpers, and presented client state (fog,
vision, ambient) kept in `Runtime.presented` for a client consumer.
Entity origin reads use authoritative components; other native fields reject.
Authority and replay run scripts through `try_step`/`step`; prediction does not.
Clones preserve program, heap, threads and waits. Shutdown resets them. Restoring
snapshots into an installed authority VM rejects absent script state. Persistent
replay pinning/restoration and client-role lifecycle evidence are still missing.
Faults are sticky and gate later engine phases; earlier writes are not rolled back.

## Remaining cutover

Remaining lifecycle/scoring/perk/streak/use/destructible policy must migrate to script
callbacks and real primitives. The original crate and full FFA/DD/DOM native closures,
player references/fields, globals and effects remain unfinished. Parser inventory,
synthetic VM checks and successful builds do not demonstrate playable original modes.
