mod aliases;
mod ambient;
mod attenuation;
mod backend;
mod breath;
mod clip_store;
mod decode_budget;
mod entity_events;
mod frontend;
mod media_queue;
mod messages;
mod pcm;
mod pcm_budget;
mod playback;
mod plugin;
mod rumble;
mod shellshock;
mod space;
mod start;
mod weapon_sounds;

pub use aliases::{
    StepGait, footstep_aliases, gear_alias, gear_rattle_alias, land_aliases, quiet_surface_alias,
    select_cg_fire_alias, select_fire_alias, step_prefix, surface_alias_candidates,
    world_surface_alias,
};
pub use ambient::{
    MapAmbientBooted, SoundIwd, prepared_match_audio_key, stage_prepared_match_audio,
};
pub use clip_store::{ClipPath, ClipPathCost, ClipPrepCost, ClipStore, clip_prep_cost};
pub use frontend::FrontendAudio;
pub use match_set::{AudioReady, AudioSilent};
pub use messages::{
    AliasCommand, BoundWeaponSound, FlameFire, Footstep, LandSound, PlayAlias, SND_ENT_LOCAL,
    SurfaceSound, ViewmodelNotetrack, ViewmodelNotetracks, WeaponSound, ent_from_number,
};
pub use playback::{
    AmbientListener, MissingAliasGaps, SoundBank, world_oneshot_channel_gains, world_oneshot_pan,
};
pub use plugin::AudioPlugin;
pub use space::{distance_inches, transform_inches};
pub use start::{
    SoundClass, StartDecision, StartDecisions, StartFailure, StartOutcome, SuppressReason,
};

mod destructible_loops;
mod flame_loops;
mod match_set;
mod match_voices;
mod projectile_loops;

mod weapon_lock;

mod script_ambient;
mod script_mix;
mod script_music;

mod admission;
mod control;
mod cue;
mod pending;
pub use cue::CueFailure;
mod device;
mod diagnostics;
pub use diagnostics::{emit as emit_audio_diagnostic, enabled as audio_diagnostics_enabled};
mod render_core;
mod runtime;
mod sources;
pub use admission::AdmissionFailure;
pub use render_core::InstanceStatus;
pub use runtime::{AudioDiagnostics, AudioRuntime};

mod media;
mod mix_bus;

mod offline;
pub use decode_budget::{DecodeMemory, decode_memory};
pub use media::PcmError;
pub use offline::{OfflineError, OfflineRenderer, OfflineSound, OfflineVoice};
pub use pcm_budget::{PcmMemory, pcm_memory};
pub use render_core::{QUANTUM, SAMPLE_RATE};

mod spatial;

mod cue_execution;
mod event;
pub use event::{AnimationMarkerId, AudioEvent, AudioEventId, AudioOccurrence};

pub use ambient::{LoadingAudio, LoadingAudioStatus, PreparedMatchAudio};

mod portal_space;
