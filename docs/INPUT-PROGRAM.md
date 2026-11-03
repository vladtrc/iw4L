# Authority input programs

A listen host with cheats can execute a complete command tape:
`input_program load <path>`, `input_program status`, `input_program dump <path>`.
Prepare the actor using normal match actions and release all buttons first.
Load before the take; commands start on the next authority step.

The JSON object contains `schema_version: 1`, `quantum_ms`, `first_sequence`
and `samples`. Quantum is a positive divisor of 50; total duration is a
multiple of 50 ms, at most 120 seconds. Every sample contains integer
`time_ms`, `buttons`, `angles: [pitch,yaw,roll]`, `weapon`, `weapon_mapped`,
`off_hand_index`, `forwardmove`, `rightmove`. Other UserCmd fields are zero.
Times are consecutive quantum offsets starting at quantum_ms. Angles are
absolute command units (65536 per turn), including the prepared delta-angle
basis; they are not mouse deltas. Weapon IDs must already be resolved.

The simulation is the exclusive input owner for that actor. Render input and
prediction stop contributing. Actor actions fail the take. Ownership remains
after completion or failure until the match is torn down; reload the map for
another take. Keep neutral samples until the actor has settled; after completion
no further commands are issued, while projectiles and match scripts continue.

The dump records each consumed command's canonical transport bytes, sequence,
authority tick, position, velocity, view angles and weapon state, plus the
program digest, spread causes, accepted shots and actual projectile launches. `complete`
requires all samples to have been consumed without an ownership failure.
Equal commands alone do not prove equal outcomes: preserve the initial player
state, life sequence, content, match seed and simulation context as well.
This tool does not implement remote program upload or full-world checkpoints.

Schema 2 additionally requires `actors`, an array of companion tapes with
`client`, `quantum_ms`, `first_sequence` and `samples`. Actors must be distinct
and prepared alive. All tapes start on the same authority tick, each relative
to its own prepared command clock. Installation validates every tape first.
The dump includes each actor's receipts and a combined ensemble digest.
A failure stops all owned actors; normal bot control yields to the tapes.
Movement and shot traces retain native per-client execution order, so fit a
moving target to the state actually sampled by the trace, not just wall time.
