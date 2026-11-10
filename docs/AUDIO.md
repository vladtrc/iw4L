# Client audio

Bevy publishes cue intentions, listener snapshots and desired source scenes.
`AudioRuntime` control resolves cues and media; `render_core` mixes prepared
PCM into CPAL, null or offline output. The callback reads fixed slots and
atomic gains; retirement threads free payloads and join media workers.

The internal rate is 48 kHz, the quantum 128 stereo frames and the physical
pool 128 slots. Control caps logical instances at 2048, processes 64 intakes
and 64 pending steps per pass, virtualizes saturated loops and rejects excess
one-shots. Cancellation and epoch invalidation bypass queues. Weapon attacks
preserve their first 20 ms during replacement; explicit Stop cancels them.

Source scenes replace one pending snapshot. The command inbox admits at most
2048 starts and reserves one coalesced reset per scope. A full start queue
refuses new starts; scope resets cancel queued starts in that scope and precede
fresh starts. Publication takes short locks over bounded state; the consumer
releases them before resolution, media preparation and device synchronization.
It delivers the latest scene after any preceding scope resets.

Each source publication has a generation and shared validity. Removal, a newer
version or explicit Stop invalidates pending layers and active playback even
when control has already taken an older scene. Reusing a stopped key with the
same caller version creates a fresh generation. Other keys sharing its alias
keep their own validity. History retains at most 4096 keys; ordinary omission
requires a newer caller version to reactivate, while explicit Stop removes
that key's history.

Family compilers publish per-variant looping, spatial, voice, gain, pitch, layer
and media policies in their catalog context. Bound sound handles and clip keys
retain its revision; foreign owners and media/source mismatches refuse before playback.
T5 layers activate at resolution; others wait for primary preparation. Layers share
deadlines, cancellation and lifetime, with independent pitch/failure. Unknown looping
uses named one-shot compatibility. [CUES.md](CUES.md) details binding and decode contracts.
Killcam worlds retain the live sound registry for round-result commands. Replay
timeline changes retire world/event cues. Music uses a separate match lifetime,
so a victory theme continues into final killcam and stops on match release or
an explicit music/alias stop. Faction themes sent through `playLocalSound` are
classified from the registered team music aliases before cross-family routing;
ordinary local sound aliases retain the world cue lifetime.

Weapon publication compiles thermal scopes, cue namespaces, melee precedence,
knife substitution and breath aliases. Prediction identifies fire occurrences;
authority verdicts suppress refused shots and duplicate effects. Entity and
animation identities stay distinct. `EventJournal` deduplicates 8192 identities
within 100 ticks; local life invalidates animation cues.

Media caps 4096 keys and 256 queued jobs, including at most 192 prewarm jobs.
Urgent requests promote queued work; two to four workers publish clips as
completed, with one reserved for urgent work. PCM pins at most 256 MiB;
decode scratch reserves 64 MiB. Geometry and partial frames are validated.
Native decoders reserve output, input and scratch before reading and validate
channel/rate/frame metadata. Families own source locators, codec wrapping,
spatial flags and speaker projection. Native policies: [T5](../extensions/t5/docs/RUNTIME.md),
[T6](../extensions/t6/docs/RUNTIME.md), [IW5](../extensions/iw5/docs/RUNTIME.md).

Desired loops use scope/epoch/object/slot versions and retain virtual cursors.
Projectile cues follow authoritative bolts and retire on removal or timeline replacement.
Blade contacts use material cues; retrieval selects the family’s item/ammo cue. Virtual sources retain alias limits; channel caps apply to physical voices.
Control ranks eight map voices with a 0.002 gain floor. Device recovery uses
bounded backoff; one-shots preserve PCM during short outages until their start
deadlines expire. Master volume is applied once. `OfflineRenderer` shares the
mixing kernel. Portal cues take the shortest route through open aperture centres;
snapshot exit availability controls attenuation and panning. General occlusion, streaming and DSP buses/tails remain unfinished.
`IW4L_AUDIO_DIAG=1` enables cue identities/decisions, request/preparation timings,
slow control stages, affinity and device/null frame counts. Set
`IW4L_AUDIO_DIAG_PATH` for a buffered file; the bounded queue counts dropped
records. `dump` and `clip` include readiness and recent decisions. Device frames
measure mixing into callbacks, before device latency or hardware playback.

In a match, `sound [family:]alias [count]` submits one-shot aliases through the
normal audio command path. Count defaults to 1 and is bounded at 8192. An
unqualified alias must be unique across the loaded families. `dump` includes
`pressure_refusals`, a cumulative count of starts refused by the bounded inbox
or pending-work budget, so an overload can be distinguished from silent playback.
