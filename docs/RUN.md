# Run and poke the live game

How to touch the live process. Observation is [`PERF.md`](PERF.md).

## Launch

```bash
set -a; . ./.env; set +a          # IW4L_GAMES; DISPLAY=:0 if the session has none
make map mp_boneyard CMDS='spawn assault; wait 2s; quit'
cargo run --profile play -p launcher -- map iw5:mp_overwatch --cmds '…'
```

`make` does not pass foreign flags through: in the `Makefile` it is the `CMDS`
variable, on the binary it is `--cmds`. The colon is a GNU make pattern:
`make map iw5:…` fails; write `make map ZONE=iw5:mp_overwatch` or use
`cargo run`. Recipes: `make scenario` / `chaos` / `bench-demo` / `bench-live` /
`lifecycle-*` (`*_CMDS` in the `Makefile`). Live recipes use `[profile.play]`;
for LTO, `PROFILE=release`.

## The match hasn't started — controls are frozen

`sim::step` allows movement only in `MatchPhase::Playing`. Before that,
`hold +attack`, `+forward` and `move` do nothing. You have to wait out
`PLAYER_WAIT_MS` 15 s + `MATCH_START_MS` 5 s
(`crates/gamemode_iw4/src/prematch.rs`); the first timer is cut short by bots
(≥ 2 live players), a single-player script waits the full 20 s. The verb is
shorter: `wait world; spawn assault; force_match_start; wait 1s; …` — a script
involving firing / hits / decals **lies with a zero without
`force_match_start`**.

## The script waits on its own (sync-by-default)

Ritual `wait`s are unnecessary: a command holds the FIFO until its own fact —
`map`/`demo`/`play` until the swap plus GPU-ready `WorldScene::spawned`,
`spawn` until `AppScreen::InGame`, `move`/`look`/`tp` until a matching
presented pose, `force_match_start` until `Playing`, `disconnect` until the
hold is torn down. `wait Ns` remains, for pacing and observation windows.
The `&` suffix is a loud refusal to wait (`map mp_rust &`), `!` jumps the queue
(in bash write `disconnect !`, otherwise history expansion bites). The 120 s
gate timeout aborts the rest of the script with an echo. Queue parsing is in
`crates/console/src/`.
Hand-typed `dump`/`clip`/`screenshot` pass through any wait while preserving
the queue; inside `--cmds` that takes an explicit `!`.

## Verbs and traps

`map spawn class give attach name kill damage move tp look nudge press hold
release bind bot wait mark record stoprecord clip demo dump screenshot ui disconnect quit`
plus the debug `force_match_start` / `showpos`.
`bot` is `add | hold | tp | give | fire`; `dump [name]` writes the current
snapshot into `dumps/`.
`clip` writes the last available 45 s into `clips/<ULID>/` (demo + dump) on the
host and on the clients; the host records authority, a client records the
snapshots it received plus its own presented state.
The source is named in the manifest; `demo LATEST` plays the clip back.
* ADS is `hold +speed_throw`, not `+speed`;
* bolt action (`fire_type=1`): `hold +attack` is **one** shot; a full magazine
  will not reload itself (`press +attack` ×N, then `press +reload`);
* `look` without `LookState` only writes `ps.viewangles` — the script is not
  aiming;
* `give` takes a namespace: `give t5:weapon/psg1_acog`, `give iw5:weapon/msr`.

`make bench-load-session` runs `make menu` with the script
`mark in_menu; map mp_hanoi; spawn; mark in_hanoi; map mp_underpass; spawn; mark in_underpass; quit`.
`python3 scripts/bench-marks.py <log>` computes the intervals between engine
marks; `--json` gives machine-readable output. Build and menu startup are not
part of the intervals.
