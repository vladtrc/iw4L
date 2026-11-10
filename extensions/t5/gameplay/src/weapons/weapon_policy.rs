use asset_game::*;

pub const POLICY: WeaponFamilyPolicy = WeaponFamilyPolicy {
    unlock: super::weapon_unlock::DECODER,
    camouflage: super::weapon_camouflage::POLICY,
    runtime: super::weapon_runtime::POLICY,
    tables: super::weapon_tables::POLICY,
    preparation: None,
    attachment_hide_tags: None,
    configuration: super::weapon_catalog::configuration::compile,
    attachments: None,
    semantics: compile_semantics,
    authored_name,
    authored_attachments,
};

fn compile_semantics(input: WeaponSemanticInputs<'_>) -> WeaponSemanticPolicy {
    WeaponSemanticPolicy {
        thermal_scope: input.overlay.is_some_and(|name| name.contains("_ir")),
        cue_namespace: input.namespace,
        melee_weapon: (!input.use_as_melee)
            .then(|| (input.find_weapon)("knife"))
            .flatten(),
        breath_cues: BreathCuePolicy {
            inhale: "wpn_sniper_breathin",
            exhale: "wpn_sniper_breathout",
            gasp: "wpn_sniper_breathgasp",
            heartbeat: "wpn_sniper_heartbeat",
        },
        melee_cues: MeleeCuePolicy {
            precedence: MeleeCuePrecedence::AuthoredFirst,
            knife_hit: "wpn_melee_knife_hit_body",
            knife_miss: "wpn_melee_knife_hit_other",
            generic_hit: "wpn_melee_hit",
            generic_miss: "wpn_melee_hit_other",
        },
    }
}
fn authored_name(base: &str, attachments: &[String]) -> String {
    if attachments == ["dw"] {
        format!("{base}dw")
    } else {
        joined_weapon_name(base, attachments)
    }
}
fn authored_attachments(base: &str, name: &str) -> Option<Vec<String>> {
    let bare = name.strip_suffix("_mp").unwrap_or(name);
    if bare == base {
        return Some(Vec::new());
    }
    if bare == format!("{base}dw") {
        return Some(vec!["dw".to_owned()]);
    }
    parse_joined_weapon_attachments(base, name)
}
