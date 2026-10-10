# Game boundary: whose rules run a match

Every game is played by its own rules, from its own data. No game borrows
another game's behaviour; a rule a game does not have yet is said out loud.

**Layers.** Each crate declares one in its `Cargo.toml`
(`[package.metadata.iw4l] layer`, and `game` for format/game crates):

| layer | may depend on | holds |
|---|---|---|
| `neutral` | neutral, format | no game rule or constant: transport, render backend, net transport, GSC pieces (`gsc`), `game_api`, math, diagnostics |
| `format` | neutral, its game's format | how one game's data is laid out (`fastfile_t5`, …) |
| `game` | neutral, its game's format/game | one game's rules (`game_iw4` + the `*_iw4` rule crates, `game_t5`, `game_iw5`, `game_t6`) |
| `session` | anything | picks one game per match; names games only in `crates/session/src/games.rs` |
| `mixed` | anything | crates still holding several games' rules; the list may only shrink |

**One game per match.** The match's game is the map's family. `games.rs` maps
it to that game's answers to the `game_api` traits: `GameScripts` (builtin
catalog, startup, engine dvars), `GameModes` (`ModeRules`: match flow, last
stand, limits, movement, weapons, `HudRules`; the library's zombies maps),
`GameMenus` (menu fonts, `MenuLayout`), `GameVision` (shellshock), the native
registry (engine services + the game's own set) and menu expression parsers
(`menu_expr`). Assets are looked up in the match's or the asset's own game,
never another game's same-named asset. Where `games.rs` lends one game another
game's rule, it is one visible, ledgered line (today: Black Ops 2 moves and
fires by MW2's rules, by owner decision).

**Unknown, not borrowed.** A game crate answers a rule it does not have with
`Rule::Unknown(unknown!("t5.area.rule", what, needs))`. The caller reports it
(`gsc: refused … unknown=<id>`, a gap line) and applies nothing in its place.
Every id is listed in that game's ledger, [`fidelity/<game>.md`](fidelity/).
So Black Ops multiplayer and MW3 maps are refused today: their gametype
scripts used to run on Modern Warfare 2's natives.

**Enforcement.** `make boundary` (`cargo xtask boundary`) checks:

* graph — the dependency rules above;
* patterns — outside game/format crates no code names a game (`GameModeKind::Zombies`,
  `ZoneGame::T5`, `AssetNamespace::Iw4`, `FamilyId::…`, `Realm::…`, a game crate
  path, a `"t5"` literal, `is_t5`/`is_zombies`); `games.rs` and the identity
  type itself are exempt;
* ledger — every `unknown!` id is in its game's ledger.

Today's debt is `xtask/boundary/allow.txt`, a ratchet: anything not in it is
new, an entry that occurs less often is stale, and `--update` only lowers it.
`BOUNDARY_ARGS=--enforce` exits non-zero on new or stale entries, and the
pre-commit hook (installed by `cargo xtask mr`) refuses such a commit; CI
(`.github/workflows/boundary.yml`) runs the same check on every push and pull
request.
