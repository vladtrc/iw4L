use super::extensions::RoundExtensions;
use super::{Builtin, Catalog, NativeRegistry, RoundScript};
use crate::frame::FrameWorld;
use asset_core::FamilyId;
use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::OnceLock;

pub struct ScriptPolicy {
    pub default_classes: &'static [&'static str],
    pub custom_bot_classes: bool,
    pub native_class_response: Option<fn(crate::ClassId) -> Option<String>>,
    pub foreign_weapon: fn(&FrameWorld, usize, u32) -> Option<u32>,
    pub integer_entity_fields: &'static [&'static str],
    pub model_argument: usize,
    pub decode_weapon_model: fn(i32) -> u8,
    pub akimbo_argument: Option<usize>,
}
fn iw4_foreign_weapon(frame: &FrameWorld, slot: usize, weapon: u32) -> Option<u32> {
    if frame
        .weapon_combat_row(weapon)
        .is_some_and(|facts| facts.weap_type == weapon_iw4::WEAPTYPE_SHIELD)
    {
        return frame.weapon_index_by_script_name("riotshield_mp");
    }
    if let Some(facts) = frame
        .missile_launch_facts(weapon)
        .filter(|facts| facts.require_lock_to_fire)
    {
        let launcher = match facts.missile_guidance {
            1 => Some("stinger_mp"),
            3 => Some("javelin_mp"),
            _ => None,
        };
        if let Some(launcher) = launcher {
            return frame.weapon_index_by_script_name(launcher);
        }
    }
    let names = ["m4_mp", "usp_mp", "frag_grenade_mp", "flash_grenade_mp"];
    frame.weapon_index_by_script_name(names[slot])
}
pub static IW4_SCRIPT_POLICY: ScriptPolicy = ScriptPolicy {
    default_classes: &["class0", "class1", "class2", "class3", "class4"],
    custom_bot_classes: true,
    native_class_response: None,
    foreign_weapon: iw4_foreign_weapon,
    integer_entity_fields: &[],
    model_argument: 1,
    decode_weapon_model: |model| model.clamp(0, 15) as u8,
    akimbo_argument: Some(2),
};
pub struct ScriptFamily {
    pub catalog: &'static [Builtin],
    pub policy: &'static ScriptPolicy,
    pub register_natives: fn(&mut NativeRegistry),
    pub initialize_round: fn(&mut RoundExtensions),
}
impl ScriptFamily {
    pub fn iw4() -> Self {
        Self {
            catalog: super::profile::iw4_builtins(),
            policy: &IW4_SCRIPT_POLICY,
            register_natives: |_| {},
            initialize_round: |_| {},
        }
    }
}
#[derive(Default)]
pub struct ScriptFamilies(BTreeMap<FamilyId, ScriptFamily>);
impl ScriptFamilies {
    pub fn register(&mut self, id: FamilyId, family: ScriptFamily) {
        assert!(
            !family.policy.default_classes.is_empty(),
            "script family needs default classes"
        );
        assert!(!self.0.contains_key(&id), "duplicate script family");
        self.0.insert(id, family);
    }
}
static FAMILIES: OnceLock<ScriptFamilies> = OnceLock::new();
pub fn install_script_families(families: ScriptFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "script families already installed")
}
pub fn script_family(id: FamilyId) -> Result<&'static ScriptFamily, String> {
    FAMILIES
        .get()
        .and_then(|families| families.0.get(&id))
        .ok_or_else(|| format!("script family {} is not registered", id.as_str()))
}
pub fn catalog_for_family(id: FamilyId) -> Result<Catalog, String> {
    let family = script_family(id)?;
    Ok(Catalog::from_list(id, family.catalog))
}
pub(crate) fn policy_for_round(round: &RoundScript) -> &'static ScriptPolicy {
    match round.program.as_ref() {
        Some(program) => {
            script_family(program.rules())
                .expect("installed script family")
                .policy
        }
        None => &IW4_SCRIPT_POLICY,
    }
}
pub fn policy_for_world(world: &World) -> &'static ScriptPolicy {
    policy_for_round(world.resource::<RoundScript>())
}
