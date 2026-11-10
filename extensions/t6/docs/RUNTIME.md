# T6 runtime policies

Native policies and capture live in `extensions/t6/`. The `ext_t6`
facade registers `Family`; application composition selects the required hooks.

## Map post effects

T6; Three-band film, bloom levels/gamma/tints, two-scale blur and highlight compression; HQ bloom, LUT volumes, SSAO and special camera filters

T6 bloom preserves authored RGB and luminance channels independently. Bloom
levels are filtered at quarter, eighth and sixteenth resolution, then composed
in linear color before highlight compression and film grading. Scene exposure
already supplies the display normalization. Bloom intermediates use floating
point textures to preserve values above one.

T6 map presets override common presets with the same name. Disabled film retains
bloom; a malformed preset retains its parse error. Missing controls are not
replaced with a generic contrast boost. Native presets transition their film,
bloom and highlight compression together. User bloom settings apply unless a
script forces the effect; explicit script disable clears bloom or film grading.

## Audio capture

T6 capture retains SAB locators; workers reserve output,
input and FLAC scratch before reading and validate channel/rate/frame metadata.
Descriptor-only SAB entries bypass cross-bank caching. T6 captures native 2D/3D
flags, dry/near distance curves, MP mixer-group hierarchy, priorities and voice
limits from the alias banks and sound-driver globals. Authored start delays
schedule on the audio clock. Six native pan weights use a named stereo fold:
rear/center at -3 dB and LFE at -6 dB.

## Faction cues

BO2 faction music uses native spawn/victory tracks, shared defeat music and the
shared time-running-out cue for both host late-match states. Announcer routes
translate host objective, flag, round and supported reward events into native
faction aliases; mode variants share their base introduction. Routes require
native catalog entries. Shared HUD text pulses use BO2 notification sounds.
References absent from the installed native banks remain reported as missing;
no replacement alias or media is synthesized for them.
