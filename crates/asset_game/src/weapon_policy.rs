use crate::{WeaponSemanticCompiler, reference_weapon_semantics};
use asset_core::FamilyId;

#[derive(Clone, Copy)]
pub struct WeaponFamilyPolicy {
    pub unlock: crate::WeaponUnlockDecoder,
    pub camouflage: crate::WeaponCamouflagePolicy,
    pub runtime: crate::WeaponRuntimePolicy,
    pub tables: crate::WeaponTableFamily,
    pub semantics: WeaponSemanticCompiler,
    pub preparation: Option<crate::WeaponConfigurationPreparer>,
    pub attachment_hide_tags: Option<fn(&crate::WeaponNativeCapture) -> Vec<String>>,
    pub configuration: crate::WeaponConfigurationCompiler,
    pub attachments: Option<crate::WeaponAttachmentPolicy>,
    pub authored_name: fn(&str, &[String]) -> String,
    pub authored_attachments: fn(&str, &str) -> Option<Vec<String>>,
}

pub fn joined_weapon_name(base: &str, attachments: &[String]) -> String {
    if attachments.is_empty() {
        base.to_owned()
    } else {
        format!("{base}_{}", attachments.join("_"))
    }
}

pub fn parse_joined_weapon_attachments(base: &str, name: &str) -> Option<Vec<String>> {
    let name = name.strip_suffix("_mp").unwrap_or(name);
    if name == base {
        return Some(Vec::new());
    }
    let rest = name.strip_prefix(base)?.strip_prefix('_')?;
    Some(rest.split('_').map(str::to_owned).collect())
}

pub const REFERENCE_WEAPON_POLICY: WeaponFamilyPolicy = WeaponFamilyPolicy {
    semantics: reference_weapon_semantics,
    tables: crate::REFERENCE_WEAPON_TABLES,
    runtime: crate::REFERENCE_WEAPON_RUNTIME,
    unlock: crate::reference_weapon_unlock,
    camouflage: crate::WeaponCamouflagePolicy {
        model: None,
        materials: None,
    },
    configuration: crate::reference_weapon_configuration,
    preparation: None,
    attachment_hide_tags: None,
    attachments: None,
    authored_name: joined_weapon_name,
    authored_attachments: parse_joined_weapon_attachments,
};

#[derive(Default)]
pub struct WeaponPolicies(std::collections::BTreeMap<FamilyId, WeaponFamilyPolicy>);
impl WeaponPolicies {
    pub fn register(&mut self, family: FamilyId, policy: WeaponFamilyPolicy) {
        assert!(
            self.0.insert(family, policy).is_none(),
            "duplicate weapon policy"
        );
    }
}
static POLICIES: std::sync::OnceLock<WeaponPolicies> = std::sync::OnceLock::new();
pub fn install_weapon_policies(policies: WeaponPolicies) -> Result<(), &'static str> {
    POLICIES
        .set(policies)
        .map_err(|_| "weapon policies already installed")
}
pub fn weapon_policy(family: FamilyId) -> Option<&'static WeaponFamilyPolicy> {
    POLICIES.get()?.0.get(&family)
}

pub(crate) fn weapon_preparation_families() -> Vec<(FamilyId, crate::WeaponConfigurationPreparer)> {
    POLICIES
        .get()
        .expect("weapon policies must be installed before preparation")
        .0
        .iter()
        .filter_map(|(&id, policy)| Some((id, policy.preparation?)))
        .collect()
}

pub fn weapon_policies() -> impl Iterator<Item = (FamilyId, &'static WeaponFamilyPolicy)> {
    POLICIES
        .get()
        .into_iter()
        .flat_map(|policies| policies.0.iter())
        .map(|(id, policy)| (*id, policy))
}
pub fn supplemental_weapon_tables(
    namespace: FamilyId,
    root: &asset_transport::GamesRoot,
) -> Result<crate::WeaponSupplementalTables, &'static str> {
    let policy = weapon_policy(namespace).ok_or("weapon table policy not installed")?;
    Ok(policy
        .tables
        .supplemental
        .map(|capture| capture(root))
        .unwrap_or_else(crate::WeaponSupplementalTables::default))
}
