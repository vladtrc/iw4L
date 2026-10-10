# BO2 multiplayer UI

In an admitted native BO2 match, press **ESC** to open the pause menu.
The online match continues. Use arrows to navigate, **Enter** to select,
**ESC** to go back, or click an item. Controller menu navigation also works.

**Create a Class** opens five saved custom classes. Choose primary/secondary
weapons, lethal/tactical equipment and a supported attachment from paginated
lists. Available options are validated against the installation's prepared
weapons. Weapon names, descriptions and artwork use decoded BO2 data when present.
You can rename a class, copy the previous class, or clear its extras.
Choose **Use on Next Respawn**: the host must accept the loadout before it applies.
This is a limited loadout editor; full Pick-10, perks and scorestreaks remain absent.

Classes save separately from MW2 classes, alongside the selected profile with a
`.t6-classes.txt` extension. An unreadable existing file is preserved; edits in that
session are not saved over it.

**Settings** includes video, audio, mouse/controller and key binding pages.
Use **Left/Right** to decrease/increase a value; Enter or click increases it.
Video exposes resolution, fullscreen, VSync, FOV, brightness, shadows, bloom
and depth of field. Controls expose mouse and controller sensitivity, inversion,
ADS scale and vibration. Binding capture accepts keyboard, mouse or controller
input; ESC cancels capture. Settings apply live and use the existing profile store.

The runtime HUD shows replicated match time/scores, native weapon name/artwork,
ammo, deaths, a held-Tab scoreboard and respawn/match results. Crosshair arms follow
the presented spread and hide while aiming; injured players get health feedback.
Native images and the distance-field bitmap font come from the owned installation.
The HUD uses native score backing; pause/class/settings pages use native button art.
Unsupported font characters retain the bundled font. Native glyphs wrap and cache
until text, tint, atlas or available width changes.
Layouts are IW4L-owned; BO2 LUI scripts and the multiplayer minimap remain absent.

Hosts can return everyone to the existing lobby with **End Match**.
**Leave Match** disconnects only the local player.
