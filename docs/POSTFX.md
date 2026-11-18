# Post effects by map family

The map chooses its authored vision preset; weapon family does not choose the
screen grade. Explicit script visions and transitions override the map preset.

| Map family | Presented profile | Remaining native effects |
|---|---|---|
| IW4 | Authored film contrast, brightness, desaturation, tints and glow | Extra glow/sky-bleed file controls need PC verification |

Other map profiles are supplied by installed families; native coverage and
remaining effects are documented in each game's `docs/RUNTIME.md`.

Screen blur, DoF and saved-screen feedback are triggered by gameplay and script
state. They are not an always-on softening filter. Film here means color grading;
there is no automatic grain layer. HUD draws after scene post processing.

See [GSC post effects](GSC-POSTFX.md) for script commands and dvars.
