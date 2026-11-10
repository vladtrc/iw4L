use bevy::prelude::*;

use asset_core::FamilyId;

pub const SND_ENT_LOCAL: u32 = u32::MAX;

#[derive(Clone, Debug)]
pub struct PlayAlias {
    pub event: Option<crate::AudioEvent>,
    pub namespace: FamilyId,
    pub alias: String,

    pub fallback: Option<String>,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub enum AliasCommand {
    PlayMusic(PlayAlias),
    Play(PlayAlias),
    PlayPitched {
        sound: PlayAlias,
        pitch: f32,
    },
    StopEntity {
        snd_ent: u32,
    },
    Stop {
        namespace: FamilyId,
        alias: String,
        snd_ent: Option<u32>,
    },
}

#[derive(Clone, Debug)]
pub struct SurfaceSound {
    pub event: Option<crate::AudioEvent>,
    pub volume_scale: f32,
    pub alias: &'static str,
    pub fallback: &'static str,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct Footstep {
    pub sound: SurfaceSound,
}

#[derive(Message, Clone, Debug)]
pub struct WeaponSound {
    pub event: Option<crate::AudioEvent>,
    pub namespace: FamilyId,
    pub alias: String,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct FlameFire {
    pub entity: Entity,
    pub weapon: u32,
    pub player_view: bool,
    pub origin_inches: [f32; 3],
}

#[derive(Message, Clone, Debug)]
pub struct BoundWeaponSound {
    pub event: Option<crate::AudioEvent>,
    pub bank_revision: u64,
    pub index: usize,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ViewmodelNotetrack {
    pub event: crate::AudioEvent,
    pub name: String,
}

#[derive(Message, Clone, Debug)]
pub struct ViewmodelNotetracks {
    pub client: sim::ClientId,
    pub life: sim::LifeSequence,
    pub weapon: u32,
    pub records: Vec<ViewmodelNotetrack>,
    pub discarded: u64,
}

#[derive(Message, Clone, Debug)]
pub struct LandSound {
    pub sound: SurfaceSound,
}

pub fn ent_from_number(number: i32) -> Option<u32> {
    u32::try_from(number).ok()
}
