pub const LIGHTWEIGHT_MOVE_SPEED_SCALER: f32 = 1.07;

pub const PERK_BULLET_DAMAGE_PERCENT: i32 = 40;

pub const PERK_EXPLOSIVE_DAMAGE_PERCENT: i32 = 40;

pub const COMBATHIGH_PERK: &str = "specialty_combathigh";

pub const COMBATHIGH_DEATH_VAL: i32 = 3;

pub const COMBATHIGH_DURATION_MS: i32 = 10_000;

pub const FINALSTAND_PERK: &str = "specialty_finalstand";

pub const FINALSTAND_DEATH_VAL: i32 = 4;

pub const FINALSTAND_DURATION_MS: i32 = 20_000;

pub const COPYCAT_PERK: &str = "specialty_copycat";

pub const COPYCAT_DEATH_VAL: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacDamageMeans {
    Primary,

    Explosive,

    Other,
}

#[must_use]
pub fn lightweight_move_speed_scale(has_lightweight: bool) -> f32 {
    if has_lightweight {
        LIGHTWEIGHT_MOVE_SPEED_SCALER
    } else {
        1.0
    }
}

#[must_use]
pub fn combathigh_until_ms(
    loadout_deathstreak: &str,
    cur_death_streak: i32,
    spawn_time_ms: i32,
) -> Option<i32> {
    if loadout_deathstreak != COMBATHIGH_PERK {
        return None;
    }
    if cur_death_streak < COMBATHIGH_DEATH_VAL {
        return None;
    }
    Some(spawn_time_ms.saturating_add(COMBATHIGH_DURATION_MS))
}

#[must_use]
pub fn combathigh_is_active(until_ms: Option<i32>, now_ms: i32) -> bool {
    until_ms.is_some_and(|until| now_ms < until)
}

#[must_use]
pub fn finalstand_should_give(loadout_deathstreak: &str, cur_death_streak: i32) -> bool {
    loadout_deathstreak == FINALSTAND_PERK && cur_death_streak >= FINALSTAND_DEATH_VAL
}

#[must_use]
pub fn copycat_should_give(loadout_deathstreak: &str, cur_death_streak: i32) -> bool {
    loadout_deathstreak == COPYCAT_PERK && cur_death_streak >= COPYCAT_DEATH_VAL
}

#[must_use]
pub fn copycat_weapnext_bind_active(pm_type: i32, in_killcam: bool) -> bool {
    in_killcam || pm_type > 7
}

#[must_use]
pub fn may_do_laststand(
    means: CacDamageMeans,
    is_headshot: bool,
    weapon_is_throwingknife: bool,
) -> bool {
    if is_headshot || weapon_is_throwingknife {
        return false;
    }
    matches!(means, CacDamageMeans::Primary | CacDamageMeans::Other)
}

#[must_use]
pub fn cac_weapon_is_throwingknife(weapon: &str) -> bool {
    weapon == "throwingknife_mp"
}

#[must_use]
pub fn cac_modified_damage(
    damage: i32,
    means: CacDamageMeans,
    inherits_perks: bool,
    attacker_has_bullet_damage: bool,
    attacker_has_explosive_damage: bool,
    victim_has_combathigh: bool,
    weapon_is_throwingknife: bool,
) -> i32 {
    let mut damage = damage;
    let mut damage_add = 0i32;
    match means {
        CacDamageMeans::Primary if inherits_perks && attacker_has_bullet_damage => {
            damage_add = percent_of(damage, PERK_BULLET_DAMAGE_PERCENT);
        }
        CacDamageMeans::Explosive if inherits_perks && attacker_has_explosive_damage => {
            damage_add = percent_of(damage, PERK_EXPLOSIVE_DAMAGE_PERCENT);
        }
        _ => {}
    }
    if victim_has_combathigh && !weapon_is_throwingknife && !matches!(means, CacDamageMeans::Other)
    {
        damage /= 3;
        damage_add /= 3;
    }
    damage.saturating_add(damage_add)
}

fn percent_of(damage: i32, percent: i32) -> i32 {
    damage.saturating_mul(percent) / 100
}
