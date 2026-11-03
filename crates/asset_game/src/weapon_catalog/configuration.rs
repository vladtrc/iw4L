use super::{WeaponRegistry, normalize_weapon_name};
use crate::{ConfigurationRefusal, FamilyKey, WeaponFamily, WeaponSelection};

pub(crate) trait WeaponConfigurationCompiler {
    fn compile(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal>;
}

pub(crate) fn authored_attachments(key: &FamilyKey, name: &str) -> Option<Vec<String>> {
    (crate::weapon_policy(key.namespace)?.authored_attachments)(&key.base, name)
}

impl WeaponRegistry {
    pub(super) fn compile_native_configuration(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal> {
        let policy = crate::weapon_policy(family.key.namespace)
            .ok_or_else(|| ConfigurationRefusal::UnknownFamily(family.key.to_string()))?;
        (policy.configuration)(self, family, selection)
    }
}

impl WeaponConfigurationCompiler for WeaponRegistry {
    fn compile(
        &self,
        family: &WeaponFamily,
        selection: &WeaponSelection,
    ) -> Result<u32, ConfigurationRefusal> {
        let id = self.compile_native_configuration(family, selection)?;
        self.configuration_admission(id)?;
        Ok(id)
    }
}

impl crate::weapon_families::FamilyContent for WeaponRegistry {
    fn published_handle(&self, id: u32) -> Option<crate::WeaponHandle> {
        self.bind_published_row(id)
            .ok()
            .map(|weapon| weapon.handle())
    }
    fn lookup(&self, namespace: crate::FamilyId, name: &str) -> Option<u32> {
        self.by_namespaced
            .get(&(namespace, normalize_weapon_name(name)))
            .copied()
    }

    fn offhand_class(&self, id: u32) -> i32 {
        self.facts_of(id).map_or(0, |facts| facts.offhand_class)
    }

    fn admission(&self, id: u32) -> Result<(), crate::ConfigurationRefusal> {
        self.configuration_admission(id)
    }

    fn prepared_all(&self) -> Vec<(u32, crate::WeaponSelection)> {
        self.configurations
            .iter()
            .map(|(selection, &id)| (id, selection.clone()))
            .collect()
    }

    fn names_in(&self, namespace: crate::FamilyId) -> Vec<(u32, String)> {
        self.published_weapons()
            .map(|weapon| weapon.wire_id())
            .filter(|&id| id != 0)
            .filter(|&id| self.identity_namespace_of(id) == Some(namespace))
            .filter(|&id| self.rows[id as usize].indexed_name)
            .map(|id| (id, normalize_weapon_name(self.name_of(id))))
            .collect()
    }
}

pub(super) fn compile_completion_names(registry: &WeaponRegistry) -> Vec<String> {
    use crate::FamilySlot;
    let mut names: Vec<String> = registry
        .weapon_families()
        .offered()
        .filter(|family| matches!(family.slot, FamilySlot::Primary | FamilySlot::Secondary))
        .filter(|family| {
            family
                .base
                .is_some_and(|id| registry.gun_xmodel_of(id).is_some())
        })
        .map(|family| family.key.short())
        .collect();
    names.extend(
        registry
            .published_weapons()
            .filter(|weapon| weapon.wire_id() != 0)
            .filter_map(|weapon| {
                let id = weapon.wire_id();
                if registry.describe_configuration(id).is_some()
                    || registry.gun_xmodel_of(id).is_none()
                    || registry.configuration_admission(id).is_err()
                {
                    return None;
                }
                let facts = weapon.hud_facts()?;
                if !facts.is_primary() || facts.offhand_class != 0 {
                    return None;
                }
                let key =
                    crate::FamilyKey::new(registry.host_namespace_of(id)?, registry.name_of(id));
                if registry.weapon_families().families().iter().any(|family| {
                    family.key.namespace == key.namespace
                        && (key.base == family.key.base
                            || key
                                .base
                                .strip_prefix(&family.key.base)
                                .is_some_and(|suffix| suffix.starts_with('_') || suffix == "dw"))
                }) {
                    return None;
                }
                Some(key.short())
            }),
    );
    names.sort();
    names.dedup();
    names
}

impl crate::WeaponConfigurationContent for WeaponRegistry {
    fn authored(&self, namespace: crate::FamilyId, name: &str) -> Option<u32> {
        self.by_namespaced
            .get(&(namespace, name.to_owned()))
            .copied()
    }
    fn prepared(&self, selection: &crate::WeaponSelection) -> Option<u32> {
        self.configurations.get(selection).copied()
    }
    fn native_admission(&self, id: u32) -> Result<(), ConfigurationRefusal> {
        self.native_configuration_admission(id)
    }
    fn name(&self, id: u32) -> &str {
        self.name_of(id)
    }
    fn namespace(&self, id: u32) -> Option<crate::FamilyId> {
        self.identity_namespace_of(id)
    }
    fn attachment_assets(&self, id: u32) -> Option<&[Option<String>]> {
        Some(&self.rows.get(id as usize)?.attachment_assets)
    }
    fn attachments(&self) -> &crate::WeaponAttachments {
        &self.attachments
    }
}
