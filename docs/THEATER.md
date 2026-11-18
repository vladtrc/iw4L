# Theater

Choose **THEATER** beneath **OPTIONS** on the main menu. The library lists
`iw4l-artifacts/demos/*.iw4ldemo` and `clips/*/clip.iw4ldemo`, newest first.
Tiles show map, duration, type and preview over the IW4 menu background.
Refresh after saving a film. Incompatible, empty or damaged recordings show an error.
Use `record [name]` / `stoprecord` in a match, or `clip` for the recent buffer.
Incompatible recordings require the build that recorded them. Player demos and clips
capture the received world and ordinary/final killcams. Theater initially selects
the killcam viewer; other perspectives share that viewer’s archived world.

Play opens the local film editor. Space pauses; period advances one tick;
comma goes back one tick; Left/Right seek five seconds; Home seeks the start.
Down plays at half speed; Up at double speed. F2 toggles HUD, F3 cycles
perspectives, and F4 toggles controls.
Page Up/Down selects a player. Green timeline ticks mark kills; red ticks mark
deaths for that player. Click the timeline to seek or use event buttons.
Seeks reload the map while retaining edit decisions; decoding runs on a worker.

Controller: D-pad/left stick moves the highlighted item; A selects and B exits.
Start toggles controls, North toggles HUD, West pauses, bumpers select a player,
triggers jump events. Left/Right on the timeline seeks five seconds.
Tab/Shift+Tab and Enter also select controls; Tools/Select opens editing tools.

Free camera uses WASD, Q/E and Shift; hold the right mouse button to look.
K places a camera waypoint; Dolly follows a cubic position path and interpolates
rotation and field of view on the same clock as the recorded scene. Blue timeline
markers and waypoint rows show saved camera times. Waypoints retain their
physical world. Dolly interpolates positions and orientations through a unique
portal between adjacent worlds, including separately placed scene worlds;
ambiguous connections require an explicit camera route. Edit pose jumps to a waypoint
and restores its camera/FOV; Update pose replaces it, Set time moves it to the
playhead, and Delete waypoint removes it. Placing a key at an existing time replaces
that key. FOV controls edit free/chase/attached views; Dolly uses saved waypoint FOVs.
Attach object follows a recorded projectile and holds its last camera when
that object is unavailable. First-person and chase follow the selected player.

Tap R/Record to start/end a segment; hold R/Screenshot for a clean screenshot
and preview. Cuts can be trimmed at the playhead, reordered, deleted and previewed. Highlight reel builds merged windows around the selected
player's recorded kills. Save/Load project uses a sibling `.theater.json` file
and verifies the recording's digest before accepting saved edit decisions.

Export demo saves a cut as a playable recording with simulation timestamps.
Existing exports are preserved. Preview cuts plays edited order through seeks; export is a recording, not an encoded video or an upload.

Recordings/clips receive sibling PNG previews; Theater captures missing previews
when first watched. Films hold the final frame for editing; Play restarts them. Exit returns to the main menu. Back closes the library.

`theater` opens the library; `theater play <name>` opens a film; `help theater`
lists editor commands. Segment indices are zero-based.
`replay` owns file decoding/recording; `theater` owns projects, timeline, browser,
controls and cameras. Sharing, voting, party Theater, CODcast/PIP and video
encoding are not implemented.
