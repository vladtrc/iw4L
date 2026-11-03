pub mod host;
pub mod profile;

pub fn registration() -> sim::script::families::ScriptFamily {
    sim::script::families::ScriptFamily {
        catalog: profile::t5_catalog::T5,
        policy: &POLICY,
        register_natives: host::natives::t5::register,
        initialize_round: |extensions| {
            extensions.state_mut::<host::natives::t5::T5State>();
        },
    }
}
fn class_response(class: sim::ClassId) -> Option<String> {
    match class.0 {
        slot @ 0..5 => Some(format!("custom{}", slot + 1)),
        slot @ 5..10 => Some(format!("prestige{}", slot - 4)),
        _ => None,
    }
}
fn foreign_weapon(frame: &sim::frame::FrameWorld, slot: usize, _: u32) -> Option<u32> {
    frame.weapon_index_by_script_name(
        ["ak47_mp", "m1911_mp", "frag_grenade_mp", "flash_grenade_mp"][slot],
    )
}
static POLICY: sim::script::families::ScriptPolicy = sim::script::families::ScriptPolicy {
    default_classes: &["smg_mp", "cqb_mp", "assault_mp", "lmg_mp", "sniper_mp"],
    custom_bot_classes: false,
    native_class_response: Some(class_response),
    foreign_weapon,
    integer_entity_fields: &["script_exploder"],
    model_argument: 2,
    decode_weapon_model: |model| (model & 63) as u8,
    akimbo_argument: None,
};
