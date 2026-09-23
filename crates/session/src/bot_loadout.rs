use std::collections::{HashMap, HashSet};

use assets::{AssetNamespace, FamilySlot, WeaponRegistry};
use bots::unique_loadout::{UNIQUE_WEAPONS, UniqueGroup, family_stem, pair_unique_loadouts};
use sim::ClassId;

const PER_GROUP: usize = 3;

const BOT_PERKS: [[&str; 3]; 3] = [
    [
        "specialty_marathon",
        "specialty_fastreload",
        "specialty_scavenger",
    ],
    [
        "specialty_bulletdamage",
        "specialty_lightweight",
        "specialty_coldblooded",
    ],
    [
        "specialty_bulletaccuracy",
        "specialty_heartbreaker",
        "specialty_localjammer",
    ],
];

fn bot_perks(offset: usize) -> [String; 3] {
    std::array::from_fn(|tier| BOT_PERKS[tier][(offset + tier) % BOT_PERKS[tier].len()].to_owned())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UniqueLoadoutProjection {
    pub primaries: Vec<u32>,
    pub secondaries: Vec<u32>,
    pub classes: Vec<sim::ClassDef>,
}

impl UniqueLoadoutProjection {
    #[must_use]
    pub fn class_ids(&self) -> Vec<ClassId> {
        self.classes.iter().map(|row| row.id).collect()
    }
}

pub fn project_unique_bot_classes(
    first_id: u32,
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    equipment: &[sim::EquipmentRuntimeFacts],
) -> UniqueLoadoutProjection {
    let rows = gun_rows(weapons);
    let kind_by_id: HashMap<u32, FamilySlot> = rows.iter().map(|row| (row.id, row.slot)).collect();

    let mut picked: HashSet<u32> = HashSet::new();
    let mut by_ns_group: HashMap<(AssetNamespace, UniqueGroup), Vec<u32>> = HashMap::new();

    for row in UNIQUE_WEAPONS {
        let Some(id) = usable_gun_id(weapons, combat, row.key) else {
            continue;
        };
        if !picked.insert(id) {
            continue;
        }
        let Some(ns) = weapons.namespace_of(id) else {
            continue;
        };
        if !kind_by_id.contains_key(&id) {
            continue;
        }
        let bucket = by_ns_group.entry((ns, row.group)).or_default();
        if bucket.len() >= PER_GROUP {
            continue;
        }
        bucket.push(id);
    }

    let unique_stems = unique_family_stems(weapons, &rows);
    let mut fill_candidates: Vec<(AssetNamespace, UniqueGroup, u32, String)> = Vec::new();
    for row in &rows {
        if picked.contains(&row.id) {
            continue;
        }
        if !combat
            .get(row.id as usize)
            .copied()
            .is_some_and(sim::WeaponCombatFacts::is_usable)
        {
            continue;
        }
        let Some(ns) = weapons.namespace_of(row.id) else {
            continue;
        };
        let Some(group) = unique_group_from_item_group(&row.item_group) else {
            continue;
        };
        let stem = family_stem(&row.name);
        if unique_stems.get(&stem) != Some(&ns) {
            continue;
        }
        fill_candidates.push((ns, group, row.id, row.name.clone()));
    }
    fill_candidates.sort_by(|a, b| a.3.cmp(&b.3).then(a.2.cmp(&b.2)));
    for (ns, group, id, _) in fill_candidates {
        let bucket = by_ns_group.entry((ns, group)).or_default();
        if bucket.len() >= PER_GROUP {
            continue;
        }
        if picked.insert(id) {
            bucket.push(id);
        }
    }

    let mut primaries = Vec::new();
    let mut secondaries = Vec::new();
    let mut seen_p = HashSet::new();
    let mut seen_s = HashSet::new();
    let mut buckets: Vec<((AssetNamespace, UniqueGroup), Vec<u32>)> =
        by_ns_group.into_iter().collect();
    buckets.sort_by_key(|(key, _)| (namespace_ord(key.0), key.1));
    for (_, ids) in buckets {
        for id in ids {
            match kind_by_id.get(&id) {
                Some(FamilySlot::Primary) => {
                    if seen_p.insert(id) {
                        primaries.push(id);
                    }
                }
                Some(FamilySlot::Secondary) => {
                    if seen_s.insert(id) {
                        secondaries.push(id);
                    }
                }
                _ => {}
            }
        }
    }

    let pairs = pair_unique_loadouts(&primaries, &secondaries);
    let mut classes = Vec::with_capacity(pairs.len());
    for (offset, (primary, secondary)) in pairs.into_iter().enumerate() {
        let class_id = first_id + offset as u32;
        let primary_key = weapons.namespaced_key_of(primary).unwrap_or_default();
        let secondary_key = weapons.namespaced_key_of(secondary).unwrap_or_default();
        let row = crate::ClassRow {
            weapons: [primary_key, secondary_key, String::new(), String::new()],
            perks: bot_perks(offset),
            ..Default::default()
        };
        let projected = crate::project_class(class_id, &row, weapons, combat, equipment);
        if projected.def.locked {
            continue;
        }
        classes.push(projected.def);
    }
    UniqueLoadoutProjection {
        primaries,
        secondaries,
        classes,
    }
}

fn usable_gun_id(
    weapons: &WeaponRegistry,
    combat: &[sim::WeaponCombatFacts],
    key: &str,
) -> Option<u32> {
    let id = weapons.resolve_index(key).ok().flatten()?;
    if id == 0 {
        return None;
    }
    combat
        .get(id as usize)
        .copied()
        .is_some_and(sim::WeaponCombatFacts::is_usable)
        .then_some(id)
}

struct GunRow {
    id: u32,
    name: String,
    item_group: String,
    slot: FamilySlot,
}

fn gun_rows(weapons: &WeaponRegistry) -> Vec<GunRow> {
    weapons
        .weapon_families()
        .offered()
        .filter(|family| matches!(family.slot, FamilySlot::Primary | FamilySlot::Secondary))
        .filter_map(|family| {
            let id = family.base?;
            Some(GunRow {
                id,
                name: weapons.name_of(id).to_owned(),
                item_group: family.item_group.clone(),
                slot: family.slot,
            })
        })
        .collect()
}

fn unique_family_stems(
    weapons: &WeaponRegistry,
    rows: &[GunRow],
) -> HashMap<String, AssetNamespace> {
    let mut ns_by_stem: HashMap<String, HashSet<AssetNamespace>> = HashMap::new();
    for row in rows {
        let Some(ns) = weapons.namespace_of(row.id) else {
            continue;
        };
        ns_by_stem
            .entry(family_stem(&row.name))
            .or_default()
            .insert(ns);
    }
    ns_by_stem
        .into_iter()
        .filter_map(|(stem, namespaces)| {
            if namespaces.len() == 1 {
                namespaces.into_iter().next().map(|ns| (stem, ns))
            } else {
                None
            }
        })
        .collect()
}

fn unique_group_from_item_group(group: &str) -> Option<UniqueGroup> {
    match group {
        "weapon_assault" => Some(UniqueGroup::Assault),
        "weapon_smg" => Some(UniqueGroup::Smg),
        "weapon_lmg" => Some(UniqueGroup::Lmg),
        "weapon_sniper" => Some(UniqueGroup::Sniper),
        "weapon_riot" => Some(UniqueGroup::Riot),
        "weapon_machine_pistol" => Some(UniqueGroup::MachinePistol),
        "weapon_shotgun" => Some(UniqueGroup::Shotgun),
        "weapon_pistol" => Some(UniqueGroup::Pistol),
        "weapon_projectile" | "weapon_launcher" => Some(UniqueGroup::Projectile),
        "weapon_cqb" => Some(UniqueGroup::Cqb),
        "weapon_special" => Some(UniqueGroup::Special),
        _ => None,
    }
}

fn namespace_ord(ns: AssetNamespace) -> u8 {
    match ns {
        AssetNamespace::Iw4 => 0,
        AssetNamespace::T5 => 1,
        AssetNamespace::Iw5 => 2,
    }
}
