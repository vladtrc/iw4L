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

pub const HITLOC_NAMES: [&str; HITLOC_COUNT] = [
    "none",
    "helmet",
    "head",
    "neck",
    "torso_upper",
    "torso_lower",
    "right_arm_upper",
    "left_arm_upper",
    "right_arm_lower",
    "left_arm_lower",
    "right_hand",
    "left_hand",
    "right_leg_upper",
    "left_leg_upper",
    "right_leg_lower",
    "left_leg_lower",
    "right_foot",
    "left_foot",
    "gun",
    "shield",
];

pub const WEAPCLASS_PISTOL: i32 = 5;

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
