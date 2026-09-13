#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsOverlayScrub {
    pub ads_up_time_norm: f32,

    pub ads_down_time_norm: f32,

    pub ads_up_weight: f32,
}

pub fn ads_overlay_scrub(f_weapon_pos_frac: f32) -> AdsOverlayScrub {
    let frac = if f_weapon_pos_frac < 0.0 {
        0.0
    } else if f_weapon_pos_frac > 1.0 {
        1.0
    } else {
        f_weapon_pos_frac
    };
    AdsOverlayScrub {
        ads_up_time_norm: frac,
        ads_down_time_norm: 1.0 - frac,
        ads_up_weight: frac,
    }
}
