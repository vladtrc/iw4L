use super::*;
use crate::{
    ConfigurationPreparationCensus, WeaponConfigurationDraft, WeaponPreparationContent,
    WeaponSelection,
};

struct Content<'a> {
    build: &'a mut WeaponBuild,
    families: crate::WeaponFamilies,
}
impl WeaponPreparationContent for Content<'_> {
    fn tables(&self) -> &[(crate::FamilyId, crate::CapturedStringTable)] {
        &self.build.family_tables
    }
    fn family(&self, key: &crate::FamilyKey) -> Option<crate::WeaponFamily> {
        self.families.family(key).cloned()
    }
    fn authored(&self, family: crate::FamilyId, name: &str) -> Option<u32> {
        self.build
            .registry
            .by_namespaced
            .get(&(family, name.to_owned()))
            .copied()
    }

    fn candidates(&self, family: crate::FamilyId) -> Vec<(u32, WeaponSelection)> {
        self.families.candidate_selections(family)
    }
    fn normalize(&self, family: crate::FamilyId, attachments: &[String]) -> Vec<String> {
        self.families.normalize(family, attachments)
    }
    fn draft(&self, id: u32) -> Option<WeaponConfigurationDraft> {
        self.build.configuration_draft_of(id)
    }
    fn len(&self) -> usize {
        self.build.registry.rows.len()
    }
    fn attachments(&self) -> &WeaponAttachments {
        &self.build.registry.attachments
    }
    fn append(
        &mut self,
        template: u32,
        combat_source: u32,
        selection: Option<WeaponSelection>,
        draft: WeaponConfigurationDraft,
    ) -> u32 {
        let row = self
            .build
            .registry
            .rows
            .get(template as usize)
            .expect("native preparation template exists")
            .clone();
        assert_eq!(
            row.namespace, draft.namespace,
            "native preparation changed row family"
        );
        let id = self.build.registry.rows.len() as u32;
        let slots = self
            .build
            .combat_slots
            .get(combat_source as usize)
            .copied()
            .unwrap_or_else(CombatFxSlots::default);
        self.build
            .combat_slots
            .resize(id as usize, CombatFxSlots::default());
        self.build.combat_slots.push(slots);
        self.build.registry.rows.push(row);
        assert!(self.build.set_configuration_draft(id, draft));
        if let Some(selection) = selection {
            self.build.registry.configurations.insert(selection, id);
        }
        id
    }
    fn replace(&mut self, id: u32, draft: WeaponConfigurationDraft) {
        assert!(
            self.build.set_configuration_draft(id, draft),
            "native preparation replaces an existing row in its family"
        );
    }
}
impl WeaponBuild {
    pub fn prepare_configurations(
        &mut self,
    ) -> Vec<(crate::FamilyId, ConfigurationPreparationCensus)> {
        let mut results = Vec::new();
        for (family, prepare) in crate::weapon_policy::weapon_preparation_families() {
            let families = crate::WeaponFamilies::build(&self.family_tables, &self.registry);
            let mut content = Content {
                build: self,
                families,
            };
            let result = prepare(&mut content);
            content.build.registry.rebuild_name_maps();
            content.build.registry.revision = mint_weapon_revision();
            results.push((family, result));
        }
        results
    }
}
