use asset_game::*;

pub const POLICY: WeaponFamilyPolicy = WeaponFamilyPolicy {
    unlock: super::weapon_unlock::DECODER,
    camouflage: super::weapon_camouflage::POLICY,
    runtime: super::weapon_runtime::POLICY,
    tables: super::weapon_tables::POLICY,
    preparation: Some(super::weapon_catalog::preparation::prepare),
    attachment_hide_tags: None,
    configuration: super::weapon_catalog::configuration::compile,
    attachments: None,
    semantics: compile_semantics,
    authored_name: joined_weapon_name,
    authored_attachments: parse_joined_weapon_attachments,
};

fn compile_semantics(input: WeaponSemanticInputs<'_>) -> WeaponSemanticPolicy {
    let mut policy = reference_weapon_semantics(input);
    // This family's existing breath cues are supplied by the reference bank.
    policy.cue_namespace = asset_core::FamilyId::IW4;
    policy
}
