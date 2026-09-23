# Run and poke the live game

How to touch the live process. Observation is [`PERF.md`](PERF.md).

```bash
set -a; . ./.env; set +a          # IW4L_GAMES; DISPLAY=:0 if the session has none
make map mp_boneyard CMDS='spawn assault; wait 2s; quit'
cargo run --profile play -p launcher -- map iw5:mp_overwatch --cmds '…'
```

`make` passes no foreign flags through: in the `Makefile` it is `CMDS`, on the
binary `--cmds`. The colon is a GNU make pattern, so `make map iw5:…` fails —
write `make map ZONE=iw5:mp_overwatch` or use `cargo run`. Recipes: `make
scenario`, `chaos`, `bench` ([`BENCH.md`](BENCH.md)), `bench-live`, `lifecycle-*`
(`*_CMDS` in the `Makefile`). Live recipes use `[profile.play]`; LTO is `PROFILE=release`.

**Controls are frozen until the match starts.** `sim::step` allows movement only
in `MatchPhase::Playing`; `hold +attack`, `+forward` and `move` do nothing before
then. You wait out `PLAYER_WAIT_MS` 15 s + `MATCH_START_MS` 5 s
(`crates/gamemode_iw4/src/prematch.rs`) — bots (≥ 2 live players) cut the first
timer short, a single-player script waits the full 20 s. The verb is shorter:
`wait world; spawn assault; force_match_start; wait 1s; …`, and a script
involving firing, hits or decals **lies with a zero without it**.

**Sync by default.** Ritual `wait`s are unnecessary: a command holds the FIFO
until its own fact — `map`/`demo`/`play` until the swap plus GPU-ready
`WorldScene::spawned`, `spawn` until `AppScreen::InGame`, `move`/`look`/`tp`
until a matching presented pose, `force_match_start` until `Playing`,
`disconnect` until the hold is torn down. `wait Ns` remains, for pacing and
observation windows. The `&` suffix is a loud refusal to wait (`map mp_rust &`),
`!` jumps the queue (in bash write `disconnect !`, or history expansion bites).
The 120 s gate timeout aborts the rest with an echo; parsing is
`crates/console/src/`. Hand-typed `dump`/`clip`/`screenshot` pass through any
wait while preserving the queue, which inside `--cmds` takes an explicit `!`.
A hand-typed `quit`/`exit`/`disconnect` goes further: it jumps **every** hold,
`wait 60s` included, and releases it. A `quit` written into `--cmds` does not —
it is part of the script and waits its turn.

## Verbs and traps

`map spawn class give attach name kill damage move tp look nudge press hold
release bind bot wait mark record stoprecord clip demo dump screenshot ui disconnect quit
finish_run`
plus the debug `force_match_start` / `showpos`. `quit` leaves now and abandons
whatever screenshot was queued or half-written; `finish_run` is the scripted
ending that waits for those files first. `disconnect` leaves the session, not
just the world — it also leaves the room, or closes it when hosting, and works
with no map installed. `bot` is
`add | hold | tp | give | fire`; `dump [name]` writes the current snapshot into
`dumps/`. `clip` writes the last available 45 s into `clips/<ULID>/` (demo + dump)
on host and clients — the host records authority, a client the snapshots it
received plus its own presented state; `demo LATEST` plays it back.

* ADS is `hold +speed_throw`, not `+speed`;
* bolt action (`fire_type=1`): `hold +attack` is **one** shot, and a full
  magazine will not reload itself (`press +attack` ×N, then `press +reload`);
* `look` without `LookState` only writes `ps.viewangles` — no aiming;
* `give` takes a namespace: `give t5:weapon/psg1_acog`, `give iw5:weapon/msr`.
