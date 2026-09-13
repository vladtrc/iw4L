pub const PLAYER_ANIM_RESTART_TOGGLE: u16 = 0x200;

pub const PLAYER_ANIM_INDEX_MASK: u16 = 0x1ff;

pub const PLAYER_ANIM_RAW_MASK: u16 = 0x3ff;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct PlayerAnimValue(u16);

impl PlayerAnimValue {
    pub const fn from_raw(raw: u16) -> Option<Self> {
        if raw & !PLAYER_ANIM_RAW_MASK == 0 {
            Some(Self(raw))
        } else {
            None
        }
    }

    pub const fn raw(self) -> u16 {
        self.0
    }

    pub const fn effective_index(self) -> u16 {
        self.0 & PLAYER_ANIM_INDEX_MASK
    }

    pub const fn restart_toggle(self) -> bool {
        self.0 & PLAYER_ANIM_RESTART_TOGGLE != 0
    }

    pub const fn toggled_restart(self) -> Self {
        Self(self.0 ^ PLAYER_ANIM_RESTART_TOGGLE)
    }

    pub const fn restarting(index: u16, old: u16) -> Self {
        Self(
            (index & PLAYER_ANIM_INDEX_MASK)
                | ((old & PLAYER_ANIM_RESTART_TOGGLE) ^ PLAYER_ANIM_RESTART_TOGGLE),
        )
    }
}

pub const ANIM_PARSE_MODES: &[&str] = &[
    "defines",
    "animations",
    "canned_animations",
    "statechanges",
    "events",
];

pub const ANIMFLAG_LOOPSYNC: u16 = 1;
pub const ANIMFLAG_NONLOOPSYNC: u16 = 2;
pub const ANIMFLAG_COMPLETE: u16 = 8;
pub const ANIMFLAG_ADDITIVE: u16 = 16;

pub const ANIMTREE_PROPERTY_NAMES: &[&str] = &["loopsync", "nonloopsync", "complete", "additive"];

pub const ANIM_BODY_PART_NAMES: &[&str] = &["** UNUSED **", "LEGS", "TORSO", "BOTH"];

pub const ANIM_MT_IDLE: u8 = 1;
pub const ANIM_MT_IDLECR: u8 = 2;
pub const ANIM_MT_IDLEPRONE: u8 = 3;
pub const ANIM_MT_IDLELASTSTAND: u8 = 51;

pub const ANIM_MT_NAMES: &[&str] = &[
    "** UNUSED **",
    "IDLE",
    "IDLECR",
    "IDLEPRONE",
    "WALK",
    "WALKBK",
    "WALKCR",
    "WALKCRBK",
    "WALKPRONE",
    "WALKPRONEBK",
    "RUN",
    "RUNBK",
    "RUNCR",
    "RUNCRBK",
    "TURNRIGHT",
    "TURNLEFT",
    "TURNRIGHTCR",
    "TURNLEFTCR",
    "CLIMBUP",
    "CLIMBDOWN",
    "SPRINT",
    "MANTLE_ROOT",
    "MANTLE_UP_57",
    "MANTLE_UP_51",
    "MANTLE_UP_45",
    "MANTLE_UP_39",
    "MANTLE_UP_33",
    "MANTLE_UP_27",
    "MANTLE_UP_21",
    "MANTLE_OVER_HIGH",
    "MANTLE_OVER_MID",
    "MANTLE_OVER_LOW",
    "FLINCH_FORWARD",
    "FLINCH_BACKWARD",
    "FLINCH_LEFT",
    "FLINCH_RIGHT",
    "STUMBLE_FORWARD",
    "STUMBLE_BACKWARD",
    "STUMBLE_WALK_FORWARD",
    "STUMBLE_WALK_BACKWARD",
    "STUMBLE_CROUCH_FORWARD",
    "STUMBLE_CROUCH_BACKWARD",
    "STUMBLE_SPRINT_FORWARD",
    "IDLELASTSTAND",
    "CRAWLLASTSTAND",
    "CRAWLLASTSTANDBK",
];

pub const ANIM_STATE_NAMES: &[&str] = &["COMBAT"];

pub const ANIM_ET_NAMES: &[&str] = &[
    "PAIN",
    "DEATH",
    "FIREWEAPON",
    "JUMP",
    "JUMPBK",
    "LAND",
    "DROPWEAPON",
    "RAISEWEAPON",
    "CLIMBMOUNT",
    "CLIMBDISMOUNT",
    "RELOAD",
    "CROUCH_TO_PRONE",
    "PRONE_TO_CROUCH",
    "STAND_TO_CROUCH",
    "CROUCH_TO_STAND",
    "STAND_TO_PRONE",
    "PRONE_TO_STAND",
    "MELEEATTACK",
    "KNIFE_MELEE",
    "KNIFE_MELEE_CHARGE",
    "SHELLSHOCK",
    "STUNNED",
];

pub const ANIM_ET_DEATH: u8 = 1;
pub const ANIM_ET_FIREWEAPON: u8 = 2;
pub const ANIM_ET_RELOAD: u8 = 10;

pub const ANIM_COND_NAMES: &[&str] = &[
    "PLAYERANIMTYPE",
    "WEAPONCLASS",
    "MOUNTED",
    "MOVETYPE",
    "CROUCHING",
    "FIRING",
    "WEAPON_POSITION",
    "STRAFING",
    "PERK",
    "DAMAGETYPE",
    "HITLOCATION",
    "HITDIRECTION",
    "AKIMBO",
    "DIVEDIRECTION",
    "RIOTSHIELDNEXT",
    "PLAYERANIMTYPEPRIMARY",
    "FASTMANTLE",
    "SCRIPTED",
];

pub const ANIM_COND_PLAYERANIMTYPE: u8 = 0;
pub const ANIM_COND_WEAPONCLASS: u8 = 1;
pub const ANIM_COND_MOUNTED: u8 = 2;
pub const ANIM_COND_MOVETYPE: u8 = 3;
pub const ANIM_COND_CROUCHING: u8 = 4;
pub const ANIM_COND_FIRING: u8 = 5;
pub const ANIM_COND_WEAPON_POSITION: u8 = 6;
pub const ANIM_COND_STRAFING: u8 = 7;
pub const ANIM_COND_PERK: u8 = 8;
pub const ANIM_COND_DAMAGETYPE: u8 = 9;
pub const ANIM_COND_HITLOCATION: u8 = 10;
pub const ANIM_COND_HITDIRECTION: u8 = 11;
pub const ANIM_COND_AKIMBO: u8 = 12;
pub const ANIM_COND_PLAYERANIMTYPEPRIMARY: u8 = 15;

pub const ANIM_COND_IS_BITFLAGS: [bool; 18] = [
    true, true, false, true, false, false, false, false, false, false, false, false, false, false,
    false, true, false, false,
];

pub fn anim_cond_evaluable(index: u8) -> bool {
    matches!(
        index,
        ANIM_COND_PLAYERANIMTYPE
            | ANIM_COND_WEAPONCLASS
            | ANIM_COND_MOVETYPE
            | ANIM_COND_STRAFING
            | ANIM_COND_PERK
            | ANIM_COND_DAMAGETYPE
            | ANIM_COND_HITLOCATION
            | ANIM_COND_HITDIRECTION
            | ANIM_COND_WEAPON_POSITION
            | ANIM_COND_AKIMBO
            | ANIM_COND_PLAYERANIMTYPEPRIMARY
    )
}

pub const ANIM_DAMAGETYPE_NAMES: &[&str] = &[
    "damage_bullet",
    "damage_explosion_light",
    "damage_explosion",
];

pub const ANIM_HITLOCATION_NAMES: &[&str] = &["hit_torso", "hit_head", "hit_neck", "hit_legs"];

pub const ANIM_HITDIRECTION_NAMES: &[&str] = &["hit_front", "hit_left", "hit_right", "hit_back"];

pub const ANIM_STRAFING_NAMES: &[&str] = &["not", "left", "right"];

pub const ANIM_WEAPON_POSITION_NAMES: &[&str] = &["hip", "ads"];

pub const ANIM_PLAYERANIMTYPE_NAMES: &[&str] = &[
    "none",
    "other",
    "pistol",
    "smg",
    "autorifle",
    "mg",
    "sniper",
    "rocketlauncher",
    "explosive",
    "grenade",
    "turret",
    "c4",
    "m203",
    "hold",
    "briefcase",
    "riotshield",
    "laptop",
    "throwingknife",
];

pub const ANIM_WEAPONCLASS_NAMES: &[&str] = &[
    "rifle",
    "sniper",
    "mg",
    "smg",
    "spread",
    "pistol",
    "grenade",
    "rocketlauncher",
    "turret",
    "throwingknife",
    "non-player",
    "item",
];

pub fn anim_cond_null_value_defaults_to_one(index: u8) -> bool {
    let i = usize::from(index);
    i < ANIM_COND_IS_BITFLAGS.len()
        && !ANIM_COND_IS_BITFLAGS[i]
        && anim_cond_value_names(index).is_empty()
}

pub fn anim_weapon_position_from_pm_flags(pm_flags: u32) -> u32 {
    u32::from((pm_flags & 0x10) != 0)
}

pub const ANIM_PERK_NAMES: &[&str] = &["** UNUSED **", "laststand", "grenadedeath"];

pub const ANIM_DAMAGE_EXPLOSION_NEAR_DIST_SQ: f32 = 6400.0;

pub const ANIM_HITDIR_FRONT_BACK_DOT_SQ: f32 = 0.49985;

pub fn anim_cond_value_names(index: u8) -> &'static [&'static str] {
    match index {
        ANIM_COND_PLAYERANIMTYPE | ANIM_COND_PLAYERANIMTYPEPRIMARY => ANIM_PLAYERANIMTYPE_NAMES,
        ANIM_COND_WEAPONCLASS => ANIM_WEAPONCLASS_NAMES,
        ANIM_COND_MOVETYPE => ANIM_MT_NAMES,
        ANIM_COND_STRAFING => ANIM_STRAFING_NAMES,
        ANIM_COND_WEAPON_POSITION => ANIM_WEAPON_POSITION_NAMES,
        ANIM_COND_PERK => ANIM_PERK_NAMES,
        ANIM_COND_DAMAGETYPE => ANIM_DAMAGETYPE_NAMES,
        ANIM_COND_HITLOCATION => ANIM_HITLOCATION_NAMES,
        ANIM_COND_HITDIRECTION => ANIM_HITDIRECTION_NAMES,
        _ => &[],
    }
}

pub fn bg_random(seed: &mut u32) -> u32 {
    let next = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    *seed = next;
    next >> 17
}

pub fn anim_script_hit_direction(forward_xy: [f32; 2], v_dir_xy: [f32; 2]) -> u8 {
    let dot = forward_xy[0] * v_dir_xy[0] + forward_xy[1] * v_dir_xy[1];
    if ANIM_HITDIR_FRONT_BACK_DOT_SQ <= dot * dot {
        if dot < 0.0 { 0 } else { 3 }
    } else {
        let cross = forward_xy[0] * v_dir_xy[1] - v_dir_xy[0] * forward_xy[1];
        if cross <= 0.0 { 1 } else { 2 }
    }
}

pub fn anim_script_hit_location(hitloc: u8) -> u8 {
    match hitloc {
        1..=2 => 1,
        3..=4 => 2,
        12..=17 => 3,
        _ => 0,
    }
}
