use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};

use crate::class_icons::cac_weapon_image;
use crate::class_presets::default_presets;

pub const NO_PERK: &str = "specialty_null";
pub(crate) const PICKER_PAGE_SIZE: usize = 12;

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

    pub fn perk_slot(self) -> Option<assets::CacPerkSlot> {
        Some(match self {
            Self::Perk1 => assets::CacPerkSlot::Perk1,
            Self::Perk2 => assets::CacPerkSlot::Perk2,
            Self::Perk3 => assets::CacPerkSlot::Perk3,
            Self::Deathstreak => assets::CacPerkSlot::Deathstreak,
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
    pub previews: std::collections::BTreeMap<String, assets::CacWeaponPreview>,
    pub resolver: CatalogResolver,
}

impl Default for ClassLoadoutCatalog {
    fn default() -> Self {
        let guns = |row| {
            retail_cac_roster(row)
                .iter()
                .map(|value| frame::CacWeaponOffer {
                    key: format!("iw4:weapon/{value}"),
                    item_group: assets::iw4_fallback_item_group(value).map(str::to_owned),
                    attachments: Vec::new(),
                })
                .collect()
        };
        let collect = |row| {
            retail_cac_roster(row)
                .iter()
                .map(|value| (*value).to_owned())
                .collect()
        };
        Self {
            revision: 0,
            primary: guns(ClassEditRow::Primary),
            secondary: guns(ClassEditRow::Secondary),
            lethal: guns(ClassEditRow::Lethal),
            tactical: guns(ClassEditRow::Tactical),
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
    pub fn from_weapon_registry(registry: std::sync::Arc<assets::WeaponRegistry>) -> Self {
        let families = registry.weapon_families();
        if families.families().is_empty() {
            return Self::default();
        }
        let mut catalog = Self::default();
        catalog.primary.clear();
        catalog.secondary.clear();
        catalog.lethal.clear();
        catalog.tactical.clear();
        for family in families.offered() {
            let key = family.key.asset_key();
            catalog.previews.insert(
                key.clone(),
                assets::CacWeaponPreview {
                    reference: family.key.base.clone(),
                    name_key: format!("@{}", family.display_key.trim_start_matches('@')),
                    image: family.image.clone(),
                    ..Default::default()
                },
            );
            for choice in &family.attachments {
                catalog.previews.insert(
                    attachment_preview_key(&key, &choice.name),
                    assets::CacWeaponPreview {
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
                assets::FamilySlot::Primary => catalog.primary.push(offer),
                assets::FamilySlot::Secondary => catalog.secondary.push(offer),
                assets::FamilySlot::Lethal => catalog.lethal.push(offer),
                assets::FamilySlot::Tactical => catalog.tactical.push(offer),
                assets::FamilySlot::Other => {}
            }
        }
        catalog.excluded = families.excluded().to_vec();
        catalog.resolver = CatalogResolver(Some(registry));
        catalog
    }

    pub fn with_weapon_tables(
        mut self,
        tables: &[(assets::AssetNamespace, assets::CapturedStringTable)],
    ) -> Self {
        for (namespace, table) in tables {
            if !assets::is_stats_table_name(&table.name) {
                continue;
            }
            for (key, preview) in &mut self.previews {
                if assets::AssetKey::parse(key).is_ok_and(|key| key.namespace == *namespace)
                    && let Some(authored) = assets::weapon_preview(table, key)
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

    pub fn with_perk_table(mut self, table: &assets::CapturedStringTable) -> Self {
        let rows = assets::perk_rows(table);
        if rows.is_empty() {
            return self;
        }
        let of_slot = |slot: assets::CacPerkSlot| -> Vec<String> {
            rows.iter()
                .filter(|row| row.reference == NO_PERK || row.slot == slot)
                .map(|row| row.reference.clone())
                .collect()
        };
        self.perks = [
            of_slot(assets::CacPerkSlot::Perk1),
            of_slot(assets::CacPerkSlot::Perk2),
            of_slot(assets::CacPerkSlot::Perk3),
        ];
        self.deathstreak = of_slot(assets::CacPerkSlot::Deathstreak);
        self
    }

    pub fn categories_for(&self, row: ClassEditRow) -> Vec<ClassPickerFolder> {
        let mut folders = Vec::new();
        for offer in self.offers(row) {
            let Ok(key) = assets::AssetKey::parse(&offer.key) else {
                continue;
            };
            let category = if matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary) {
                let Some(category) = offer
                    .item_group
                    .as_deref()
                    .and_then(assets::cac_category_from_item_group)
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
                assets::AssetKey::parse(&offer.key)
                    .is_ok_and(|key| key.namespace == folder.namespace)
                    && folder.category.is_none_or(|category| {
                        offer
                            .item_group
                            .as_deref()
                            .and_then(assets::cac_category_from_item_group)
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

    pub fn check_attachments(
        &self,
        row: ClassEditRow,
        weapon: &str,
        attachments: &[String],
        rules: assets::LoadoutRules,
    ) -> Result<(), assets::ConfigurationRefusal> {
        if let Some(registry) = self.resolver.0.as_deref() {
            let family = assets::FamilyKey::parse(weapon)
                .ok_or_else(|| assets::ConfigurationRefusal::UnknownFamily(weapon.to_owned()))?;
            return registry
                .resolve_configuration(&assets::WeaponSelection::with(family, attachments), rules)
                .map(|_| ());
        }
        let offered = self.attachments(row, weapon);
        if let Some(name) = attachments.iter().find(|name| !offered.contains(name)) {
            return Err(assets::ConfigurationRefusal::NotOffered(name.clone()));
        }
        if attachments.len() > rules.max_attachments {
            return Err(assets::ConfigurationRefusal::RuleRestricted(format!(
                "at most {} attachments",
                rules.max_attachments
            )));
        }
        Ok(())
    }
}

pub fn attachment_preview_key(weapon: &str, attachment: &str) -> String {
    format!("{weapon}+{attachment}")
}

#[derive(Clone, Default)]
pub struct CatalogResolver(pub Option<std::sync::Arc<assets::WeaponRegistry>>);

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
    pub fn from_preset(preset: &crate::ClassPreset) -> Self {
        let perk = |i: usize| {
            preset
                .perks
                .get(i)
                .map(|p| p.reference.to_owned())
                .unwrap_or_else(|| NO_PERK.to_owned())
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

    pub fn loadout_rules(&self) -> assets::LoadoutRules {
        assets::LoadoutRules::for_class(&self.perk1)
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
    pub namespace: assets::AssetNamespace,
    pub category: Option<assets::CacAuthoredCategory>,
}

impl ClassPickerFolder {
    pub fn slug(self) -> String {
        match self.category {
            Some(category) => format!("{}:{}", self.namespace.as_str(), category.slug()),
            None => self.namespace.as_str().to_owned(),
        }
    }
}

#[derive(Resource, Clone, Debug)]
pub struct ClassSetupScratch {
    pub selected: usize,

    pub summary_active: bool,
    pub editing: Option<ClassEditRow>,

    pub editing_attachment: Option<ClassEditRow>,

    pub picker_category: Option<ClassPickerFolder>,
    pub picker_page: usize,

    pub rename_buffer: Option<String>,
    pub slots: Vec<ClassSlotState>,

    pub revision: u64,
}

impl Default for ClassSetupScratch {
    fn default() -> Self {
        Self {
            selected: 0,
            summary_active: false,
            editing: None,
            editing_attachment: None,
            picker_category: None,
            picker_page: 0,
            rename_buffer: None,
            slots: default_presets()
                .iter()
                .map(ClassSlotState::from_preset)
                .collect(),
            revision: 0,
        }
    }
}

impl ClassSetupScratch {
    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub(crate) fn reset_navigation(&mut self) {
        self.selected = self.selected.min(self.slots.len().saturating_sub(1));
        self.summary_active = false;
        self.editing = None;
        self.editing_attachment = None;
        self.picker_category = None;
        self.rename_buffer = None;
        self.bump();
    }

    pub fn select_slot(&mut self, index: usize) -> bool {
        if index < self.slots.len() {
            self.selected = index;
            self.summary_active = true;
            self.editing = None;
            self.editing_attachment = None;
            self.picker_category = None;
            self.bump();
            return true;
        }
        false
    }

    pub fn begin_edit(&mut self, row: ClassEditRow) {
        self.picker_page = 0;
        self.editing = Some(row);
        self.editing_attachment = None;
        self.picker_category = None;
        self.bump();
    }

    pub fn reset_class(&mut self) -> bool {
        let Some(preset) = crate::class_presets::preset_at(self.selected) else {
            return false;
        };
        let Some(slot) = self.slots.get_mut(self.selected) else {
            return false;
        };
        let lock = slot.lock_reason.take();
        *slot = ClassSlotState::from_preset(preset);
        slot.lock_reason = lock;
        self.bump();
        true
    }

    pub(crate) fn leave_summary(&mut self) -> bool {
        if !self.summary_active || self.editing.is_some() || self.editing_attachment.is_some() {
            return false;
        }
        self.summary_active = false;
        self.bump();
        true
    }

    pub fn cancel_edit(&mut self) {
        self.picker_page = 0;
        if let Some(row) = self.editing_attachment.take() {
            self.editing = Some(row);
            self.bump();
            return;
        }
        if self.picker_category.take().is_some() {
            self.bump();
            return;
        }
        self.editing = None;
        self.bump();
    }

    pub fn begin_attachment_edit(&mut self, row: ClassEditRow) -> bool {
        if !matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary) {
            return false;
        }
        self.editing = None;
        self.editing_attachment = Some(row);
        self.picker_page = 0;
        self.bump();
        true
    }

    pub fn pick_attachment(
        &mut self,
        catalog: &ClassLoadoutCatalog,
        value: Option<String>,
    ) -> bool {
        let Some(row) = self.editing_attachment else {
            return false;
        };
        let Some(slot) = self.slots.get_mut(self.selected) else {
            return false;
        };
        let weapon = slot.row_value(row).to_owned();
        let rules = slot.loadout_rules();
        let mut chosen = match row {
            ClassEditRow::Primary => slot.primary_attachments.clone(),
            ClassEditRow::Secondary => slot.secondary_attachments.clone(),
            _ => return false,
        };
        match value {
            None => chosen.clear(),
            Some(value) => {
                if let Some(at) = chosen.iter().position(|name| *name == value) {
                    chosen.remove(at);
                } else {
                    chosen.push(value);
                }
            }
        }
        if catalog
            .check_attachments(row, &weapon, &chosen, rules)
            .is_err()
        {
            return false;
        }
        match row {
            ClassEditRow::Primary => slot.primary_attachments = chosen,
            _ => slot.secondary_attachments = chosen,
        }
        self.editing_attachment = None;
        self.picker_category = None;
        self.picker_page = 0;
        self.bump();
        true
    }

    pub fn begin_rename(&mut self) {
        self.editing = None;
        self.editing_attachment = None;
        self.rename_buffer = self.slots.get(self.selected).map(|slot| slot.name.clone());
        self.bump();
    }

    pub fn commit_rename(&mut self, value: String) -> bool {
        if self.rename_buffer.is_none() {
            return false;
        }
        let value: String = value.trim().chars().take(20).collect();
        if value.is_empty() {
            return false;
        }
        let Some(slot) = self.slots.get_mut(self.selected) else {
            return false;
        };
        slot.name = value;
        self.rename_buffer = None;
        self.bump();
        true
    }

    pub fn cancel_rename(&mut self) {
        if self.rename_buffer.take().is_some() {
            self.bump();
        }
    }

    pub fn pick_category(
        &mut self,
        catalog: &ClassLoadoutCatalog,
        category: ClassPickerFolder,
    ) -> bool {
        let Some(row) = self.editing else {
            return false;
        };
        if !ClassLoadoutCatalog::uses_categories(row)
            || !catalog.categories_for(row).contains(&category)
        {
            return false;
        }
        self.picker_category = Some(category);
        self.picker_page = 0;
        self.bump();
        true
    }

    pub fn picker_options(&self, catalog: &ClassLoadoutCatalog, row: ClassEditRow) -> Vec<String> {
        match self.picker_category {
            Some(category) if ClassLoadoutCatalog::uses_categories(row) => {
                catalog.keys_in_category(row, category)
            }
            _ => catalog.options(row),
        }
    }

    pub(crate) fn is_picker(&self) -> bool {
        self.editing_attachment.is_some() || self.editing.is_some()
    }

    pub fn pick(&mut self, catalog: &ClassLoadoutCatalog, value: String) -> bool {
        let Some(row) = self.editing else {
            return false;
        };
        if !self
            .picker_options(catalog, row)
            .iter()
            .any(|option| option == &value)
        {
            return false;
        }
        let Some(slot) = self.slots.get_mut(self.selected) else {
            return false;
        };
        slot.set_row(row, value);
        self.editing = None;
        self.picker_page = 0;
        if matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary) {
            self.editing_attachment = Some(row);
        } else {
            self.picker_category = None;
        }
        self.bump();
        true
    }
}

pub(crate) fn class_widget_is_active(scratch: &ClassSetupScratch, id: &str) -> bool {
    if scratch.rename_buffer.is_some() {
        return matches!(
            id,
            "class_setup/rename_buffer" | "class_setup/rename_cancel" | "class_setup/rename_accept"
        );
    }
    if scratch.editing_attachment.is_some() {
        return id == "class_setup/attachment_cancel"
            || id == "class_setup/attachment_none"
            || id.starts_with("class_setup/attachment/");
    }
    if let Some(row) = scratch.editing {
        return id == "class_setup/pick_cancel"
            || matches!(
                id,
                "class_setup/pick/page_prev" | "class_setup/pick/page_next"
            )
            || if ClassLoadoutCatalog::uses_categories(row) && scratch.picker_category.is_none() {
                id.starts_with("class_setup/cat/")
            } else {
                id.starts_with("class_setup/pick/")
            };
    }
    if scratch.summary_active {
        id == "class_setup/back" || is_edit_row_widget(id)
    } else {
        id == "class_setup/back" || id.starts_with("class_setup/slot/")
    }
}

fn is_edit_row_widget(id: &str) -> bool {
    ClassEditRow::ALL.iter().any(|row| edit_row_id(*row) == id)
        || matches!(id, "class_setup/rename" | "class_setup/reset")
}

pub(crate) fn edit_row_id(row: ClassEditRow) -> &'static str {
    match row {
        ClassEditRow::Primary => "class_setup/primary",
        ClassEditRow::Secondary => "class_setup/secondary",
        ClassEditRow::Lethal => "class_setup/lethal",
        ClassEditRow::Tactical => "class_setup/tactical",
        ClassEditRow::Perk1 => "class_setup/perk1",
        ClassEditRow::Perk2 => "class_setup/perk2",
        ClassEditRow::Perk3 => "class_setup/perk3",
        ClassEditRow::Deathstreak => "class_setup/deathstreak",
    }
}

pub(crate) fn focus_after_cac_intent(
    scratch: &ClassSetupScratch,
    catalog: &ClassLoadoutCatalog,
    intent: &crate::UiIntent,
) -> Option<String> {
    use crate::UiIntent;
    match intent {
        UiIntent::CacSelectSlot(_) => Some("class_setup/primary".into()),
        UiIntent::CacEditRow(raw) => {
            let row = ClassEditRow::from_u8(*raw)?;
            if ClassLoadoutCatalog::uses_categories(row) {
                catalog
                    .categories_for(row)
                    .first()
                    .map(|category| format!("class_setup/cat/{}", category.slug()))
                    .or_else(|| Some("class_setup/pick_cancel".into()))
            } else if catalog.options(row).is_empty() {
                Some("class_setup/pick_cancel".into())
            } else {
                Some("class_setup/pick/0".into())
            }
        }
        UiIntent::CacPickCategory(_) => Some("class_setup/pick/0".into()),
        UiIntent::CacPick(_) => scratch.editing.map(|row| {
            if matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary) {
                "class_setup/attachment_none".into()
            } else {
                edit_row_id(row).to_owned()
            }
        }),
        UiIntent::CacResetClass => Some("class_setup/reset".into()),
        UiIntent::CacEditAttachments(raw) => ClassEditRow::from_u8(*raw)
            .filter(|row| matches!(row, ClassEditRow::Primary | ClassEditRow::Secondary))
            .map(|_| "class_setup/attachment_none".into()),
        UiIntent::CacPickAttachment(_) => scratch
            .editing_attachment
            .map(edit_row_id)
            .map(str::to_owned),
        UiIntent::CacBeginRename => Some("class_setup/rename_buffer".into()),
        UiIntent::CacCommitRename(_) | UiIntent::CacCancelRename => {
            Some("class_setup/rename".into())
        }
        UiIntent::CacCancelEdit => cancel_edit_focus_target(scratch),
        _ => None,
    }
}

pub(crate) fn cancel_edit_focus_target(scratch: &ClassSetupScratch) -> Option<String> {
    if let Some(row) = scratch.editing_attachment {
        let _ = row;
        Some("class_setup/pick/0".into())
    } else if let Some(category) = scratch.picker_category {
        Some(format!("class_setup/cat/{}", category.slug()))
    } else {
        scratch.editing.map(edit_row_id).map(str::to_owned)
    }
}

pub fn apply_cac_intent(
    scratch: &mut ClassSetupScratch,
    catalog: &ClassLoadoutCatalog,
    intent: &crate::model::UiIntent,
) -> bool {
    use crate::model::UiIntent;
    match intent {
        UiIntent::CacSelectSlot(index) => scratch.select_slot(*index as usize),
        UiIntent::CacEditRow(row) => {
            let Some(row) = ClassEditRow::from_u8(*row) else {
                return false;
            };
            scratch.begin_edit(row);
            true
        }
        UiIntent::CacPick(value) => scratch.pick(catalog, value.clone()),
        UiIntent::CacResetClass => scratch.reset_class(),
        UiIntent::CacPickCategory(raw) => {
            let Some(category) = scratch
                .editing
                .and_then(|row| catalog.categories_for(row).get(*raw as usize).copied())
            else {
                return false;
            };
            scratch.pick_category(catalog, category)
        }
        UiIntent::CacEditAttachments(raw) => {
            ClassEditRow::from_u8(*raw).is_some_and(|row| scratch.begin_attachment_edit(row))
        }
        UiIntent::CacPickAttachment(value) => scratch.pick_attachment(catalog, value.clone()),
        UiIntent::CacPage(delta) => {
            let count = if let Some(row) = scratch.editing_attachment {
                catalog
                    .attachments(row, scratch.slots[scratch.selected].row_value(row))
                    .len()
                    + 1
            } else if let Some(row) = scratch.editing {
                if ClassLoadoutCatalog::uses_categories(row) && scratch.picker_category.is_none() {
                    catalog.categories_for(row).len()
                } else {
                    scratch.picker_options(catalog, row).len()
                }
            } else {
                return false;
            };
            let pages = count.div_ceil(PICKER_PAGE_SIZE).max(1);
            let page = (scratch.picker_page as i32 + delta).rem_euclid(pages as i32) as usize;
            if page == scratch.picker_page {
                return false;
            }
            scratch.picker_page = page;
            scratch.bump();
            true
        }
        UiIntent::CacBeginRename => {
            scratch.begin_rename();
            true
        }
        UiIntent::CacCommitRename(value) => scratch.commit_rename(value.clone()),
        UiIntent::CacCancelRename => {
            scratch.cancel_rename();
            true
        }
        UiIntent::CacCancelEdit => {
            let folder_page = if scratch.editing_attachment.is_none() {
                scratch
                    .editing
                    .zip(scratch.picker_category)
                    .and_then(|(row, folder)| {
                        catalog
                            .categories_for(row)
                            .iter()
                            .position(|candidate| *candidate == folder)
                    })
                    .map(|index| index / PICKER_PAGE_SIZE)
            } else {
                None
            };
            scratch.cancel_edit();
            if let Some(page) = folder_page {
                scratch.picker_page = page;
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn apply_cac_intents(
    mut intents: MessageReader<crate::UiIntent>,
    mut scratch: ResMut<ClassSetupScratch>,
    catalog: Res<ClassLoadoutCatalog>,
    mut focus: ResMut<crate::Focus>,
) {
    for intent in intents.read() {
        let target = focus_after_cac_intent(&scratch, &catalog, intent);
        if apply_cac_intent(&mut scratch, &catalog, intent) {
            if matches!(intent, crate::UiIntent::CacPage(_)) {
                let start = scratch.picker_page * PICKER_PAGE_SIZE;
                focus.widget = Some(if scratch.editing_attachment.is_some() {
                    if start == 0 {
                        "class_setup/attachment_none".into()
                    } else {
                        format!("class_setup/attachment/{}", start - 1)
                    }
                } else {
                    if let Some(row) = scratch.editing
                        && ClassLoadoutCatalog::uses_categories(row)
                        && scratch.picker_category.is_none()
                    {
                        catalog
                            .categories_for(row)
                            .get(start)
                            .map(|folder| format!("class_setup/cat/{}", folder.slug()))
                            .unwrap_or_else(|| "class_setup/pick_cancel".into())
                    } else {
                        format!("class_setup/pick/{start}")
                    }
                });
            } else if let Some(target) = target {
                focus.widget = Some(target);
            }
        }
    }
}

pub(crate) fn drive_cac_pages(
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<bevy::input::mouse::MouseWheel>,
    mut commands: MessageReader<crate::MenuShellCmd>,
    stack: Res<crate::RetailMenuStack>,
    scratch: Res<ClassSetupScratch>,
    mut intents: MessageWriter<crate::UiIntent>,
) {
    let scroll: f32 = wheel.read().map(|event| event.y).sum();
    let mut direction = 0;
    for command in commands.read() {
        match command {
            crate::MenuShellCmd::Nav(crate::NavDir::Left) => direction = -1,
            crate::MenuShellCmd::Nav(crate::NavDir::Right) => direction = 1,
            _ => {}
        }
    }
    if stack.names.last().map(String::as_str) != Some("class_setup")
        || scratch.rename_buffer.is_some()
    {
        return;
    }
    if scratch.editing.is_none() && scratch.editing_attachment.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::PageUp) || scroll > 0.0 {
        direction = -1;
    }
    if keys.just_pressed(KeyCode::ArrowRight)
        || keys.just_pressed(KeyCode::PageDown)
        || scroll < 0.0
    {
        direction = 1;
    }
    if direction != 0 {
        intents.write(crate::UiIntent::CacPage(direction));
    }
}

pub(crate) fn edit_class_name(
    mut keyboard: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    mut scratch: ResMut<ClassSetupScratch>,
    mut out: MessageWriter<crate::UiIntent>,
) {
    let events: Vec<KeyboardInput> = keyboard.read().cloned().collect();
    let Some(mut buffer) = scratch.rename_buffer.clone() else {
        return;
    };
    let mut changed = false;
    let mut done = None;
    for event in &events {
        if !event.state.is_pressed() {
            continue;
        }
        match &event.logical_key {
            Key::Character(text)
                if !keys.pressed(KeyCode::ControlLeft) && !keys.pressed(KeyCode::ControlRight) =>
            {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    if buffer.chars().count() >= 20 {
                        break;
                    }
                    buffer.push(ch);
                    changed = true;
                }
            }
            Key::Backspace => changed |= buffer.pop().is_some(),
            Key::Enter => done = Some(true),
            Key::Escape => done = Some(false),
            _ => {}
        }
    }
    match done {
        Some(true) => {
            out.write(crate::UiIntent::CacCommitRename(buffer));
        }
        Some(false) => {
            out.write(crate::UiIntent::CacCancelRename);
        }
        None => {
            if changed {
                scratch.rename_buffer = Some(buffer);
                scratch.bump();
            }
        }
    }
}

fn retail_cac_roster(row: ClassEditRow) -> &'static [&'static str] {
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
        for opt in retail_cac_roster(row) {
            if row.perk_slot().is_some() {
                stems.push(crate::class_icons::cac_material_iwd_stem(opt));
            } else if let Some(stem) = cac_weapon_image(opt) {
                stems.push(stem);
            }
        }
    }
    stems.sort_unstable();
    stems.dedup();
    stems
}
