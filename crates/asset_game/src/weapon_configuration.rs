use crate::{ConfigurationRefusal, FamilyId, WeaponAttachments, WeaponFamily, WeaponSelection};

/// Captured and prepared content exposed to a game's configuration admission rules.
pub trait WeaponConfigurationContent {
    fn authored(&self, namespace: FamilyId, name: &str) -> Option<u32>;
    fn prepared(&self, selection: &WeaponSelection) -> Option<u32>;
    fn native_admission(&self, id: u32) -> Result<(), ConfigurationRefusal>;
    fn name(&self, id: u32) -> &str;
    fn namespace(&self, id: u32) -> Option<FamilyId>;
    fn attachment_assets(&self, id: u32) -> Option<&[Option<String>]>;
    fn attachments(&self) -> &WeaponAttachments;
}

pub type WeaponConfigurationCompiler = fn(
    &dyn WeaponConfigurationContent,
    &WeaponFamily,
    &WeaponSelection,
) -> Result<u32, ConfigurationRefusal>;

pub fn compile_authored_weapon_configuration(
    content: &dyn WeaponConfigurationContent,
    family: &WeaponFamily,
    name: String,
) -> Result<u32, ConfigurationRefusal> {
    let id = content
        .authored(family.key.namespace, &name)
        .ok_or_else(|| ConfigurationRefusal::MissingContent(format!("{name}_mp")))?;
    content.native_admission(id)?;
    Ok(id)
}

pub fn reference_weapon_configuration(
    content: &dyn WeaponConfigurationContent,
    family: &WeaponFamily,
    selection: &WeaponSelection,
) -> Result<u32, ConfigurationRefusal> {
    compile_authored_weapon_configuration(
        content,
        family,
        crate::joined_weapon_name(&family.key.base, &selection.attachments),
    )
}
