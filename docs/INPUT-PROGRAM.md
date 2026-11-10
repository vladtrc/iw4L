# Authority input programs

A listen host with cheats can execute a complete command tape:
`input_program load <path>`, `input_program status`, `input_program dump <path>`.
Prepare the actor using normal match actions and release all buttons first.
For a rehearsal with different actor durability, `input_program prepare_health
<client> <health>` sets that alive actor's initial health and maximum health
on a cheat-enabled listen host. It refuses after any input program is installed;
it cannot inject a heal or damage during a take.
`input_program prepare_actor <client> <json>` prepares a fixed name, zero-based
IW4 rank/prestige, native card title/icon table rows and score/kills/deaths under
the same restrictions. The stock script's round `pers` statistics are prepared
too, so subsequent kills/deaths keep the initial scoreboard. Updating a player's
kill total also adjusts the round's kill history by the same difference, so a
prepared ongoing match does not award first blood again. Card rows are
validated against loaded content; this does not unlock or rank up an account.
Optional `streak: {count: 11, loadout: [predator_missile, harrier_airstrike, emp],
last_earned: harrier_airstrike, owned: []}` prepares the current life counter and
selected rewards. Names/costs are validated against the loaded table; count is
0..25 and loadout has three different costs. Stock GSC setup and owned grants run
on the next authority step, before recording; allow that step before loading a
tape. Subsequent kills earn rewards normally. Dumps include `initial_streak` and
`final_streak` (count, last earned, owned rewards).
For a team mode, `input_program prepare_team_scores <allies> <axis>` prepares
the native script scores before installing a tape. It requires an active cheat
listen host, nonnegative scores below the match score limit and no installed
program. The match must not have ended and its script team scores must already
be initialized. Subsequent kills and match completion use the normal game scripts.
External Rust callers use `SimWorld::prepare_input_team_scores` under the same
simulation restrictions.
Load before the take; commands start on the next authority step.

The JSON object contains `schema_version: 1`, `quantum_ms`, `first_sequence`
and `samples`. Quantum is a positive divisor of 50, or zero to preserve irregular recorded
command times without resampling. Recorded times must be strictly increasing
and positive; the final time is the duration. Commands are issued in the
authority step ending at or after their timestamp, including empty steps across
recorded gaps. Total duration is a
multiple of 50 ms, at most 120 seconds. Every sample contains integer
`time_ms`, `buttons`, `angles: [pitch,yaw,roll]`, `weapon`, `weapon_mapped`,
`off_hand_index`, `forwardmove`, `rightmove`. Optional `remote_control: [pitch,yaw]` preserves the two recorded byte values (0..255, interpreted as signed steering by the native missile). It defaults to zero; retain the REMOTE_CONTROL button for recorded missile control. Other UserCmd fields are zero.
With a positive quantum, times are consecutive quantum offsets starting at quantum_ms. Angles are
absolute command units (65536 per turn), including the prepared delta-angle
basis; they are not mouse deltas. Weapon IDs must already be resolved. Optional `future_weapons: [id,...]` declares known catalog weapons that can be acquired during the take; it grants nothing. Actual possession is checked before each authority tick issues commands. A zero weapon means no selection request, which is useful during pickup/replacement.

The simulation is the exclusive input owner for that actor. Render input and
prediction stop contributing. Actor actions fail the take. Ownership remains
after completion or failure until the match is torn down; reload the map for
another take. Keep neutral samples until the actor has settled; after completion
no further commands are issued, while projectiles and match scripts continue.
Capture is limited to the longest tape plus 120 seconds of postroll; exceeding
that budget fails the take and stops accumulating evidence. Exclusive ownership
still lasts until teardown.

The dump records each consumed command's canonical transport bytes, sequence,
and the tape's original canonical `source_bytes`. `bytes` remains the command
after native script constraints (for example, returning from a killstreak item
to the previous weapon); source matching uses `source_bytes` when present.
The two channels keep a stock script override visible without mistaking it for
an edit to the authored tape. Older dumps have only `bytes`.
It also records
authority tick, position, velocity, view angles and weapon state, plus the
program digest, spread causes, accepted shots and actual projectile launches.
`projectile_impacts` exports the native impact log filtered to that actor and
take start (the world retains the latest 64 impacts): geometry, terminal collider,
position and damage distinguish an obstructed throw from an aim miss. `complete`
requires all samples to have been consumed without an ownership failure.
Equal commands alone do not prove equal outcomes: preserve the initial player
state, life sequence, content, match seed and simulation context as well.
This tool does not implement remote program upload or full-world checkpoints.

Optional `scoreboard_window_ms: [start,end]` authors a local Tab hold within the
tape duration. It has its own digest domain and is published in snapshot client
presentation state, so demo playback shows the same scoreboard. It leaves
weapon/movement commands intact. Dumps retain the shown ticks and positions,
initial cast metadata, native GSC score transitions and final killcam intervals.
An interval's `closed_tick` is present only after its spectator seat disappears
or is replaced. A partial interval alone does not prove a completed final camera.
`wait finalkillcam` holds the console FIFO until a local input-program actor has
observed a native final killcam and all final killcam seats have closed. Dump and
stop recording then, before stock postgame cleanup retires the match.

Schema 2 additionally requires `actors`, an array of companion tapes with
`client`, `quantum_ms`, `first_sequence` and `samples`. Actors must be distinct
and prepared alive. Optional `stop_on_death: true` ends an opponent track on death in its prepared life; a new life still fails. All tapes start on the same authority tick, each relative
to its own prepared command clock. Installation validates every tape first.
The dump includes each actor's receipts and a combined ensemble digest.
A failure stops all owned actors; normal bot control yields to the tapes.
Movement and shot traces retain native per-client execution order, so fit a
moving target to the state actually sampled by the trace, not just wall time.

For editable Rust choreography, YAML recipes and automatic tape generation, see [SCENARIOS.md](SCENARIOS.md). Dumps also include health, inventory, weapon-use flags, ADS, terminal death ticks, match events, pickups and the final score state.
