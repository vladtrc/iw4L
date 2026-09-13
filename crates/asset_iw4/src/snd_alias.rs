use crate::size::{SND_ALIAS, SND_CURVE};

pub const SND_ALIAS_ALIAS_NAME: usize = 0x00;

pub const SND_ALIAS_SUBTITLE: usize = 0x04;

pub const SND_ALIAS_SECONDARY: usize = 0x08;

pub const SND_ALIAS_CHAIN: usize = 0x0c;

pub const SND_ALIAS_MIXER_GROUP: usize = 0x10;

pub const SND_ALIAS_SOUND_FILE: usize = 0x14;

pub const SND_ALIAS_SEQUENCE: usize = 0x18;

pub const SND_ALIAS_VOL_MIN: usize = 0x1c;

pub const SND_ALIAS_VOL_MAX: usize = 0x20;

pub const SND_ALIAS_PITCH_MIN: usize = 0x24;

pub const SND_ALIAS_PITCH_MAX: usize = 0x28;

pub const SND_ALIAS_DIST_MIN: usize = 0x2c;

pub const SND_ALIAS_DIST_MAX: usize = 0x30;

pub const SND_ALIAS_VELOCITY_MIN: usize = 0x34;

pub const SND_ALIAS_FLAGS: usize = 0x38;

pub const SND_ALIAS_SLAVE_PERCENTAGE: usize = 0x3c;

pub const SND_ALIAS_PROBABILITY: usize = 0x40;

pub const SND_ALIAS_LFE_PERCENTAGE: usize = 0x44;

pub const SND_ALIAS_CENTER_PERCENTAGE: usize = 0x48;

pub const SND_ALIAS_START_DELAY: usize = 0x4c;

pub const SND_ALIAS_VOLUME_FALLOFF_CURVE: usize = 0x50;

pub const SND_ALIAS_ENVELOP_MIN: usize = 0x54;

pub const SND_ALIAS_ENVELOP_MAX: usize = 0x58;

pub const SND_ALIAS_ENVELOP_PERCENTAGE: usize = 0x5c;

pub const SND_ALIAS_SPEAKER_MAP: usize = 0x60;

const _: () = assert!(SND_ALIAS_SPEAKER_MAP + 4 == SND_ALIAS);

pub const SND_ALIAS_FLAG_LOOPING: u32 = 0x1;

pub const SND_ALIAS_FLAG_MASTER: u32 = 0x2;

pub const SND_ALIAS_FLAG_SLAVE: u32 = 0x4;

pub const SND_ALIAS_FLAG_FULL_REVERB: u32 = 0x8;

pub const SND_ALIAS_FLAG_RANDOM_START: u32 = 0x20;

pub const SND_ALIAS_FLAG_TYPE_SHIFT: u32 = 7;
pub const SND_ALIAS_FLAG_TYPE_MASK: u32 = 0x180;

pub const SND_ALIAS_FLAG_CHANNEL_SHIFT: u32 = 9;
pub const SND_ALIAS_FLAG_CHANNEL_MASK: u32 = 0x3f;

pub const SND_CURVE_FILENAME: usize = 0x00;

pub const SND_CURVE_KNOT_COUNT: usize = 0x04;

pub const SND_CURVE_KNOTS: usize = 0x08;

pub const SND_CURVE_KNOT_STRIDE: usize = 8;

pub const SND_CURVE_MAX_KNOTS: usize = (SND_CURVE - SND_CURVE_KNOTS) / SND_CURVE_KNOT_STRIDE;

pub const SND_CURVE_DEFAULT_ASSET_NAME: &str = "default";

pub const SND_ENTCHANNEL_FILE: &str = "soundaliases/channels.def";

pub const SND_ENTCHANNEL_MAX: usize = 64;

pub const SND_ENTCHANNEL_DEFAULT_MAX_VOICES: i32 = 0x34;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SndAliasSampleKind {
    Loaded,
    Streamed,

    Reject,
}

impl SndAliasSampleKind {
    pub const fn from_field(field: u32) -> Self {
        match field & 3 {
            1 => Self::Loaded,
            2 => Self::Streamed,
            _ => Self::Reject,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SndAliasFlags {
    pub raw: u32,
}

impl SndAliasFlags {
    pub const fn from_word(raw: u32) -> Self {
        Self { raw }
    }

    pub const fn channel(self) -> u32 {
        (self.raw >> SND_ALIAS_FLAG_CHANNEL_SHIFT) & SND_ALIAS_FLAG_CHANNEL_MASK
    }

    pub const fn sample_kind(self) -> SndAliasSampleKind {
        SndAliasSampleKind::from_field(self.raw >> SND_ALIAS_FLAG_TYPE_SHIFT)
    }

    pub const fn looping(self) -> bool {
        self.raw & SND_ALIAS_FLAG_LOOPING != 0
    }

    pub const fn master(self) -> bool {
        self.raw & SND_ALIAS_FLAG_MASTER != 0
    }

    pub const fn slave(self) -> bool {
        self.raw & SND_ALIAS_FLAG_SLAVE != 0
    }

    pub const fn full_reverb(self) -> bool {
        self.raw & SND_ALIAS_FLAG_FULL_REVERB != 0
    }

    pub const fn random_start(self) -> bool {
        self.raw & SND_ALIAS_FLAG_RANDOM_START != 0
    }
}

pub const SND_CURVE_EVAL_OUT_OF_RANGE: f32 = -1.0;

pub fn snd_curve_eval(knots: &[[f32; 2]], frac: f32) -> f32 {
    let n = knots.len();
    if n <= 1 {
        return SND_CURVE_EVAL_OUT_OF_RANGE;
    }
    let mut i = 1;
    while knots[i][0] < frac {
        i += 1;
        if i >= n {
            return SND_CURVE_EVAL_OUT_OF_RANGE;
        }
    }
    let x0 = knots[i - 1][0];
    let y0 = knots[i - 1][1];
    let x1 = knots[i][0];
    let y1 = knots[i][1];
    (y1 - y0) * ((frac - x0) / (x1 - x0)) + y0
}

pub fn snd_attenuate(knots: &[[f32; 2]], dist: f32, dist_min: f32, dist_max: f32) -> f32 {
    let delta = dist - dist_min;
    if delta <= 0.0 {
        return 1.0;
    }
    let frac = delta / (dist_max - dist_min);
    if frac >= 1.0 {
        return 0.0;
    }
    snd_curve_eval(knots, frac)
}
