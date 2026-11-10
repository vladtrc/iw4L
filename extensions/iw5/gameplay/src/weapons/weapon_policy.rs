use asset_game::*;

pub const POLICY: WeaponFamilyPolicy = WeaponFamilyPolicy {
    unlock: super::weapon_unlock::DECODER,
    camouflage: super::weapon_camouflage::POLICY,
    runtime: super::weapon_runtime::POLICY,
    tables: super::weapon_tables::POLICY,
    preparation: Some(super::weapon_catalog::preparation::prepare),
    attachment_hide_tags: Some(super::weapon_catalog::preparation::hide_tags),
    configuration: super::weapon_catalog::configuration::compile,
    attachments: Some(super::weapon_catalog::attachments::POLICY),
    semantics: reference_weapon_semantics,
    authored_name: joined_weapon_name,
    authored_attachments: parse_joined_weapon_attachments,
};
