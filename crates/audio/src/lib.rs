mod aliases;
mod ambient;
mod backend;
mod clip_store;
mod emit;
mod entity_events;
mod frontend;
mod messages;
mod objectives;
mod pcm;
mod playback;
mod plugin;
pub mod policy;
mod shellshock;
mod space;
mod start;
mod voice;

pub use aliases::{
    StepGait, footstep_aliases, gear_rattle_alias, land_aliases, mantle_gear_alias,
    quiet_surface_alias, select_cg_fire_alias, select_fire_alias, step_prefix,
    surface_alias_candidates, world_surface_alias,
};
pub use ambient::{
    MAX_ACTIVE_MAP_EMITTERS, MIN_AUDIBLE_EMITTER_GAIN, MapAmbient, MapAmbientBooted, MapEmitter,
    SoundIwd, stop_map_ambient, update_map_emitter_gain,
};
pub use clip_store::ClipStore;
pub use emit::{BobCycleTracker, emit_footstep_on_bob_wrap, emit_weapon_fire};
pub use match_set::AudioReady;
pub use messages::{
    Footstep, LandSound, PlayAlias, SND_ENT_LOCAL, StopAlias, ViewmodelNotetracks, WeaponSound,
    snd_ent_from_number,
};
pub use pcm::{LivePan, LoopingPcmAudio, PcmAudio, decode_audio_bytes};
pub use playback::{
    AmbientListener, Channel3d, MissingAliasGaps, SoundBank, SoundPickState,
    world_oneshot_channel_gains, world_oneshot_pan,
};
pub use plugin::AudioPlugin;
pub use policy::music::ScriptMusicHost;
pub use space::{distance_inches, transform_inches};
pub use start::{
    SoundClass, StartDecision, StartDecisions, StartFailure, StartOutcome, SuppressReason,
};
pub use voice::VoiceOccupancy;

mod map_doors;
mod match_set;
