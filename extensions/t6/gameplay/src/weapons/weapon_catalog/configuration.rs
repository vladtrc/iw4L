use asset_game::*;

pub fn compile(
    content: &dyn WeaponConfigurationContent,
    family: &WeaponFamily,
    selection: &WeaponSelection,
) -> Result<u32, ConfigurationRefusal> {
    let policy = super::super::weapon_policy::POLICY;
    compile_authored_weapon_configuration(
        content,
        family,
        (policy.authored_name)(&family.key.base, &selection.attachments),
    )
}
