use asset_game::*;
pub const POLICY: TeamFamily = TeamFamily {
    bind_objectives: true,
    allow_absent_flag_base_fx: true,
    icon_alias_suffixes: &["_fade"],
    announcer: Some(super::announcer::route),
    objectives: Some(|settings| {
        objective_visuals(
            settings.allies_charset.as_deref(),
            settings.axis_charset.as_deref(),
        )
    }),
    sources: None,
};
pub fn objective_visuals(allies: Option<&str>, axis: Option<&str>) -> ObjectiveVisuals {
    let flag = |faction| match faction {
        Some("seals") => Some("mp_flag_allies_1"),
        Some("fbi") => Some("mp_flag_allies_2"),
        Some("isa") => Some("mp_flag_allies_3"),
        Some("pla") => Some("mp_flag_axis_1"),
        Some("pmc") => Some("mp_flag_axis_2"),
        Some("cd") => Some("mp_flag_axis_3"),
        _ => None,
    };
    let flags = [flag(allies), flag(axis)];
    ObjectiveVisuals {
        neutral_flag: Some("mp_flag_neutral".into()),
        bomb: Some("prop_suitcase_bomb".into()),
        bomb_explosion_fx: Some("maps/mp_maps/fx_mp_exp_bomb".into()),
        plant_weapon: Some("briefcase_bomb_mp".into()),
        defuse_weapon: Some("briefcase_bomb_defuse_mp".into()),
        flag: flags.map(|name| name.map(str::to_owned)),
        flag_carry: flags.map(|name| name.map(|name| format!("{name}_carry"))),
        crate_model: [
            Some("t6_wpn_supply_drop_ally".into()),
            Some("t6_wpn_supply_drop_axis".into()),
        ],
        crate_overlay: [
            Some("t6_wpn_supply_drop_ally".into()),
            Some("t6_wpn_supply_drop_axis".into()),
        ],
        ..Default::default()
    }
}

use std::collections::BTreeMap;

pub fn t6_team_properties(bytes: &[u8]) -> Option<BTreeMap<String, String>> {
    if bytes.get(..8)? != b"\x80GSC\r\n\0\x06" {
        return None;
    }
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let half = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let mut at = word(24)? as usize;
    let mut uses = Vec::new();
    for _ in 0..half(50)? {
        let offset = half(at)? as usize;
        let count = *bytes.get(at + 2)?;
        let kind = *bytes.get(at + 3)?;
        let tail = bytes.get(offset..)?;
        let value = std::str::from_utf8(tail.get(..tail.iter().position(|&b| b == 0)?)?).ok()?;
        at += 4;
        for _ in 0..count {
            let address = word(at)? as usize;
            at += 4;
            if kind == 0 {
                uses.push((address, value));
            }
        }
    }
    uses.sort_unstable_by_key(|u| u.0);
    let mut values = BTreeMap::new();
    for pair in uses.windows(2) {
        let [(value_at, value), (key_at, key)] = pair else {
            unreachable!()
        };
        if key_at - value_at > 12 {
            continue;
        }
        let tail = bytes.get(key_at + 2..)?;
        if tail.starts_with(&[0x14, 0x1c, 0x1c, 0x28])
            || (key.starts_with("g_") && tail.starts_with(&[0x19, 0, 0x4e, 0x2e]))
        {
            values.insert((*key).to_owned(), (*value).to_owned());
        }
    }
    for triple in uses.windows(3) {
        let [(value_at, value), (_, key), (category_at, category)] = triple else {
            unreachable!()
        };
        if category_at - value_at <= 20
            && matches!(*category, "strings" | "music")
            && bytes
                .get(category_at + 2..)?
                .starts_with(&[0x14, 0x1c, 0x1c, 0x28])
        {
            values.insert(format!("{category}/{key}"), (*value).to_owned());
        }
    }
    for &(address, key) in &uses {
        if matches!(key, "attackers" | "defenders")
            && bytes.get(address + 2..)?.starts_with(&[0x14, 0x1c, 0x28])
        {
            values.insert(key.to_owned(), String::new());
        }
    }
    Some(values)
}
