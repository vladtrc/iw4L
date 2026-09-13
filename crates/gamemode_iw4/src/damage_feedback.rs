pub const HIT_ALERT_ALIAS: &str = "MP_hit_alert";

pub const DAMAGE_FEEDBACK_SHADER: &str = "damage_feedback";

pub const DAMAGE_FEEDBACK_WIDTH: i32 = 24;

pub const DAMAGE_FEEDBACK_HEIGHT: i32 = 48;

pub const DAMAGE_FEEDBACK_X: i32 = -12;

pub const DAMAGE_FEEDBACK_Y: i32 = -12;

pub const DAMAGE_FEEDBACK_FADE_MS: i32 = 1_000;

pub const SCAVENGER_FADE_MS: i32 = 2_500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeHit {
    Standard,

    HitBodyArmor,

    HitEndGame,

    Stun,

    None,

    Scavenger,
}

impl TypeHit {
    pub const fn as_str(self) -> &'static str {
        match self {
            TypeHit::Standard => "standard",
            TypeHit::HitBodyArmor => "hitBodyArmor",
            TypeHit::HitEndGame => "hitEndGame",
            TypeHit::Stun => "stun",
            TypeHit::None => "none",
            TypeHit::Scavenger => "scavenger",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageFeedbackPulse {
    pub type_hit: TypeHit,

    pub shader: &'static str,
    pub width: i32,
    pub height: i32,

    pub x: i32,

    pub y: i32,
    pub fade_ms: i32,

    pub sound: Option<&'static str>,
}

fn gsc_int(value: f32) -> i32 {
    value as i32
}

pub fn update_damage_feedback(
    type_hit: TypeHit,
    hardcore: bool,
    y_offset: f32,
) -> Option<DamageFeedbackPulse> {
    let mut x = DAMAGE_FEEDBACK_X;
    let mut y = DAMAGE_FEEDBACK_Y;
    let mut fade_ms = DAMAGE_FEEDBACK_FADE_MS;
    let (shader, width, height, sound) = match type_hit {
        TypeHit::HitBodyArmor => (
            "damage_feedback_j",
            DAMAGE_FEEDBACK_WIDTH,
            DAMAGE_FEEDBACK_HEIGHT,
            Some(HIT_ALERT_ALIAS),
        ),
        TypeHit::HitEndGame => (
            "damage_feedback_endgame",
            DAMAGE_FEEDBACK_WIDTH,
            DAMAGE_FEEDBACK_HEIGHT,
            Some(HIT_ALERT_ALIAS),
        ),
        TypeHit::Stun | TypeHit::None => return None,
        TypeHit::Scavenger if !hardcore => {
            x = -36;
            y = 32;
            fade_ms = SCAVENGER_FADE_MS;
            ("scavenger_pickup", 64, 32, None)
        }
        TypeHit::Standard | TypeHit::Scavenger => (
            DAMAGE_FEEDBACK_SHADER,
            DAMAGE_FEEDBACK_WIDTH,
            DAMAGE_FEEDBACK_HEIGHT,
            Some(HIT_ALERT_ALIAS),
        ),
    };
    y -= gsc_int(y_offset);
    Some(DamageFeedbackPulse {
        type_hit,
        shader,
        width,
        height,
        x,
        y,
        fade_ms,
        sound,
    })
}
