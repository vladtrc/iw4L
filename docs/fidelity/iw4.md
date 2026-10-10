# Modern Warfare 2 — fidelity ledger

Rule: Modern Warfare 2 matches run on Modern Warfare 2's own rules and data.
No other game's behaviour stands in, no guessed constant, no stub that
pretends to work. A rule that cannot be recovered is an `unknown!` in
`game_iw4` (or one of the IW4 rule crates) and is listed here with its id;
`cargo xtask boundary` refuses an `unknown!` id that is missing here.

Severity: **V** visible, **A** audible, **G** gameplay, **I** invisible.

## Open

- [ ] **G** `weapon_change_complete` (a Black Ops notify) is raised in every
      game (`sim/src/script/host/weapons.rs`).
- [ ] **G** Modern Warfare 2 matches start `iw4l_t6/equipment` when the zones
      carry it (Black Ops 2 equipment in an MW2 match, `Iw4Startup`) and mark `t6lethal`/`t6tactical` on classes
      (`sim/src/script/host/players.rs`): cross-family composition, an IW4L
      feature (docs/FAMILIES.md), not Modern Warfare 2.

## Closed

- [x] **G** Builtins Modern Warfare 2's scripts bind that only Black Ops'
      native files implemented. The shared ones (`clamp`, `log`,
      `sighttracepassed`, `getassignedteam`, the weapon queries) are served
      from `natives/shared.rs`; Black Ops' no-op, recorded and refused tables
      (`enablelinkto`, `resettimeout`, `setarchive`, `announcement`,
      `addtestclient`, `stopallrumbles`, `vibrate`, …) are not bound for
      Modern Warfare 2 any more: the registry is per game
      (`session/src/games.rs::natives`) and Modern Warfare 2's compiled
      scripts call none of them (no builtin gap at match start).
