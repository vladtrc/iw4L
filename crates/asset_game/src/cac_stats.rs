use crate::menu_catalog::CapturedStringTable;

pub const STATS_REF_COL: i32 = 4;

pub const STATS_GROUP_COL: i32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacAuthoredCategory {
    Assault,
    Smg,
    Lmg,
    Sniper,
    Riot,
    MachinePistol,
    Shotgun,
    Pistol,
    Projectile,
    Cqb,
    Special,
}

impl CacAuthoredCategory {
    #[must_use]
    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Assault,
            1 => Self::Smg,
            2 => Self::Lmg,
            3 => Self::Sniper,
            4 => Self::Riot,
            5 => Self::MachinePistol,
            6 => Self::Shotgun,
            7 => Self::Pistol,
            8 => Self::Projectile,
            9 => Self::Cqb,
            10 => Self::Special,
            _ => return None,
        })
    }

    #[must_use]
    pub fn menu_label(self) -> &'static str {
        match self {
            Self::Assault => "ASSAULT RIFLES",
            Self::Smg => "SUB MACHINE GUNS",
            Self::Lmg => "LIGHT MACHINE GUNS",
            Self::Sniper => "SNIPER RIFLES",
            Self::Riot => "RIOT SHIELD",
            Self::MachinePistol => "MACHINE PISTOLS",
            Self::Shotgun => "SHOTGUNS",
            Self::Pistol => "HANDGUNS",
            Self::Projectile => "ROCKETS",
            Self::Cqb => "CQB",
            Self::Special => "SPECIAL WEAPONS",
        }
    }

    #[must_use]
    pub fn loc_key(self) -> Option<&'static str> {
        Some(match self {
            Self::Assault => "@MENU_ASSAULT_RIFLES_CAPS",
            Self::Smg => "@MENU_SMGS_CAPS",
            Self::Lmg => "@MENU_LMGS_CAPS",
            Self::Sniper => "@MENU_SNIPER_RIFLES_CAPS",
            Self::Riot => "@MENU_RIOT_SHIELD_CAPS",
            Self::MachinePistol => "@MENU_MACHINE_PISTOLS_CAPS",
            Self::Shotgun => "@MENU_SHOTGUNS_CAPS",
            Self::Pistol => "@MENU_HANDGUNS_CAPS",
            Self::Projectile => "@MENU_ROCKETS_CAPS",
            Self::Cqb | Self::Special => return None,
        })
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Assault => "assault",
            Self::Smg => "smg",
            Self::Lmg => "lmg",
            Self::Sniper => "sniper",
            Self::Shotgun => "shotgun",
            Self::Riot => "riot",
            Self::MachinePistol => "machine_pistol",
            Self::Pistol => "pistol",
            Self::Projectile => "projectile",
            Self::Cqb => "cqb",
            Self::Special => "special",
        }
    }
}

#[must_use]
pub fn cac_category_from_item_group(group: &str) -> Option<CacAuthoredCategory> {
    match group {
        "weapon_assault" => Some(CacAuthoredCategory::Assault),
        "weapon_smg" => Some(CacAuthoredCategory::Smg),
        "weapon_lmg" => Some(CacAuthoredCategory::Lmg),
        "weapon_sniper" => Some(CacAuthoredCategory::Sniper),
        "weapon_shotgun" => Some(CacAuthoredCategory::Shotgun),
        "weapon_riot" => Some(CacAuthoredCategory::Riot),
        "weapon_machine_pistol" => Some(CacAuthoredCategory::MachinePistol),
        "weapon_pistol" => Some(CacAuthoredCategory::Pistol),
        "weapon_projectile" | "weapon_launcher" => Some(CacAuthoredCategory::Projectile),
        "weapon_cqb" => Some(CacAuthoredCategory::Cqb),
        "weapon_special" => Some(CacAuthoredCategory::Special),
        _ => None,
    }
}

impl CapturedStringTable {
    #[must_use]
    pub fn lookup(&self, search_col: i32, key: &str, return_col: i32) -> &str {
        for row in 0..self.rows {
            if self.cell(row as i32, search_col).eq_ignore_ascii_case(key) {
                return self.cell(row as i32, return_col);
            }
        }
        ""
    }
}

#[must_use]
pub fn weapon_stats_ref_candidates(name: &str) -> Vec<&str> {
    let mut out = Vec::with_capacity(4);
    out.push(name);
    let stem = name.strip_suffix("_mp").unwrap_or(name);
    if stem != name {
        out.push(stem);
    }
    if let Some(rest) = stem.strip_prefix("iw5_") {
        out.push(rest);
        if rest != stem {}
    }
    out
}

#[must_use]
pub fn item_group_for_weapon<'a>(
    table: &'a CapturedStringTable,
    weapon_name: &str,
) -> Option<&'a str> {
    for candidate in weapon_stats_ref_candidates(weapon_name) {
        let group = table.lookup(STATS_REF_COL, candidate, STATS_GROUP_COL);
        if !group.is_empty() {
            return Some(group);
        }
    }
    None
}

#[must_use]
pub fn iw4_fallback_item_group(weapon_name: &str) -> Option<&'static str> {
    let stem = weapon_name
        .strip_suffix("_mp")
        .unwrap_or(weapon_name)
        .rsplit(':')
        .next()
        .unwrap_or(weapon_name);
    let stem = stem.rsplit('/').next().unwrap_or(stem);
    match stem {
        "ak47" | "m16" | "m4" | "fn2000" | "masada" | "famas" | "fal" | "scar" | "tavor" => {
            Some("weapon_assault")
        }
        "mp5k" | "uzi" | "p90" | "kriss" | "ump45" => Some("weapon_smg"),
        "rpd" | "sa80" | "mg4" | "m240" | "aug" => Some("weapon_lmg"),
        "barrett" | "wa2000" | "m21" | "cheytac" => Some("weapon_sniper"),
        "ranger" | "model1887" | "striker" | "aa12" | "m1014" | "spas12" => Some("weapon_shotgun"),
        "riotshield" => Some("weapon_riot"),
        "beretta" | "usp" | "deserteagle" | "coltanaconda" | "deserteaglegold" => {
            Some("weapon_pistol")
        }
        "glock" | "beretta393" | "pp2000" | "tmp" => Some("weapon_machine_pistol"),
        "gl" | "m79" | "rpg" | "at4" | "stinger" | "javelin" => Some("weapon_projectile"),
        _ => None,
    }
}

#[must_use]
pub fn is_stats_table_name(name: &str) -> bool {
    let n = name.replace('\\', "/");
    let leaf = n.rsplit('/').next().unwrap_or(&n);
    leaf.eq_ignore_ascii_case("statstable.csv")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacWeaponFact {
    pub key: crate::AssetKey,
    pub weap_class: i32,
    pub item_group: Option<String>,
}

#[must_use]
pub fn primary_sniper_keys(rows: &[CacWeaponFact]) -> Vec<crate::AssetKey> {
    rows.iter()
        .filter(|row| {
            row.item_group.as_deref().is_some_and(|g| {
                cac_category_from_item_group(g) == Some(CacAuthoredCategory::Sniper)
            })
        })
        .map(|row| row.key.clone())
        .collect()
}

pub const STATS_NAME_COL: i32 = 3;
pub const STATS_IMAGE_COL: i32 = 6;
pub const STATS_DESC_COL: i32 = 7;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CacStatBar {
    Accuracy,
    Damage,
    Range,
    FireRate,
    Mobility,
}

impl CacStatBar {
    pub const ALL: [Self; 5] = [
        Self::Accuracy,
        Self::Damage,
        Self::Range,
        Self::FireRate,
        Self::Mobility,
    ];

    #[must_use]
    pub fn column(self) -> i32 {
        match self {
            Self::Accuracy => 22,
            Self::Damage => 23,
            Self::Range => 24,
            Self::FireRate => 25,
            Self::Mobility => 26,
        }
    }

    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Accuracy => "@MPUI_ACCURACY",
            Self::Damage => "@MPUI_DAMAGE",
            Self::Range => "@MPUI_RANGE",
            Self::FireRate => "@MPUI_FIRE_RATE",
            Self::Mobility => "@MPUI_MOBILITY",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacWeaponPreview {
    pub reference: String,
    pub name_key: String,
    pub image: String,
    pub desc_key: String,
    pub bars: Vec<(CacStatBar, i32)>,
}

#[must_use]
pub fn weapon_preview(table: &CapturedStringTable, weapon_name: &str) -> Option<CacWeaponPreview> {
    let leaf = weapon_name.rsplit(['/', ':']).next().unwrap_or(weapon_name);
    for candidate in weapon_stats_ref_candidates(leaf) {
        let Some(row) = table.lookup_row_in_col(STATS_REF_COL, candidate) else {
            continue;
        };
        let key = |col: i32| match table.cell(row, col) {
            "" => String::new(),
            cell => format!("@{cell}"),
        };
        let bars = CacStatBar::ALL
            .into_iter()
            .filter_map(|bar| {
                let cell = table.cell(row, bar.column());
                cell.parse::<i32>().ok().map(|value| (bar, value))
            })
            .collect();
        return Some(CacWeaponPreview {
            reference: candidate.to_owned(),
            name_key: key(STATS_NAME_COL),
            image: table.cell(row, STATS_IMAGE_COL).to_owned(),
            desc_key: key(STATS_DESC_COL),
            bars,
        });
    }
    None
}

pub const PERK_REF_COL: i32 = 1;
pub const PERK_NAME_COL: i32 = 2;
pub const PERK_ICON_COL: i32 = 3;
pub const PERK_DESC_COL: i32 = 4;
pub const PERK_SLOT_COL: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacPerkSlot {
    Perk1,
    Perk2,
    Perk3,
    Deathstreak,
}

impl CacPerkSlot {
    #[must_use]
    pub fn from_table_token(token: &str) -> Option<Self> {
        Some(match token {
            "perk1" => Self::Perk1,
            "perk2" => Self::Perk2,
            "perk3" => Self::Perk3,
            "perk4" => Self::Deathstreak,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacPerkRow {
    pub reference: String,
    pub name_key: String,
    pub image: String,
    pub desc_key: String,
    pub slot: CacPerkSlot,
}

#[must_use]
pub fn perk_rows(table: &CapturedStringTable) -> Vec<CacPerkRow> {
    let mut out = Vec::new();
    for row in 0..table.rows as i32 {
        let Some(slot) = CacPerkSlot::from_table_token(table.cell(row, PERK_SLOT_COL)) else {
            continue;
        };
        let reference = table.cell(row, PERK_REF_COL);

        if reference.is_empty() || reference.starts_with('_') {
            continue;
        }
        let key = |col: i32| match table.cell(row, col) {
            "" => String::new(),
            cell => format!("@{cell}"),
        };
        out.push(CacPerkRow {
            reference: reference.to_owned(),
            name_key: key(PERK_NAME_COL),
            image: table.cell(row, PERK_ICON_COL).to_owned(),
            desc_key: key(PERK_DESC_COL),
            slot,
        });
    }
    out
}

#[must_use]
pub fn perk_row<'a>(rows: &'a [CacPerkRow], reference: &str) -> Option<&'a CacPerkRow> {
    rows.iter()
        .find(|row| row.reference.eq_ignore_ascii_case(reference))
}

#[must_use]
pub fn is_perk_table_name(name: &str) -> bool {
    let n = name.replace('\\', "/");
    let leaf = n.rsplit('/').next().unwrap_or(&n);
    leaf.eq_ignore_ascii_case("perktable.csv")
}
