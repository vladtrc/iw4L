pub const WEAPTYPE_BULLET: i32 = 0;
pub const WEAPTYPE_GRENADE: i32 = 1;
pub const WEAPTYPE_PROJECTILE: i32 = 2;

pub const WEAPCLASS_GRENADE: i32 = 6;

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
