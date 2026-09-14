pub const WEAPTYPE_BULLET: i32 = 0;
pub const WEAPTYPE_GRENADE: i32 = 1;
pub const WEAPTYPE_PROJECTILE: i32 = 2;

pub const WEAPCLASS_GRENADE: i32 = 6;
pub const WEAPCLASS_SPREAD: i32 = 4;
pub const WEAPCLASS_TURRET: i32 = 8;
pub const BULLET_MAX_RANGE: f32 = 8192.0;
pub const ROCKET_SPREAD_PLANE: f32 = 16.0;
pub const HITLOC_COUNT: usize = 20;
pub const LOCATION_DAMAGE_IDENTITY: [f32; HITLOC_COUNT] = [1.0; HITLOC_COUNT];

pub fn bake_location_damage(
    weap_type: i32,
    weap_class: i32,
    global: [f32; HITLOC_COUNT],
    weapon: Option<[f32; HITLOC_COUNT]>,
) -> [f32; HITLOC_COUNT] {
    if weap_type == WEAPTYPE_BULLET && weap_class != WEAPCLASS_TURRET {
        let weapon = weapon.unwrap_or(LOCATION_DAMAGE_IDENTITY);
        let mut out = LOCATION_DAMAGE_IDENTITY;
        for i in 0..HITLOC_COUNT {
            out[i] = global[i] * weapon[i];
        }
        out
    } else {
        global
    }
}

pub fn location_damage_scale(table: &[f32; HITLOC_COUNT], hitloc: u8) -> f32 {
    let slot = if (hitloc as usize) < HITLOC_COUNT {
        hitloc as usize
    } else {
        0
    };
    table[slot]
}

pub fn location_damage_is_valid(table: &[f32; HITLOC_COUNT]) -> bool {
    table.iter().all(|value| value.is_finite() && *value >= 0.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireWeaponKind {
    Bullet,
    ThrownGrenade,
    GrenadeLauncher,
    Missile,
}

pub const fn fire_weapon_kind(weap_type: i32, weap_class: i32) -> Option<FireWeaponKind> {
    match weap_type {
        WEAPTYPE_BULLET => Some(FireWeaponKind::Bullet),
        WEAPTYPE_GRENADE => Some(FireWeaponKind::ThrownGrenade),
        WEAPTYPE_PROJECTILE => {
            if weap_class == WEAPCLASS_GRENADE {
                Some(FireWeaponKind::GrenadeLauncher)
            } else {
                Some(FireWeaponKind::Missile)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_table_product_and_unknown_slot() {
        let mut global = LOCATION_DAMAGE_IDENTITY;
        let mut weapon = LOCATION_DAMAGE_IDENTITY;
        global[2] = 1.5;
        weapon[2] = 1.2;
        let bullet = bake_location_damage(WEAPTYPE_BULLET, 0, global, Some(weapon));
        assert!((bullet[2] - 1.8).abs() < 1e-6);
        let grenade =
            bake_location_damage(WEAPTYPE_GRENADE, WEAPCLASS_GRENADE, global, Some(weapon));
        assert_eq!(grenade[2], 1.5);
        let mut table = LOCATION_DAMAGE_IDENTITY;
        table[0] = 0.25;
        table[2] = 1.5;
        assert_eq!(location_damage_scale(&table, 99), 0.25);
        assert_eq!(location_damage_scale(&table, 2), 1.5);
    }
}
