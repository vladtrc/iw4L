//! Which IW4 weapon a T6 weapon borrows its models, animations, sounds and
//! icons from while T6 content is not loaded.
//!
//! A T6 weapon without a stand-in is not captured: killstreak guns, turrets
//! and scripted items have nothing a player could carry here.

/// T6 name → IW4 stand-in, for the base names (variants are reduced to
/// these first, see [`base_name`]).
const STAND_INS: &[(&str, &str)] = &[
    // Assault rifles.
    ("an94", "ak47_mp"),
    ("hk416", "m4_mp"),
    ("scar", "scar_mp"),
    ("sig556", "m16_mp"),
    ("saritch", "fal_mp"),
    ("sa58", "fal_mp"),
    ("type95", "famas_mp"),
    ("tar21", "tavor_mp"),
    ("xm8", "fn2000_mp"),
    // Submachine guns.
    ("mp7", "mp5k_mp"),
    ("vector", "kriss_mp"),
    ("qcw05", "uzi_mp"),
    ("pdw57", "p90_mp"),
    ("evoskorpion", "tmp_mp"),
    ("insas", "ump45_mp"),
    // Light machine guns.
    ("qbb95", "rpd_mp"),
    ("lsat", "mg4_mp"),
    ("mk48", "m240_mp"),
    ("hamr", "sa80_mp"),
    // Sniper rifles.
    ("as50", "barrett_mp"),
    ("dsr50", "cheytac_mp"),
    ("svu", "m21_mp"),
    ("ballista", "cheytac_mp"),
    // Pistols.
    ("fnp45", "usp_mp"),
    ("fiveseven", "beretta_mp"),
    ("kard", "deserteagle_mp"),
    ("judge", "coltanaconda_mp"),
    ("beretta93r", "beretta393_mp"),
    // Shotguns.
    ("saiga12", "aa12_mp"),
    ("870mcs", "spas12_mp"),
    ("srm1216", "striker_mp"),
    ("ksg", "m1014_mp"),
    // Launchers and specials.
    ("smaw", "at4_mp"),
    ("usrpg", "rpg_mp"),
    ("fhj18", "stinger_mp"),
    ("riotshield", "riotshield_mp"),
    ("crossbow", "m79_mp"),
    ("knife_ballistic", "throwingknife_mp"),
    // Lethal equipment.
    ("frag_grenade", "frag_grenade_mp"),
    ("sticky_grenade", "semtex_mp"),
    ("hatchet", "throwingknife_mp"),
    ("satchel_charge", "c4_mp"),
    ("claymore", "claymore_mp"),
    ("bouncingbetty", "claymore_mp"),
    // Tactical equipment.
    ("flash_grenade", "flash_grenade_mp"),
    ("concussion_grenade", "concussion_grenade_mp"),
    ("willy_pete", "smoke_grenade_mp"),
    ("emp_grenade", "concussion_grenade_mp"),
    ("proximity_grenade", "concussion_grenade_mp"),
    ("sensor_grenade", "smoke_grenade_mp"),
    ("trophy_system", "claymore_mp"),
    ("tactical_insertion", "flare_mp"),
];

/// T6 tactical equipment whose stand-in is not a tactical grenade in IW4
/// (`flare_mp` is IW4's tactical insertion, equipment there).
const TACTICAL_EQUIPMENT: &[&str] = &["tactical_insertion"];

/// Whether a T6 weapon is tactical equipment the IW4 stand-in's offhand
/// class would file as lethal.
pub fn is_tactical_equipment(name: &str) -> bool {
    TACTICAL_EQUIPMENT.contains(&base_name(name))
}

/// T6 equipment standing in for a claymore without its laser.
const SHEDS_STAND_IN_TRAIL: &[&str] = &["bouncingbetty", "trophy_system"];

/// Whether a T6 weapon drops its stand-in's projectile trail and beacon.
pub fn sheds_stand_in_trail(name: &str) -> bool {
    SHEDS_STAND_IN_TRAIL.contains(&base_name(name))
}

/// T6 equipment that is thrown like a grenade but stays where it lands.
const STAYS_PLANTED: &[&str] = &["sensor_grenade"];

/// Whether a T6 weapon is thrown like a grenade but stays where it lands.
pub fn stays_planted(name: &str) -> bool {
    STAYS_PLANTED.contains(&base_name(name))
}

/// T6 equipment whose scripts plant a model of their own, which its weapon
/// does not name: it stands for the thrown one.
const PLANTED_MODELS: &[(&str, &str)] = &[("tactical_insertion", "t6_wpn_tac_insert_world")];

/// The model a T6 weapon's scripts plant it as, when its weapon does not
/// name one.
pub fn planted_model(name: &str) -> Option<&'static str> {
    let base = base_name(name);
    PLANTED_MODELS
        .iter()
        .find(|(t6, _)| *t6 == base)
        .map(|(_, model)| *model)
}

/// The T6 effects IW4 matches play: T6 equipment's (lights in team colour
/// and enemy colour, bursts, flashes), played by the engine and by
/// `iw4l_t6/equipment`.
pub const T6_EFFECTS: &[&str] = &[
    "misc/fx_equip_tac_insert_light_grn",
    "misc/fx_equip_tac_insert_light_red",
    "misc/fx_equip_tac_insert_exp",
    "weapon/bouncing_betty/fx_betty_explosion",
    "weapon/bouncing_betty/fx_betty_destroyed",
    "weapon/bouncing_betty/fx_betty_launch_dust",
    "weapon/bouncing_betty/fx_betty_light_green",
    "weapon/bouncing_betty/fx_betty_light_red",
    "weapon/trophy_system/fx_trophy_flash_lng",
    "weapon/trophy_system/fx_trophy_radius_detonation",
    "weapon/trophy_system/fx_trophy_light_friendly",
    "weapon/trophy_system/fx_trophy_light_enemy",
    "weapon/trophy_system/fx_trophy_deploy_impact",
    "weapon/grenade/fx_prox_grenade_scan_grn",
    "weapon/grenade/fx_prox_grenade_scan_red",
    "weapon/grenade/fx_prox_grenade_wrn_grn",
    "weapon/grenade/fx_prox_grenade_wrn_red",
    "weapon/grenade/fx_prox_grenade_impact_player_spwner",
    "weapon/grenade/fx_spark_disabled_weapon",
    "weapon/c4/fx_c4_light_green",
    "weapon/c4/fx_c4_light_red",
    "weapon/emp/fx_emp_explosion_equip",
    "explosions/fx_exp_equipment",
    "explosions/fx_exp_equipment_lg",
    // T6 weapons' own explosion effects (`projExplosionEffect`).
    "explosions/fx_flashbang",
    "weapon/grenade/fx_prox_grenade_exp",
    "weapon/satchel/fx_explosion_satchel_generic",
    "weapon/sensor_grenade/fx_sensor_exp_scan_friendly",
    "weapon/sensor_grenade/fx_sensor_exp_scan_enemy",
];

/// The T6 sound aliases `iw4l_t6/equipment` plays (no weapon names them).
pub const T6_EQUIPMENT_SOUNDS: &[&str] = &[
    "wpn_claymore_alert",
    "fly_betty_jump",
    "fly_betty_explo",
    "dst_equipment_destroy",
    "dst_disable_spark",
    "dst_tac_insert_break",
    "wpn_taser_mine_zap",
    "wpn_taser_mine_tacmask",
    "wpn_trophy_alert",
    "wpn_trophy_spin",
    "fly_sensor_nade_lp",
];

/// The T6 weapon whose knife and swings every gun's melee borrows when
/// the gun has no melee clip of its own (T6 rifles and snipers do not).
pub const MELEE_WEAPON: &str = "knife_mp";

/// `sf_an94_mp`, `dualoptic_an94_mp`, `gl_an94_mp`, `fnp45_dw_mp` and
/// `fnp45_lh_mp` are all `an94` / `fnp45` underneath.
pub fn base_name(name: &str) -> &str {
    let mut base = name.strip_suffix("_mp").unwrap_or(name);
    for prefix in ["sf_", "dualoptic_", "gl_"] {
        if let Some(rest) = base.strip_prefix(prefix) {
            base = rest;
        }
    }
    for suffix in ["_dw", "_lh"] {
        if let Some(rest) = base.strip_suffix(suffix) {
            base = rest;
        }
    }
    base
}

/// The IW4 weapon a T6 weapon borrows its looks from, or `None` when the
/// weapon has no stand-in and is left out.
pub fn stand_in_for(name: &str) -> Option<&'static str> {
    let base = base_name(name);
    STAND_INS
        .iter()
        .find(|(t6, _)| *t6 == base)
        .map(|(_, iw4)| *iw4)
}

/// A T6 `StringTable` asset of a finished load (`name`, `columnCount`,
/// `rowCount`, then `rowCount × columnCount` cells of `{ char* string, int hash }`).
pub fn capture_t6_string_table(
    load: &fastfile_t6::ZoneLoad,
    asset: &fastfile_t6::LoadedAsset,
) -> Option<crate::CapturedStringTable> {
    if asset.ty != fastfile_t6::AssetType::StringTable {
        return None;
    }
    let h = &asset.header;
    let word = |at: usize| Some(u32::from_le_bytes(h.get(at..at + 4)?.try_into().ok()?));
    let ptr = |raw: u32| {
        (raw != 0 && raw < 0xFFFF_FFFE).then(|| fastfile_t6::Ptr {
            block: ((raw - 1) >> 29) as u8,
            offset: (raw - 1) & 0x1FFF_FFFF,
        })
    };
    let text = |p: Option<fastfile_t6::Ptr>| {
        p.and_then(|p| load.blocks.cstr(p).ok())
            .map(|b| String::from_utf8_lossy(b).into_owned())
    };
    let name = text(ptr(word(0)?))?;
    let columns = usize::try_from(word(4)? as i32).ok()?;
    let rows = usize::try_from(word(8)? as i32).ok()?;
    let values = ptr(word(12)?);
    let mut cells = Vec::with_capacity(rows * columns);
    for i in 0..rows * columns {
        let cell = values.map(|v| v.at(8 * i as u32));
        let string = cell
            .and_then(|c| load.blocks.ptr_at(c).ok().flatten())
            .and_then(|p| load.blocks.cstr(p).ok())
            .map_or_else(String::new, |b| String::from_utf8_lossy(b).into_owned());
        cells.push(string);
    }
    Some(crate::CapturedStringTable {
        name,
        columns,
        rows,
        cells,
    })
}
