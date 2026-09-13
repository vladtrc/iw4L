pub const SCAVENGER_ALTMODE_DEFAULT: bool = true;
pub const SCAVENGER_SECONDARY_DEFAULT: bool = true;

pub const MAX_PER_PLAYER_EXPLOSIVES_DEFAULT: i32 = 2;

pub const RIOT_SHIELD_XP_BULLETS_DEFAULT: i32 = 15;

pub const WEAPON_LIST_STAT_MAX: i32 = 149;

pub const EXTRA_PRECACHE_ITEMS: &[&str] = &[
    "flare_mp",
    "scavenger_bag_mp",
    "frag_grenade_short_mp",
    "destructible_car",
];

pub const CLAYMORE_DETECTION_CONE_ANGLE: f32 = 70.0;

pub const CLAYMORE_DETECTION_MIN_DIST: f32 = 20.0;

pub const CLAYMORE_DETECTION_GRACE: f32 = 0.75;

pub const CLAYMORE_DETONATE_RADIUS: f32 = 192.0;

pub const STINGER_FX: &str = "explosions/aerial_explosion_large";

pub fn claymore_detection_dot() -> f32 {
    libm::cosf(CLAYMORE_DETECTION_CONE_ANGLE * core::f32::consts::PI / 180.0)
}

pub fn max_per_player_explosives(dvar: i32) -> i32 {
    if dvar > 1 { dvar } else { 1 }
}

pub fn scavenger_mode_flags(perk_scavenger_mode: i32) -> (bool, bool) {
    match perk_scavenger_mode {
        1 => (false, true),
        2 => (true, false),
        3 => (false, false),
        _ => (true, true),
    }
}

pub fn stats_row_is_weapon(name: &str, group: &str) -> bool {
    !name.is_empty() && group.contains("weapon_")
}

pub const STATS_TABLE_DUMP_SHA16: &str = "841fde1ee1493814";
pub const STATS_WEAPON_ROWS: &[(i32, &str, &str, &[&str])] = &[
    (0, "weapon_riot", "riotshield", &[]),
    (
        1,
        "weapon_pistol",
        "beretta",
        &["fmj", "silencer", "akimbo", "tactical", "xmags"],
    ),
    (
        2,
        "weapon_pistol",
        "usp",
        &["fmj", "silencer", "akimbo", "tactical", "xmags"],
    ),
    (
        3,
        "weapon_pistol",
        "deserteagle",
        &["fmj", "akimbo", "tactical"],
    ),
    (
        4,
        "weapon_pistol",
        "coltanaconda",
        &["fmj", "akimbo", "tactical"],
    ),
    (5, "weapon_pistol", "deserteaglegold", &[]),
    (
        7,
        "weapon_machine_pistol",
        "glock",
        &["reflex", "silencer", "fmj", "akimbo", "eotech", "xmags"],
    ),
    (
        8,
        "weapon_machine_pistol",
        "beretta393",
        &["reflex", "silencer", "fmj", "akimbo", "eotech", "xmags"],
    ),
    (
        9,
        "weapon_machine_pistol",
        "pp2000",
        &["reflex", "silencer", "fmj", "akimbo", "eotech", "xmags"],
    ),
    (
        10,
        "weapon_machine_pistol",
        "tmp",
        &["reflex", "silencer", "fmj", "akimbo", "eotech", "xmags"],
    ),
    (
        12,
        "weapon_smg",
        "mp5k",
        &[
            "rof", "reflex", "silencer", "acog", "fmj", "akimbo", "eotech", "thermal", "xmags",
        ],
    ),
    (
        13,
        "weapon_smg",
        "uzi",
        &[
            "rof", "reflex", "silencer", "acog", "fmj", "akimbo", "eotech", "thermal", "xmags",
        ],
    ),
    (
        14,
        "weapon_smg",
        "p90",
        &[
            "rof", "reflex", "silencer", "acog", "fmj", "akimbo", "eotech", "thermal", "xmags",
        ],
    ),
    (
        15,
        "weapon_smg",
        "kriss",
        &[
            "rof", "reflex", "silencer", "acog", "fmj", "akimbo", "eotech", "thermal", "xmags",
        ],
    ),
    (
        16,
        "weapon_smg",
        "ump45",
        &[
            "rof", "reflex", "silencer", "acog", "fmj", "akimbo", "eotech", "thermal", "xmags",
        ],
    ),
    (
        18,
        "weapon_assault",
        "ak47",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        19,
        "weapon_assault",
        "m16",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        20,
        "weapon_assault",
        "m4",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        21,
        "weapon_assault",
        "fn2000",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        22,
        "weapon_assault",
        "masada",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        23,
        "weapon_assault",
        "famas",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        24,
        "weapon_assault",
        "fal",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        25,
        "weapon_assault",
        "scar",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        26,
        "weapon_assault",
        "tavor",
        &[
            "gl",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "shotgun",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (28, "weapon_projectile", "gl", &[]),
    (29, "weapon_projectile", "m79", &[]),
    (30, "weapon_projectile", "rpg", &[]),
    (31, "weapon_projectile", "at4", &[]),
    (32, "weapon_projectile", "stinger", &[]),
    (33, "weapon_projectile", "javelin", &[]),
    (
        35,
        "weapon_sniper",
        "barrett",
        &["silencer", "acog", "fmj", "heartbeat", "thermal", "xmags"],
    ),
    (
        36,
        "weapon_sniper",
        "wa2000",
        &["silencer", "acog", "fmj", "heartbeat", "thermal", "xmags"],
    ),
    (
        37,
        "weapon_sniper",
        "m21",
        &["silencer", "acog", "fmj", "heartbeat", "thermal", "xmags"],
    ),
    (
        38,
        "weapon_sniper",
        "cheytac",
        &["silencer", "acog", "fmj", "heartbeat", "thermal", "xmags"],
    ),
    (40, "weapon_shotgun", "ranger", &["akimbo", "fmj"]),
    (41, "weapon_shotgun", "model1887", &["akimbo", "fmj"]),
    (
        42,
        "weapon_shotgun",
        "striker",
        &["reflex", "silencer", "grip", "fmj", "eotech", "xmags"],
    ),
    (
        43,
        "weapon_shotgun",
        "aa12",
        &["reflex", "silencer", "grip", "fmj", "eotech", "xmags"],
    ),
    (
        44,
        "weapon_shotgun",
        "m1014",
        &["reflex", "silencer", "grip", "fmj", "eotech", "xmags"],
    ),
    (
        45,
        "weapon_shotgun",
        "spas12",
        &["reflex", "silencer", "grip", "fmj", "eotech", "xmags"],
    ),
    (
        47,
        "weapon_lmg",
        "rpd",
        &[
            "grip",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        48,
        "weapon_lmg",
        "sa80",
        &[
            "grip",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        49,
        "weapon_lmg",
        "mg4",
        &[
            "grip",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        50,
        "weapon_lmg",
        "m240",
        &[
            "grip",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (
        51,
        "weapon_lmg",
        "aug",
        &[
            "grip",
            "reflex",
            "silencer",
            "acog",
            "fmj",
            "eotech",
            "heartbeat",
            "thermal",
            "xmags",
        ],
    ),
    (53, "weapon_explosive", "c4", &[]),
    (54, "weapon_explosive", "claymore", &[]),
    (56, "weapon_explosive", "airdrop_marker", &[]),
    (57, "weapon_explosive", "semtex", &[]),
    (60, "weapon_grenade", "frag_grenade", &[]),
    (61, "weapon_grenade", "flash_grenade", &[]),
    (62, "weapon_grenade", "smoke_grenade", &[]),
    (63, "weapon_grenade", "concussion_grenade", &[]),
    (64, "weapon_grenade", "throwingknife", &[]),
    (65, "weapon_other", "onemanarmy", &[]),
];

pub fn stats_weapon_group(stat_id: i32) -> Option<&'static str> {
    STATS_WEAPON_ROWS
        .iter()
        .find(|(id, _, _, _)| *id == stat_id)
        .map(|(_, g, _, _)| *g)
}

pub fn is_valid_weapon_listed(ref_string: &str) -> Option<bool> {
    let rest = ref_string.strip_suffix("_mp")?;
    for &(_, group, name, atts) in STATS_WEAPON_ROWS {
        if !stats_row_is_weapon(name, group) {
            continue;
        }
        if rest == name {
            return Some(true);
        }
        let prefix_len = name.len();
        if rest.len() > prefix_len + 1
            && rest.as_bytes().get(prefix_len) == Some(&b'_')
            && rest.starts_with(name)
        {
            let att = &rest[prefix_len + 1..];
            if att.contains('_') {
                return None;
            }
            return Some(atts.contains(&att));
        }
    }
    Some(false)
}
