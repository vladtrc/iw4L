use crate::{FamilyId, WeaponAttachments, WeaponConfigurationDraft, WeaponSelection};

#[derive(Clone, Debug, Default)]
pub struct ConfigurationPreparationCensus {
    pub prepared: usize,
    pub refused: Vec<WeaponSelection>,
    pub alternates: usize,
}

/// Build-stage content and insertion operations; format-specific composition stays in the game.
pub trait WeaponPreparationContent {
    fn tables(&self) -> &[(FamilyId, crate::CapturedStringTable)];
    fn family(&self, key: &crate::FamilyKey) -> Option<crate::WeaponFamily>;
    fn authored(&self, family: FamilyId, name: &str) -> Option<u32>;
    fn candidates(&self, family: FamilyId) -> Vec<(u32, WeaponSelection)>;
    fn normalize(&self, family: FamilyId, attachments: &[String]) -> Vec<String>;
    fn draft(&self, id: u32) -> Option<WeaponConfigurationDraft>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn attachments(&self) -> &WeaponAttachments;
    fn append(
        &mut self,
        template: u32,
        combat_source: u32,
        selection: Option<WeaponSelection>,
        draft: WeaponConfigurationDraft,
    ) -> u32;
    fn replace(&mut self, id: u32, draft: WeaponConfigurationDraft);
}

pub type WeaponConfigurationPreparer =
    fn(&mut dyn WeaponPreparationContent) -> ConfigurationPreparationCensus;
