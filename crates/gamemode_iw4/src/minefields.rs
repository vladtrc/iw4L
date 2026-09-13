use crate::radius_damage::g_radius_damage_amount;

pub const MINEFIELD_TARGETNAME: &str = "minefield";

pub const MINE_EXPLOSION_FX: &str = "explosions/grenadeExp_dirt";

pub const MINEFIELD_CLICK_ALIAS: &str = "minefield_click";

pub const MINEFIELD_EXPLO_ALIAS: &str = "explo_mine";

pub const MINEFIELD_CLICK_WAIT_MS: u32 = 500;

pub const MINEFIELD_RANDOM_WAIT_MAX_MS: u32 = 500;

pub const MINEFIELD_RANGE: f32 = 300.0;
pub const MINEFIELD_MAX_DAMAGE: i32 = 2000;
pub const MINEFIELD_MIN_DAMAGE: i32 = 50;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinefieldDetonate {
    pub origin: [f32; 3],
    pub range: f32,
    pub max_damage: i32,
    pub min_damage: i32,
}

pub fn minefield_kill(still_touching: bool, origin: [f32; 3]) -> Option<MinefieldDetonate> {
    if !still_touching {
        return None;
    }
    Some(MinefieldDetonate {
        origin,
        range: MINEFIELD_RANGE,
        max_damage: MINEFIELD_MAX_DAMAGE,
        min_damage: MINEFIELD_MIN_DAMAGE,
    })
}

pub fn minefield_splash(dist: f32, vis_scale: f32) -> i32 {
    g_radius_damage_amount(
        MINEFIELD_MAX_DAMAGE as f32,
        MINEFIELD_MIN_DAMAGE as f32,
        MINEFIELD_RANGE,
        dist,
        vis_scale,
    )
}
