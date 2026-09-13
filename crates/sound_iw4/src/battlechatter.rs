pub const BATTLECHATTER_INFIX: &str = "1_";

pub const STEM_RELOAD: &str = "inform_reloading_generic";

pub const STEM_FRAG: &str = "inform_attack_grenade";

pub const STEM_FLASH: &str = "inform_attack_flashbang";

pub const STEM_SMOKE: &str = "inform_attack_smoke";

pub const STEM_STUN: &str = "inform_attack_stun";

pub const STEM_C4: &str = "inform_attack_thwc4";

pub const STEM_CLAYMORE: &str = "inform_plant_claymore";

pub const STEM_KILLFIRM: &str = "inform_killfirm_infantry";

pub const BATTLECHATTER_STEMS: &[&str] = &[
    STEM_RELOAD,
    STEM_FRAG,
    STEM_FLASH,
    STEM_SMOKE,
    STEM_STUN,
    STEM_C4,
    STEM_CLAYMORE,
    STEM_KILLFIRM,
];

pub const DEATH_VOICE_MIN: u32 = 1;

pub const DEATH_VOICE_MAX_EXCLUSIVE: u32 = 8;

pub const fn death_voice_nationality(axis: bool) -> &'static str {
    if axis { "russian" } else { "american" }
}
