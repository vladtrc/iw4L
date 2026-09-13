pub const CLASS_MAP_STOCK: &[(&str, i32)] = &[
    ("class0", 0),
    ("class1", 1),
    ("class2", 2),
    ("class3", 3),
    ("class4", 4),
    ("class5", 5),
    ("class6", 6),
    ("class7", 7),
    ("class8", 8),
    ("class9", 9),
    ("class10", 10),
    ("class11", 11),
    ("class12", 12),
    ("class13", 13),
    ("class14", 14),
];

pub const CLASS_MAP_CUSTOM: &[(&str, i32)] = &[
    ("custom1", 0),
    ("custom2", 1),
    ("custom3", 2),
    ("custom4", 3),
    ("custom5", 4),
    ("custom6", 5),
    ("custom7", 6),
    ("custom8", 7),
    ("custom9", 8),
    ("custom10", 9),
];

pub const CLASS_MAP_COPYCAT: i32 = -1;

pub const DEFAULT_CLASS: &str = "CLASS_ASSAULT";

pub const CLASS_TABLE_NAME: &str = "mp/classTable.csv";

pub const CLASS_TABLE_DUMP_SHA16: &str = "5d4188bc2889b828";
pub const CLASS_TABLE_VALUE_COLS: i32 = 11;

pub const CLASS_TABLE_ROWS: &[(&str, &[&str])] = &[
    (
        "loadoutName",
        &[
            "CLASS_CLASS1",
            "CLASS_CLASS2",
            "CLASS_CLASS3",
            "CLASS_CLASS4",
            "CLASS_CLASS5",
            "CLASS_CLASS1",
            "CLASS_CLASS2",
            "CLASS_CLASS3",
            "CLASS_CLASS4",
            "CLASS_CLASS5",
            "CLASS_DEFAULT",
        ],
    ),
    (
        "loadoutPrimary",
        &[
            "famas",
            "ump45",
            "sa80",
            "barrett",
            "riotshield",
            "m4",
            "mp5k",
            "rpd",
            "cheytac",
            "riotshield",
            "m4",
        ],
    ),
    (
        "loadoutPrimaryAttachment",
        &[
            "gl",
            "eotech",
            "reflex",
            "heartbeat",
            "none",
            "none",
            "none",
            "none",
            "none",
            "none",
            "none",
        ],
    ),
    (
        "loadoutPrimaryAttachment2",
        &[
            "none", "none", "grip", "fmj", "none", "none", "none", "none", "none", "none", "none",
        ],
    ),
    (
        "loadoutPrimaryCamo",
        &[
            "none", "none", "none", "none", "none", "none", "none", "none", "none", "none", "none",
        ],
    ),
    (
        "loadoutSecondary",
        &[
            "spas12",
            "coltanaconda",
            "at4",
            "usp",
            "pp2000",
            "usp",
            "spas12",
            "at4",
            "pp2000",
            "pp2000",
            "usp",
        ],
    ),
    (
        "loadoutSecondaryAttachment",
        &[
            "silencer", "tactical", "none", "silencer", "akimbo", "none", "none", "none", "none",
            "none", "none",
        ],
    ),
    (
        "loadoutSecondaryAttachment2",
        &[
            "none", "none", "none", "none", "none", "none", "none", "none", "none", "none", "none",
        ],
    ),
    (
        "loadoutSecondaryCamo",
        &[
            "none", "none", "none", "none", "none", "none", "none", "none", "none", "none", "none",
        ],
    ),
    (
        "loadoutEquipment",
        &[
            "frag_grenade_mp",
            "semtex_mp",
            "semtex_mp",
            "specialty_tacticalinsertion",
            "specialty_blastshield",
            "frag_grenade_mp",
            "semtex_mp",
            "semtex_mp",
            "frag_grenade_mp",
            "frag_grenade_mp",
            "frag_grenade_mp",
        ],
    ),
    (
        "loadoutPerk1",
        &[
            "specialty_scavenger",
            "specialty_marathon",
            "specialty_bling",
            "specialty_bling",
            "specialty_marathon",
            "specialty_fastreload",
            "specialty_marathon",
            "specialty_fastreload",
            "specialty_fastreload",
            "specialty_marathon",
            "specialty_fastreload",
        ],
    ),
    (
        "loadoutPerk2",
        &[
            "specialty_bulletdamage",
            "specialty_lightweight",
            "specialty_explosivedamage",
            "specialty_coldblooded",
            "specialty_hardline",
            "specialty_bulletdamage",
            "specialty_lightweight",
            "specialty_bulletdamage",
            "specialty_bulletdamage",
            "specialty_lightweight",
            "specialty_bulletdamage",
        ],
    ),
    (
        "loadoutPerk3",
        &[
            "specialty_extendedmelee",
            "specialty_heartbreaker",
            "specialty_detectexplosive",
            "specialty_localjammer",
            "specialty_extendedmelee",
            "specialty_bulletaccuracy",
            "specialty_extendedmelee",
            "specialty_extendedmelee",
            "specialty_bulletaccuracy",
            "specialty_extendedmelee",
            "specialty_bulletaccuracy",
        ],
    ),
    (
        "loadoutOffHand",
        &[
            "concussion_grenade",
            "flash_grenade",
            "flash_grenade",
            "smoke_grenade",
            "concussion_grenade",
            "concussion_grenade",
            "flash_grenade",
            "flash_grenade",
            "smoke_grenade",
            "concussion_grenade",
            "concussion_grenade",
        ],
    ),
    (
        "loadoutDeathStreak",
        &[
            "specialty_copycat",
            "specialty_finalstand",
            "specialty_combathigh",
            "specialty_copycat",
            "specialty_combathigh",
            "specialty_copycat",
            "specialty_copycat",
            "specialty_copycat",
            "specialty_copycat",
            "specialty_copycat",
            "specialty_copycat",
        ],
    ),
];

pub fn class_table_csv_key(gsc_search: &str) -> &str {
    match gsc_search {
        "loadoutOffhand" => "loadoutOffHand",
        "loadoutDeathstreak" => "loadoutDeathStreak",
        other => other,
    }
}

pub fn class_table_cell(gsc_search: &str, return_col: i32) -> Option<&'static str> {
    if return_col < 1 || return_col > CLASS_TABLE_VALUE_COLS {
        return None;
    }
    let key = class_table_csv_key(gsc_search);
    let row = CLASS_TABLE_ROWS.iter().find(|(k, _)| *k == key)?;
    row.1.get((return_col - 1) as usize).copied()
}

pub const CAMO_TABLE_DUMP_SHA16: &str = "fc0805e01f31df25";
pub const CAMO_TABLE_ROWS: &[(i32, &str)] = &[
    (0, "none"),
    (1, "woodland"),
    (2, "desert"),
    (3, "arctic"),
    (4, "digital"),
    (5, "red_urban"),
    (6, "red_tiger"),
    (7, "blue_tiger"),
    (8, "orange_fall"),
    (9, "gold"),
    (10, "prestige"),
];

pub fn camo_table_id(camo_name: &str) -> Option<i32> {
    CAMO_TABLE_ROWS
        .iter()
        .find(|(_, n)| *n == camo_name)
        .map(|(id, _)| *id)
}

pub const CLASS_INIT_SHADERS: &[&str] = &["specialty_pistoldeath", "specialty_finalstand"];

pub const VALID_PRIMARY: &[&str] = &[
    "riotshield",
    "ak47",
    "m16",
    "m4",
    "fn2000",
    "masada",
    "famas",
    "fal",
    "scar",
    "tavor",
    "mp5k",
    "uzi",
    "p90",
    "kriss",
    "ump45",
    "barrett",
    "wa2000",
    "m21",
    "cheytac",
    "rpd",
    "sa80",
    "mg4",
    "m240",
    "aug",
];

pub const VALID_SECONDARY: &[&str] = &[
    "beretta",
    "usp",
    "deserteagle",
    "coltanaconda",
    "glock",
    "beretta393",
    "pp2000",
    "tmp",
    "m79",
    "rpg",
    "at4",
    "stinger",
    "javelin",
    "ranger",
    "model1887",
    "striker",
    "aa12",
    "m1014",
    "spas12",
    "onemanarmy",
];

pub const VALID_ATTACHMENT: &[&str] = &[
    "none",
    "acog",
    "reflex",
    "silencer",
    "grip",
    "gl",
    "akimbo",
    "thermal",
    "shotgun",
    "heartbeat",
    "fmj",
    "rof",
    "xmags",
    "eotech",
    "tactical",
];

pub const VALID_CAMO: &[&str] = &[
    "none",
    "woodland",
    "desert",
    "arctic",
    "digital",
    "red_urban",
    "red_tiger",
    "blue_tiger",
    "orange_fall",
];

pub const VALID_EQUIPMENT: &[&str] = &[
    "frag_grenade_mp",
    "semtex_mp",
    "throwingknife_mp",
    "specialty_tacticalinsertion",
    "specialty_blastshield",
    "claymore_mp",
    "c4_mp",
];

pub const VALID_OFFHAND: &[&str] = &["flash_grenade", "concussion_grenade", "smoke_grenade"];

pub const VALID_PERK1: &[&str] = &[
    "specialty_marathon",
    "specialty_fastreload",
    "specialty_scavenger",
    "specialty_bling",
    "specialty_onemanarmy",
];

pub const VALID_PERK2: &[&str] = &[
    "specialty_bulletdamage",
    "specialty_lightweight",
    "specialty_hardline",
    "specialty_coldblooded",
    "specialty_explosivedamage",
];

pub const VALID_PERK3: &[&str] = &[
    "specialty_extendedmelee",
    "specialty_bulletaccuracy",
    "specialty_localjammer",
    "specialty_heartbreaker",
    "specialty_detectexplosive",
    "specialty_pistoldeath",
];

pub const VALID_DEATHSTREAK: &[&str] = &[
    "specialty_copycat",
    "specialty_combathigh",
    "specialty_grenadepulldeath",
    "specialty_finalstand",
];

fn listed(hay: &[&str], needle: &str) -> bool {
    hay.iter().any(|s| *s == needle)
}

pub fn is_valid_primary(ref_string: &str) -> bool {
    listed(VALID_PRIMARY, ref_string)
}
pub fn is_valid_secondary(ref_string: &str) -> bool {
    listed(VALID_SECONDARY, ref_string)
}
pub fn is_valid_attachment(ref_string: &str) -> bool {
    listed(VALID_ATTACHMENT, ref_string)
}
pub fn is_valid_camo(ref_string: &str) -> bool {
    listed(VALID_CAMO, ref_string)
}
pub fn is_valid_equipment(ref_string: &str) -> bool {
    listed(VALID_EQUIPMENT, ref_string)
}
pub fn is_valid_offhand(ref_string: &str) -> bool {
    listed(VALID_OFFHAND, ref_string)
}
pub fn is_valid_perk1(ref_string: &str) -> bool {
    listed(VALID_PERK1, ref_string)
}
pub fn is_valid_perk2(ref_string: &str) -> bool {
    listed(VALID_PERK2, ref_string)
}
pub fn is_valid_perk3(ref_string: &str) -> bool {
    listed(VALID_PERK3, ref_string)
}
pub fn is_valid_deathstreak(ref_string: &str) -> bool {
    listed(VALID_DEATHSTREAK, ref_string)
}

pub fn is_valid_class(class: Option<&str>) -> bool {
    matches!(class, Some(s) if !s.is_empty())
}

pub fn get_value_in_range(value: i32, min_value: i32, max_value: i32) -> i32 {
    if value > max_value {
        max_value
    } else if value < min_value {
        min_value
    } else {
        value
    }
}

pub fn class_map_index(response: &str) -> Option<i32> {
    if response == "copycat" {
        return Some(CLASS_MAP_COPYCAT);
    }
    CLASS_MAP_STOCK
        .iter()
        .chain(CLASS_MAP_CUSTOM.iter())
        .find(|(k, _)| *k == response)
        .map(|(_, v)| *v)
}

pub fn get_class_choice(response: &str) -> Option<&str> {
    class_map_index(response).map(|_| response)
}

pub fn get_weapon_choice(response: &str) -> i32 {
    match response.split_once(',') {
        Some((_, rest)) => gsc_int(rest),
        None => 0,
    }
}

fn gsc_int(s: &str) -> i32 {
    let mut n = 0i32;
    for c in s.chars() {
        let Some(d) = c.to_digit(10) else {
            break;
        };
        n = n.saturating_mul(10).saturating_add(d as i32);
    }
    n
}

pub fn set_class(new_class: &str) -> &str {
    new_class
}

pub const LOADOUT_FALLBACK_CLASS: i32 = 10;

pub const CAMO_TABLE_NAME: &str = "mp/camoTable.csv";

pub const PERKS_OFF_OMA_WEAPON: &str = "beretta_mp";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassTableLookup {
    pub search_col: i32,
    pub search_value: &'static str,
    pub return_col: i32,
}

pub fn class_table_return_col(class_index: i32) -> i32 {
    class_index + 1
}

pub fn table_get_weapon(class_index: i32, weapon_index: i32) -> ClassTableLookup {
    ClassTableLookup {
        search_col: 0,
        search_value: if weapon_index == 0 {
            "loadoutPrimary"
        } else {
            "loadoutSecondary"
        },
        return_col: class_table_return_col(class_index),
    }
}

pub fn table_get_weapon_attachment(
    class_index: i32,
    weapon_index: i32,
    attachment_index: i32,
) -> ClassTableLookup {
    let search_value = match (weapon_index, attachment_index) {
        (0, 0) => "loadoutPrimaryAttachment",
        (0, _) => "loadoutPrimaryAttachment2",
        (_, 0) => "loadoutSecondaryAttachment",
        _ => "loadoutSecondaryAttachment2",
    };
    ClassTableLookup {
        search_col: 0,
        search_value,
        return_col: class_table_return_col(class_index),
    }
}

pub fn table_attachment_or_none(temp_name: &str) -> &str {
    if temp_name.is_empty() || temp_name == "none" {
        "none"
    } else {
        temp_name
    }
}

pub fn table_get_weapon_camo(class_index: i32, weapon_index: i32) -> ClassTableLookup {
    ClassTableLookup {
        search_col: 0,
        search_value: if weapon_index == 0 {
            "loadoutPrimaryCamo"
        } else {
            "loadoutSecondaryCamo"
        },
        return_col: class_table_return_col(class_index),
    }
}

pub fn table_get_equipment(class_index: i32) -> ClassTableLookup {
    ClassTableLookup {
        search_col: 0,
        search_value: "loadoutEquipment",
        return_col: class_table_return_col(class_index),
    }
}

pub fn table_get_perk(class_index: i32, perk_index: i32) -> Option<ClassTableLookup> {
    let search_value = match perk_index {
        0 => "loadoutPerk0",
        1 => "loadoutPerk1",
        2 => "loadoutPerk2",
        3 => "loadoutPerk3",
        4 => "loadoutPerk4",
        _ => return None,
    };
    Some(ClassTableLookup {
        search_col: 0,
        search_value,
        return_col: class_table_return_col(class_index),
    })
}

pub fn table_get_offhand(class_index: i32) -> ClassTableLookup {
    ClassTableLookup {
        search_col: 0,
        search_value: "loadoutOffhand",
        return_col: class_table_return_col(class_index),
    }
}

pub fn table_get_killstreak() -> &'static str {
    "none"
}

pub fn table_get_deathstreak(class_index: i32) -> ClassTableLookup {
    ClassTableLookup {
        search_col: 0,
        search_value: "loadoutDeathstreak",
        return_col: class_table_return_col(class_index),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadoutSource {
    Copycat,
    Custom,
    Stock,
}

pub fn give_loadout_source(
    copycat_in_use: bool,
    allow_copycat: Option<bool>,
    class: &str,
) -> LoadoutSource {
    let allow = allow_copycat.unwrap_or(true);
    if copycat_in_use && allow {
        LoadoutSource::Copycat
    } else if class.contains("custom") {
        LoadoutSource::Custom
    } else {
        LoadoutSource::Stock
    }
}

pub fn bling_clears_second_attachments(loadout_perk1: &str) -> bool {
    loadout_perk1 != "specialty_bling"
}

pub fn oma_replaces_secondary(loadout_perk1: &str, loadout_secondary: &str) -> bool {
    loadout_perk1 != "specialty_onemanarmy" && loadout_secondary == "onemanarmy"
}

pub const LOADOUT_SECONDARY_CAMO_FORCED: &str = "none";

pub fn letter_to_number(c: u8) -> Option<i32> {
    let c = c.to_ascii_lowercase();
    if c.is_ascii_lowercase() {
        Some(i32::from(c - b'a'))
    } else {
        None
    }
}

pub fn attachments_sorted<'a>(a: &'a str, b: &'a str) -> Option<(&'a str, &'a str)> {
    let a0 = letter_to_number(*a.as_bytes().first()?)?;
    let b0 = letter_to_number(*b.as_bytes().first()?)?;
    if a0 < b0 {
        return Some((a, b));
    }
    if a0 > b0 {
        return Some((b, a));
    }
    let a1 = letter_to_number(*a.as_bytes().get(1)?)?;
    let b1 = letter_to_number(*b.as_bytes().get(1)?)?;
    if a1 < b1 { Some((a, b)) } else { Some((b, a)) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltWeaponParts<'a> {
    pub base: &'a str,
    pub attachment0: Option<&'a str>,
    pub attachment1: Option<&'a str>,
}

pub fn build_weapon_parts<'a>(
    base_name: &'a str,
    attachment1: &'a str,
    attachment2: &'a str,
    perks_enabled: bool,
) -> Option<BuiltWeaponParts<'a>> {
    let mut att2 = attachment2;
    if !perks_enabled {
        att2 = "none";
        if base_name == "onemanarmy" {
            return Some(BuiltWeaponParts {
                base: PERKS_OFF_OMA_WEAPON,
                attachment0: None,
                attachment1: None,
            });
        }
    }
    if attachment1 != "none" && att2 != "none" {
        let (first, second) = attachments_sorted(attachment1, att2)?;
        Some(BuiltWeaponParts {
            base: base_name,
            attachment0: Some(first),
            attachment1: Some(second),
        })
    } else if attachment1 != "none" {
        Some(BuiltWeaponParts {
            base: base_name,
            attachment0: Some(attachment1),
            attachment1: None,
        })
    } else if att2 != "none" {
        Some(BuiltWeaponParts {
            base: base_name,
            attachment0: Some(att2),
            attachment1: None,
        })
    } else {
        Some(BuiltWeaponParts {
            base: base_name,
            attachment0: None,
            attachment1: None,
        })
    }
}
