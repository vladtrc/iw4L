# Killcam

Killcams show an archived world, including the attacker's player state,
projectiles, script movers, combat events and script effects.

- Knife and launcher kills begin in the attacker's first-person view. Entity
  focus begins when replay reaches the killing projectile's birth time.
- Pavelow kills focus on the helicopter behind its firing turret. Sentry
  kills use the turret view. Camera roles travel with script mover snapshots.
- Replay starts within the attacker's life responsible for the kill. Archive
  clipping is returned to the script so it can adjust timing and duration.
- An exhausted archive stops supplying a killcam world. Live state does not
  receive killcam HUD metadata when no archived player state is available.
- Projectile cameras keep their last position after impact. Reused entity
  slots cannot redirect the camera to another projectile or mover.

Sound event identities use archived ticks during replay, including entity-ring
sounds and viewmodel animation markers. Repeated snapshots do not replay the
same event. Entering, rewinding or leaving killcam changes the event timeline
and cancels event cues from the previous timeline. Music, local announcements
and other cues without replay event identities continue across these transitions.

FX transitions reset active effects, tracers, marks and projectile bolt state.
Map and script effects are reconstructed from the presented world. Script FX
start times and repeating schedules use the viewer's replay clock.

`net::policy::killcam` selects archive history; `net::policy::seat` assembles
the viewer's world. `render_anim::occupancy::killcam` owns camera transitions.
`Snapshot::view_tick` supplies the clock for replay events. Audio context and
FX ownership follow the entity event timeline.

Mover camera metadata uses game protocol 112; peers must use the same build.
