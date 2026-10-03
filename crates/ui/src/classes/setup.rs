use bevy::prelude::*;

use crate::classes::icons::cac_weapon_image;

pub const NO_PERK: &str = "specialty_null";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassEditRow {
    Primary,
    Secondary,
    Lethal,
    Tactical,
    Perk1,
    Perk2,
    Perk3,
    Deathstreak,
}

impl ClassEditRow {
    pub const ALL: [Self; 8] = [
        Self::Primary,
        Self::Secondary,
        Self::Lethal,
        Self::Tactical,
        Self::Perk1,
        Self::Perk2,
        Self::Perk3,
        Self::Deathstreak,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Primary => "Primary",
            Self::Secondary => "Secondary",
            Self::Lethal => "Lethal",
            Self::Tactical => "Tactical",
            Self::Perk1 => "Perk 1",
            Self::Perk2 => "Perk 2",
            Self::Perk3 => "Perk 3",
            Self::Deathstreak => "Deathstreak",
        }
    }

    pub fn loc_key(self) -> &'static str {
        match self {
            Self::Primary => "@MENU_PRIMARY_CAPS",
            Self::Secondary => "@MENU_SECONDARY_CAPS",
            Self::Lethal => "@MENU_EQUIPMENT_CAPS",
            Self::Tactical => "@MENU_SPECIAL_GRENADE_CAPS",
            Self::Perk1 => "@MENU_PERK1_CAPS",
            Self::Perk2 => "@MENU_PERK2_CAPS",
            Self::Perk3 => "@MENU_PERK3_CAPS",
            Self::Deathstreak => "@MENU_DEATHSTREAK_CAPS",
        }
    }

    pub fn perk_slot(self) -> Option<asset_game::CacPerkSlot> {
        Some(match self {
            Self::Perk1 => asset_game::CacPerkSlot::Perk1,
            Self::Perk2 => asset_game::CacPerkSlot::Perk2,
            Self::Perk3 => asset_game::CacPerkSlot::Perk3,
            Self::Deathstreak => asset_game::CacPerkSlot::Deathstreak,
            _ => return None,
        })
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Primary,
            1 => Self::Secondary,
            2 => Self::Lethal,
            3 => Self::Tactical,
            4 => Self::Perk1,
            5 => Self::Perk2,
            6 => Self::Perk3,
            7 => Self::Deathstreak,
            _ => return None,
        })
    }
}

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct ClassLoadoutCatalog {
    pub revision: u64,
    pub primary: Vec<frame::CacWeaponOffer>,
    pub secondary: Vec<frame::CacWeaponOffer>,
    pub lethal: Vec<frame::CacWeaponOffer>,
    pub tactical: Vec<frame::CacWeaponOffer>,
    pub perks: [Vec<String>; 3],
    pub deathstreak: Vec<String>,
    pub excluded: Vec<(String, String)>,
    pub previews: std::collections::BTreeMap<String, asset_game::CacWeaponPreview>,
    pub resolver: CatalogResolver,
}

impl Default for ClassLoadoutCatalog {
    fn default() -> Self {
        let collect = |row| {
            cac_roster(row)
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        };
        Self {
            revision: 0,
            primary: Vec::new(),
            secondary: Vec::new(),
            lethal: Vec::new(),
            tactical: Vec::new(),
            perks: [
                collect(ClassEditRow::Perk1),
                collect(ClassEditRow::Perk2),
                collect(ClassEditRow::Perk3),
            ],
            deathstreak: collect(ClassEditRow::Deathstreak),
            excluded: Vec::new(),
            previews: std::collections::BTreeMap::new(),
            resolver: CatalogResolver::default(),
        }
    }
}

impl ClassLoadoutCatalog {
    pub fn from_weapon_registry(registry: std::sync::Arc<asset_game::WeaponRegistry>) -> Self {
        let families = registry.weapon_families();
        let mut catalog = Self::default();
        for family in families.offered() {
            let key = family.key.asset_key();
            catalog.previews.insert(
                key.clone(),
                asset_game::CacWeaponPreview {
                    reference: family.key.base.clone(),
                    name_key: format!("@{}", family.name_key()),
                    image: family.image.clone(),
                    ..Default::default()
                },
            );
            for choice in &family.attachments {
                catalog.previews.insert(
                    attachment_preview_key(&key, &choice.name),
                    asset_game::CacWeaponPreview {
                        reference: choice.name.clone(),
                        name_key: format!("@{}", choice.caption_key),
                        image: choice.icon.clone(),
                        desc_key: format!("@{}", choice.desc_key),
                        bars: Vec::new(),
                    },
                );
            }
            let offer = frame::CacWeaponOffer {
                key,
                item_group: Some(family.item_group.clone()),
                attachments: family
                    .attachments
                    .iter()
                    .map(|choice| choice.name.clone())
                    .collect(),
            };
            match family.slot {
                asset_game::FamilySlot::Primary => catalog.primary.push(offer),
                asset_game::FamilySlot::Secondary => catalog.secondary.push(offer),
                asset_game::FamilySlot::Lethal => catalog.lethal.push(offer),
                asset_game::FamilySlot::Tactical => catalog.tactical.push(offer),
                asset_game::FamilySlot::Other => {}
            }
        }
        catalog.excluded = families.excluded().to_vec();
        catalog.resolver = CatalogResolver(Some(registry));
        catalog
    }

    pub fn with_weapon_tables(
        mut self,
        tables: &[(asset_core::AssetNamespace, asset_game::CapturedStringTable)],
    ) -> Self {
        for (namespace, table) in tables {
            if !asset_game::is_stats_table_name(&table.name) {
                continue;
            }
            for (key, preview) in &mut self.previews {
                if asset_core::AssetKey::parse(key).is_ok_and(|key| key.namespace == *namespace)
                    && let Some(authored) = asset_game::weapon_preview(table, key)
                {
                    *preview = authored;
                }
            }
        }
        self
    }

    pub fn options(&self, row: ClassEditRow) -> Vec<String> {
        match row {
            ClassEditRow::Primary => self.primary.iter().map(|o| o.key.clone()).collect(),
            ClassEditRow::Secondary => self.secondary.iter().map(|o| o.key.clone()).collect(),
            ClassEditRow::Lethal => self.lethal.iter().map(|o| o.key.clone()).collect(),
            ClassEditRow::Tactical => self.tactical.iter().map(|o| o.key.clone()).collect(),
            ClassEditRow::Perk1 => self.perks[0].clone(),
            ClassEditRow::Perk2 => self.perks[1].clone(),
            ClassEditRow::Perk3 => self.perks[2].clone(),
            ClassEditRow::Deathstreak => self.deathstreak.clone(),
        }
    }

    fn offers(&self, row: ClassEditRow) -> &[frame::CacWeaponOffer] {
        match row {
            ClassEditRow::Primary => &self.primary,
            ClassEditRow::Secondary => &self.secondary,
            ClassEditRow::Lethal => &self.lethal,
            ClassEditRow::Tactical => &self.tactical,
            ClassEditRow::Perk1
            | ClassEditRow::Perk2
            | ClassEditRow::Perk3
            | ClassEditRow::Deathstreak => &[],
        }
    }

    pub fn with_perk_table(mut self, table: &asset_game::CapturedStringTable) -> Self {
        let rows = asset_game::perk_rows(table);
        if rows.is_empty() {
            return self;
        }
        let of_slot = |slot: asset_game::CacPerkSlot| -> Vec<String> {
            rows.iter()
                .filter(|row| row.reference == NO_PERK || row.slot == slot)
                .map(|row| row.reference.clone())
                .collect()
        };
        self.perks = [
            of_slot(asset_game::CacPerkSlot::Perk1),
            of_slot(asset_game::CacPerkSlot::Perk2),
            of_slot(asset_game::CacPerkSlot::Perk3),
        ];
        self.deathstreak = of_slot(asset_game::CacPerkSlot::Deathstreak);
        for offer in self.lethal.iter().chain(&self.tactical) {
            if asset_core::AssetKey::parse(&offer.key)
                .is_ok_and(|key| key.namespace == asset_core::AssetNamespace::Iw4)
            {
                let leaf = offer.key.rsplit('/').next().unwrap_or(&offer.key);
                let reference = if leaf.ends_with("_mp") {
                    leaf.to_owned()
                } else {
                    format!("{leaf}_mp")
                };
                let icon = table.lookup(1, &reference, 3);
                if !icon.is_empty()
                    && let Some(preview) = self.previews.get_mut(&offer.key)
                {
                    preview.image = icon.to_owned();
                    preview.desc_key = format!("@{}", table.lookup(1, &reference, 4));
                }
            }
        }
        for row in rows {
            self.previews.insert(
                row.reference.clone(),
                asset_game::CacWeaponPreview {
                    reference: row.reference,
                    name_key: row.name_key,
                    image: row.image,
                    desc_key: row.desc_key,
                    ..Default::default()
                },
            );
        }
        self
    }

    pub fn categories_for(&self, row: ClassEditRow) -> Vec<ClassPickerFolder> {
        let mut folders = Vec::new();
        for offer in self.offers(row) {
            let Ok(key) = asset_core::AssetKey::parse(&offer.key) else {
                continue;
            };
            let category = if matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary) {
                let Some(category) = offer
                    .item_group
                    .as_deref()
                    .and_then(asset_game::cac_category_from_item_group)
                else {
                    continue;
                };
                Some(category)
            } else {
                None
            };
            let folder = ClassPickerFolder {
                namespace: key.namespace,
                category,
            };
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
        folders.sort_by_key(|folder| (folder.namespace.as_str(), folder.category));
        folders
    }

    pub fn keys_in_category(&self, row: ClassEditRow, folder: ClassPickerFolder) -> Vec<String> {
        self.offers(row)
            .iter()
            .filter(|offer| {
                asset_core::AssetKey::parse(&offer.key)
                    .is_ok_and(|key| key.namespace == folder.namespace)
                    && folder.category.is_none_or(|category| {
                        offer
                            .item_group
                            .as_deref()
                            .and_then(asset_game::cac_category_from_item_group)
                            == Some(category)
                    })
            })
            .map(|offer| offer.key.clone())
            .collect()
    }

    pub fn uses_categories(row: ClassEditRow) -> bool {
        row.perk_slot().is_none()
    }

    pub fn attachments(&self, row: ClassEditRow, weapon: &str) -> &[String] {
        self.offers(row)
            .iter()
            .find(|offer| offer.key == weapon)
            .map(|offer| offer.attachments.as_slice())
            .unwrap_or(&[])
    }

    pub fn validate_class(&self, slot: &ClassSlotState) -> Result<(), String> {
        let registry = self
            .resolver
            .0
            .as_deref()
            .ok_or("Weapon catalog is not ready")?;
        let row = session::ClassRow::from(&frame::HostClassSlot::from(slot));
        session::loadout::resolve_personal_class(&row, registry).map(|_| ())
    }

    pub fn validate_edit(&self, slot: &ClassSlotState, row: ClassEditRow) -> Result<(), String> {
        let registry = self
            .resolver
            .0
            .as_deref()
            .ok_or("Weapon catalog is not ready")?;
        let attachments = match row {
            ClassEditRow::Primary => slot.primary_attachments.as_slice(),
            ClassEditRow::Secondary => slot.secondary_attachments.as_slice(),
            _ => &[],
        };
        if Self::uses_categories(row) {
            session::resolve_class_weapon(
                registry,
                slot.row_value(row),
                attachments,
                slot.loadout_rules(),
            )?;
        } else if !self
            .options(row)
            .iter()
            .any(|value| value == slot.row_value(row))
        {
            return Err("Perk is unavailable".into());
        }
        Ok(())
    }

    pub fn check_attachments(
        &self,
        weapon: &str,
        attachments: &[String],
        rules: asset_game::LoadoutRules,
    ) -> Result<(), asset_game::ConfigurationRefusal> {
        let registry = self.resolver.0.as_deref().ok_or_else(|| {
            asset_game::ConfigurationRefusal::MissingContent("Weapon catalog is not ready".into())
        })?;
        let family = asset_game::FamilyKey::parse(weapon)
            .ok_or_else(|| asset_game::ConfigurationRefusal::UnknownFamily(weapon.to_owned()))?;
        registry
            .resolve_configuration(
                &asset_game::WeaponSelection::with(family, attachments),
                rules,
            )
            .map(|_| ())
    }
}

pub fn attachment_preview_key(weapon: &str, attachment: &str) -> String {
    format!("{weapon}+{attachment}")
}

#[derive(Clone, Default)]
pub struct CatalogResolver(pub Option<std::sync::Arc<asset_game::WeaponRegistry>>);

impl PartialEq for CatalogResolver {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        }
    }
}

impl Eq for CatalogResolver {}

impl std::fmt::Debug for CatalogResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self.0 {
            Some(_) => "CatalogResolver(registry)",
            None => "CatalogResolver(none)",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassSlotState {
    pub name: String,
    pub primary: String,
    pub primary_attachments: Vec<String>,
    pub secondary: String,
    pub secondary_attachments: Vec<String>,
    pub lethal: String,
    pub tactical: String,
    pub perk1: String,
    pub perk2: String,
    pub perk3: String,
    pub deathstreak: String,

    pub lock_reason: Option<String>,
}

impl ClassSlotState {
    pub fn from_host_slot(slot: &frame::HostClassSlot) -> Self {
        Self {
            name: slot.name.clone(),
            primary: slot.primary.clone(),
            secondary: slot.secondary.clone(),
            primary_attachments: slot.primary_attachments.clone(),
            secondary_attachments: slot.secondary_attachments.clone(),
            lethal: slot.lethal.clone(),
            tactical: slot.tactical.clone(),
            perk1: slot.perks[0].clone(),
            perk2: slot.perks[1].clone(),
            perk3: slot.perks[2].clone(),
            deathstreak: slot.deathstreak.clone(),
            lock_reason: None,
        }
    }

    pub fn from_preset(preset: &crate::ClassPreset) -> Self {
        let perk = |i: usize| match preset.perks[i] {
            "" => NO_PERK.to_owned(),
            perk => perk.to_owned(),
        };
        Self {
            name: preset.name.to_owned(),
            primary: preset.primary.to_owned(),
            primary_attachments: preset
                .primary_attachments
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            secondary: preset.secondary.to_owned(),
            secondary_attachments: preset
                .secondary_attachments
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            lethal: preset.lethal.to_owned(),
            tactical: preset.tactical.to_owned(),
            perk1: perk(0),
            perk2: perk(1),
            perk3: perk(2),
            deathstreak: preset.deathstreak.to_owned(),
            lock_reason: None,
        }
    }

    pub fn row_value(&self, row: ClassEditRow) -> &str {
        match row {
            ClassEditRow::Primary => &self.primary,
            ClassEditRow::Secondary => &self.secondary,
            ClassEditRow::Lethal => &self.lethal,
            ClassEditRow::Tactical => &self.tactical,
            ClassEditRow::Perk1 => &self.perk1,
            ClassEditRow::Perk2 => &self.perk2,
            ClassEditRow::Perk3 => &self.perk3,
            ClassEditRow::Deathstreak => &self.deathstreak,
        }
    }

    pub fn loadout_rules(&self) -> asset_game::LoadoutRules {
        asset_game::LoadoutRules::for_class(&self.perk1)
    }

    pub fn set_row(&mut self, row: ClassEditRow, value: String) {
        match row {
            ClassEditRow::Primary => {
                if self.primary != value {
                    self.primary = value;
                    self.primary_attachments.clear();
                }
            }
            ClassEditRow::Secondary => {
                if self.secondary != value {
                    self.secondary = value;
                    self.secondary_attachments.clear();
                }
            }
            ClassEditRow::Lethal => self.lethal = value,
            ClassEditRow::Tactical => self.tactical = value,
            ClassEditRow::Perk1 => self.perk1 = value,
            ClassEditRow::Perk2 => self.perk2 = value,
            ClassEditRow::Perk3 => self.perk3 = value,
            ClassEditRow::Deathstreak => self.deathstreak = value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassPickerFolder {
    pub namespace: asset_core::AssetNamespace,
    pub category: Option<asset_game::CacAuthoredCategory>,
}

impl ClassPickerFolder {
    pub fn slug(self) -> String {
        match self.category {
            Some(category) => format!("{}:{}", self.namespace.as_str(), category.slug()),
            None => self.namespace.as_str().to_owned(),
        }
    }
}

fn cac_roster(row: ClassEditRow) -> &'static [&'static str] {
    match row {
        ClassEditRow::Primary => &[
            "m4_mp",
            "famas_mp",
            "scar_mp",
            "tar21_mp",
            "fal_mp",
            "m16_mp",
            "masada_mp",
            "fn2000_mp",
            "ak47_mp",
            "mp5k_mp",
            "uzi_mp",
            "p90_mp",
            "kriss_mp",
            "ump45_mp",
            "rpd_mp",
            "sa80_mp",
            "mg4_mp",
            "m240_mp",
            "aug_mp",
            "barrett_mp",
            "cheytac_mp",
            "wa2000_mp",
            "m21_mp",
            "riotshield_mp",
        ],
        ClassEditRow::Secondary => &[
            "glock_mp",
            "beretta393_mp",
            "pp2000_mp",
            "tmp_mp",
            "ranger_mp",
            "model1887_mp",
            "striker_mp",
            "aa12_mp",
            "m1014_mp",
            "spas12_mp",
            "usp_mp",
            "beretta_mp",
            "deserteagle_mp",
            "coltanaconda_mp",
            "at4_mp",
            "rpg_mp",
            "stinger_mp",
            "javelin_mp",
        ],
        ClassEditRow::Lethal => &[
            "frag_grenade_mp",
            "semtex_mp",
            "throwingknife_mp",
            "claymore_mp",
            "c4_mp",
        ],
        ClassEditRow::Tactical => &[
            "flash_grenade_mp",
            "concussion_grenade_mp",
            "smoke_grenade_mp",
            "flare_mp",
        ],
        ClassEditRow::Perk1 => &[
            "specialty_null",
            "specialty_marathon",
            "specialty_fastreload",
            "specialty_scavenger",
            "specialty_onemanarmy",
            "specialty_bling",
        ],
        ClassEditRow::Perk2 => &[
            "specialty_null",
            "specialty_bulletdamage",
            "specialty_lightweight",
            "specialty_hardline",
            "specialty_coldblooded",
            "specialty_explosivedamage",
        ],
        ClassEditRow::Perk3 => &[
            "specialty_null",
            "specialty_extendedmelee",
            "specialty_bulletaccuracy",
            "specialty_localjammer",
            "specialty_heartbreaker",
            "specialty_detectexplosive",
            "specialty_pistoldeath",
        ],
        ClassEditRow::Deathstreak => &[
            "specialty_null",
            "specialty_grenadepulldeath",
            "specialty_c4death",
            "specialty_combathigh",
            "specialty_finalstand",
            "specialty_copycat",
        ],
    }
}

pub fn picker_icon_stems() -> Vec<&'static str> {
    let mut stems = Vec::new();
    for row in ClassEditRow::ALL {
        for opt in cac_roster(row) {
            if row.perk_slot().is_some() {
                stems.push(crate::classes::icons::cac_material_iwd_stem(opt));
            } else if let Some(stem) = cac_weapon_image(opt) {
                stems.push(stem);
            }
        }
    }
    stems.sort_unstable();
    stems.dedup();
    stems
}
