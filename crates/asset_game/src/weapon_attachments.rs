use asset_core::FamilyId;
use std::any::Any;

/// Owned native attachment data. The format owner defines how donor data merges.
pub trait WeaponAttachmentData: Any + std::fmt::Debug + Send + Sync {
    fn clone_box(&self) -> Box<dyn WeaponAttachmentData>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn into_any(self: Box<Self>) -> Box<dyn Any + Send + Sync>;
    fn absorb(&mut self, donor: Box<dyn WeaponAttachmentData>);
}
impl Clone for Box<dyn WeaponAttachmentData> {
    fn clone(&self) -> Self {
        self.as_ref().clone_box()
    }
}

#[derive(Clone, Debug, Default)]
pub struct WeaponAttachments(std::collections::BTreeMap<FamilyId, Box<dyn WeaponAttachmentData>>);
impl WeaponAttachments {
    pub fn get<T: WeaponAttachmentData>(&self, family: FamilyId) -> Option<&T> {
        self.0.get(&family)?.as_any().downcast_ref()
    }
    pub fn get_or_insert<T: WeaponAttachmentData + Default>(&mut self, family: FamilyId) -> &mut T {
        self.0
            .entry(family)
            .or_insert_with(|| Box::new(T::default()))
            .as_any_mut()
            .downcast_mut()
            .expect("attachment data type changed within a family")
    }
    pub fn absorb(&mut self, donor: Self) {
        for (family, data) in donor.0 {
            match self.0.entry(family) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(data);
                }
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().absorb(data)
                }
            }
        }
    }
    pub fn default_models(
        &self,
        family: FamilyId,
        slots: &[Option<String>],
    ) -> Option<(Option<String>, Option<String>)> {
        let policy = crate::weapon_policy(family)?.attachments?;
        (policy.default_models)(self, slots)
    }
    pub fn shared_ammo(&self, family: FamilyId, name: &str) -> bool {
        crate::weapon_policy(family)
            .and_then(|policy| policy.attachments)
            .is_some_and(|policy| (policy.shared_ammo)(self, name))
    }
}

#[derive(Clone, Copy)]
pub struct WeaponAttachmentPolicy {
    pub default_models:
        fn(&WeaponAttachments, &[Option<String>]) -> Option<(Option<String>, Option<String>)>,
    pub shared_ammo: fn(&WeaponAttachments, &str) -> bool,
}
