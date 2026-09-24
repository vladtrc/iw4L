use bevy::prelude::*;

use assets::AssetNamespace;

pub const SND_ENT_LOCAL: u32 = 0;

#[derive(Clone, Debug)]
pub struct PlayAlias {
    pub namespace: AssetNamespace,
    pub alias: String,

    pub fallback: Option<String>,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub enum AliasCommand {
    Play(PlayAlias),
    PlayPitched {
        sound: PlayAlias,
        pitch: f32,
    },
    Stop {
        namespace: AssetNamespace,
        alias: String,
        snd_ent: Option<u32>,
    },
}

#[derive(Message, Clone, Debug)]
pub struct Footstep {
    pub alias: &'static str,
    pub fallback: &'static str,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct WeaponSound {
    pub namespace: AssetNamespace,
    pub alias: String,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct BoundWeaponSound {
    pub bank_revision: u64,
    pub index: usize,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

#[derive(Message, Clone, Debug)]
pub struct ViewmodelNotetracks {
    pub generation: frame::WorldGeneration,
    pub client: sim::ClientId,
    pub life: sim::LifeSequence,
    pub weapon: u32,
    pub names: Vec<String>,
}

#[derive(Message, Clone, Debug)]
pub struct LandSound {
    pub alias: &'static str,
    pub fallback: &'static str,
    pub origin_inches: Option<[f32; 3]>,
    pub snd_ent: Option<u32>,
}

pub fn snd_ent_from_number(number: i32) -> Option<u32> {
    u32::try_from(number).ok()
}
