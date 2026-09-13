#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeaponDefEventSoundPair {
    pub world: usize,
    pub player: usize,
}

pub const ITEM_PICKUP: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0x50,
    player: 0x54,
};

pub const AMMO_PICKUP: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0x58,
    player: 0x5c,
};

pub const EMPTY_FIRE: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0x90,
    player: 0x94,
};

pub const RECHAMBER: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xa8,
    player: 0xac,
};

pub const RELOAD: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xb0,
    player: 0xb4,
};

pub const RELOAD_EMPTY: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xb8,
    player: 0xbc,
};

pub const RELOAD_START: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xc0,
    player: 0xc4,
};

pub const RELOAD_END: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xc8,
    player: 0xcc,
};

pub const ALT_SWITCH: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xe8,
    player: 0xec,
};

pub const RAISE: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xf0,
    player: 0xf4,
};

pub const FIRST_RAISE: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0xf8,
    player: 0xfc,
};

pub const PUTAWAY: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0x100,
    player: 0x104,
};

pub const PULLBACK: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0x64,
    player: 0x68,
};

pub const MELEE_SWIPE: WeaponDefEventSoundPair = WeaponDefEventSoundPair {
    world: 0x98,
    player: 0x9c,
};

pub const MELEE_HIT: usize = 0xa0;

pub const MELEE_MISS: usize = 0xa4;

pub const EV_RELOAD: i32 = 0x12;
pub const EV_RELOAD_FROM_EMPTY: i32 = 0x13;
pub const EV_RELOAD_START: i32 = 0x14;
pub const EV_RELOAD_END: i32 = 0x15;

pub const fn pair_for_event(event: i32) -> Option<WeaponDefEventSoundPair> {
    match event {
        0x0a => Some(ITEM_PICKUP),
        0x0b => Some(AMMO_PICKUP),
        0x0c => Some(EMPTY_FIRE),
        0x12 => Some(RELOAD),
        0x13 => Some(RELOAD_EMPTY),
        0x14 => Some(RELOAD_START),
        0x15 => Some(RELOAD_END),
        0x18 => Some(RAISE),
        0x19 => Some(FIRST_RAISE),
        0x1a => Some(PUTAWAY),
        0x1b => Some(ALT_SWITCH),
        0x1d => Some(PULLBACK),
        0x21 => Some(RECHAMBER),
        0x2e => Some(MELEE_SWIPE),
        _ => None,
    }
}

pub const fn slot_for_event(event: i32, player_view: bool) -> Option<usize> {
    if event == 0x33 {
        return Some(MELEE_HIT);
    }
    if event == 0x34 {
        return Some(MELEE_MISS);
    }
    match pair_for_event(event) {
        Some(pair) if player_view => Some(pair.player),
        Some(pair) => Some(pair.world),
        None => None,
    }
}

pub fn pm_begin_reload_event(facts: &crate::pm_weapon::WeaponCombatFacts, clip: i32) -> i32 {
    if facts.segmented_reload {
        EV_RELOAD_START
    } else {
        pm_reload_insert_event(facts, clip)
    }
}

pub fn pm_reload_insert_event(facts: &crate::pm_weapon::WeaponCombatFacts, clip: i32) -> i32 {
    if facts.weap_type == 0 && clip <= 0 {
        EV_RELOAD_FROM_EMPTY
    } else {
        EV_RELOAD
    }
}

pub fn play_note_mapped_sound_alias<'a, K: AsRef<str>, V: AsRef<str>>(
    note: &str,
    map: &'a [(K, V)],
) -> Option<&'a str> {
    if note.eq_ignore_ascii_case("end") || map.is_empty() {
        return None;
    }
    map.iter()
        .find(|(key, _)| key.as_ref().eq_ignore_ascii_case(note))
        .map(|(_, value)| value.as_ref())
}

pub fn play_note_mapped_rumble_alias<'a, K: AsRef<str>, V: AsRef<str>>(
    note: &str,
    map: &'a [(K, V)],
) -> Option<&'a str> {
    play_note_mapped_sound_alias(note, map)
}
