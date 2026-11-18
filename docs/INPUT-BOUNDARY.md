# Input ownership boundary

Use `UserCmd` inside `PlayerCommand` and `TickInput` as the authoritative command
boundary. Local devices, remote peers, bots and tapes are producers of that
boundary. Producers may choose commands; they may not move players, grant damage
or bypass weapon possession to realize choreography. Preparation is a separate,
validated operation before a run. Cheats are required for the current tape and
preparation APIs.

Each actor has one selected producer in a tick. Tape installation validates all
actors before acquiring any of them. An owned actor suppresses device prediction,
network forwarding and bot control. Authority arbitration also removes foreign
commands, shot samples and actor actions; conflicting actions fail the take.
Failure or completion does not return control to another producer: ownership
ends at match teardown. A companion failure stops every owned actor.

Command times and sequences are explicit. Preserve irregular recorded times;
never derive simulation time from render FPS or resample a tape silently. All
actors start on the same authority tick with their own prepared command clock.
Record submitted canonical bytes separately from commands transformed by normal
script constraints. A successful run requires consumed receipts, correct life,
continuous ticks and no failure; equal commands alone do not prove equal state.
Presentation controls such as the scoreboard are a separate channel and must
not manufacture movement or weapon commands.


Scene layers: [SCENE-CONTRACT.md](SCENE-CONTRACT.md).
