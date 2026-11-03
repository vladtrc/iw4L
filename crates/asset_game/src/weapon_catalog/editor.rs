use super::configuration::WeaponConfigurationCompiler;
use super::*;
use crate::weapon_families::FamilyContent;

#[derive(Clone, Debug)]
pub struct EditorWeaponCatalog {
    registry: Arc<WeaponRegistry>,
    families: crate::WeaponFamilies,
}

pub struct EditorWeaponSelection {
    row: u32,
    pub selection: crate::WeaponSelection,
}

impl EditorWeaponSelection {
    pub fn wire_id(&self) -> u32 {
        self.row
    }
}

impl WeaponRegistry {
    pub fn editor_catalog(self: &Arc<Self>) -> EditorWeaponCatalog {
        let mut editor = EditorWeaponCatalog {
            registry: self.clone(),
            families: Default::default(),
        };
        editor.families = crate::WeaponFamilies::build(&self.family_tables, &editor);
        editor
    }
}

impl WeaponConfigurationCompiler for EditorWeaponCatalog {
    fn compile(
        &self,
        family: &crate::WeaponFamily,
        selection: &crate::WeaponSelection,
    ) -> Result<u32, crate::ConfigurationRefusal> {
        self.registry
            .compile_native_configuration(family, selection)
    }
}

impl FamilyContent for EditorWeaponCatalog {
    fn published_handle(&self, id: u32) -> Option<crate::WeaponHandle> {
        FamilyContent::published_handle(self.registry.as_ref(), id)
    }
    fn lookup(&self, ns: crate::FamilyId, name: &str) -> Option<u32> {
        FamilyContent::lookup(self.registry.as_ref(), ns, name)
    }
    fn offhand_class(&self, id: u32) -> i32 {
        FamilyContent::offhand_class(self.registry.as_ref(), id)
    }
    fn admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        self.registry.native_configuration_admission(id)
    }
    fn names_in(&self, ns: crate::FamilyId) -> Vec<(u32, String)> {
        FamilyContent::names_in(self.registry.as_ref(), ns)
    }
    fn prepared_all(&self) -> Vec<(u32, crate::WeaponSelection)> {
        FamilyContent::prepared_all(self.registry.as_ref())
    }
}

impl EditorWeaponCatalog {
    pub fn shared_rank_table(&self) -> Result<&crate::CapturedStringTable, &'static str> {
        self.registry.shared_rank_table()
    }
    pub fn shared_rank_icons(&self) -> Result<&crate::CapturedStringTable, &'static str> {
        self.registry.shared_rank_icons()
    }
    pub fn rank_progression(
        &self,
    ) -> Result<gamemode_iw4::progression::RankProgression, &'static str> {
        self.registry.rank_progression()
    }
    pub fn item_unlock_requirement(
        &self,
        namespace: crate::FamilyId,
        name: &str,
    ) -> Result<gamemode_iw4::progression::UnlockRequirement, &'static str> {
        self.registry.item_unlock_requirement(namespace, name)
    }

    pub fn len(&self) -> usize {
        self.registry.len()
    }
    pub fn is_empty(&self) -> bool {
        self.registry.is_empty()
    }
    pub fn namespace_count(&self, ns: crate::FamilyId) -> usize {
        self.registry.namespace_count(ns)
    }
    pub fn weapon_families(&self) -> &crate::WeaponFamilies {
        &self.families
    }
    pub fn admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        self.registry.native_configuration_admission(id)
    }
    pub fn resolve_configuration(
        &self,
        selection: &crate::WeaponSelection,
        rules: crate::LoadoutRules,
    ) -> Result<EditorWeaponSelection, crate::ConfigurationRefusal> {
        let resolved = self.families.resolve(selection, rules, self)?;
        let row = self
            .registry
            .bind(resolved.handle())
            .expect("editor publication owner")
            .wire_id();
        Ok(EditorWeaponSelection {
            row,
            selection: selection.clone(),
        })
    }
    pub fn resolve_index(&self, name: &str) -> Result<Option<u32>, UnknownWeaponName> {
        self.registry.resolve_index(name)
    }
    pub fn describe_configuration(&self, id: u32) -> Option<&crate::WeaponSelection> {
        self.families.describe(id)
    }
    pub fn identity_namespace_of(&self, id: u32) -> Option<crate::FamilyId> {
        self.registry.identity_namespace_of(id)
    }
    pub fn camouflage_slot(&self, id: u32, name: &str) -> Option<u8> {
        self.registry.camouflage_slot(id, name)
    }
    pub fn camouflage_choices(&self, id: u32) -> Vec<(u8, &str)> {
        self.registry.camouflage_choices(id)
    }
    pub fn appearance_status(
        &self,
        id: u32,
        slot: u8,
    ) -> Option<(AppearanceModelStatus, AppearanceModelStatus)> {
        let appearance = self.registry.select_appearance(id, slot)?;
        Some((appearance.view_status(), appearance.world_status()))
    }
    pub fn camouflage_caption(&self, id: u32, name: &str) -> Option<&str> {
        self.registry.camouflage_caption(id, name)
    }
    pub fn camouflage_preview(&self, id: u32, name: &str) -> Option<String> {
        self.registry.camouflage_preview(id, name)
    }
    pub fn hud_icon_image_of(&self, id: u32) -> Option<&str> {
        self.registry.hud_icon_image_of(id)
    }
    pub fn class_slot_admission(&self, id: u32, slot: usize) -> bool {
        if id == 0 {
            return true;
        }
        if self.admission(id).is_err() {
            return false;
        }
        if slot < 2 {
            self.registry
                .bind_published_row(id)
                .ok()
                .and_then(|weapon| {
                    weapon
                        .combat_facts(weapon_iw4::WeaponHostRules::default(), None)
                        .ok()
                })
                .is_some_and(weapon_iw4::WeaponCombatFacts::is_usable)
        } else {
            self.registry.equipment_facts_of(id).is_some_and(|facts| {
                facts.is_offhand()
                    && matches!((slot, facts.offhand_class), (2, 1 | 4 | 5) | (3, 2 | 3))
            })
        }
    }
}
