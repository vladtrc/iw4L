use asset_anim::AnimationFamily;
use asset_audio::AudioFamily;
use asset_core::{FamilyId, ZoneFormat};
use asset_material::material_bindings::MaterialBindingFamily;
use asset_material::material_compile::compiler::MaterialCompiler;
use asset_model::ModelFamily;
use asset_transport::zone::ZoneDecoder;
use game_family::GameFamily;
use render_material::MaterialAdapter;
use sim::model_behaviors::ModelBehavior;
use sim::script::families::ScriptFamily;

pub const ID: FamilyId = FamilyId::new("t5");

pub struct Family;

impl ZoneFormat for Family {
    fn id(&self) -> FamilyId {
        ID
    }
    fn zone_version(&self) -> u32 {
        ext_t5_fastfile::ZONE_VERSION_PC
    }
    fn accepts_zone_header(&self, header: &[u8]) -> bool {
        header.len() >= 12
            && (header.starts_with(ext_t5_fastfile::MAGIC_SIGNED)
                || header.starts_with(ext_t5_fastfile::MAGIC_UNSIGNED))
            && header[8..12] == self.zone_version().to_le_bytes()
    }
    fn anchor_zone(&self) -> &'static str {
        "common_mp"
    }
    fn localized_zones(&self, language: &str) -> Vec<String> {
        vec![
            format!("{language}_code_post_gfx_mp"),
            format!("{language}_common_mp"),
            format!("{language}_ui_mp"),
        ]
    }
}

impl GameFamily for Family {
    fn bot_weapons(&self) -> &'static [bots::unique_loadout::UniqueWeapon] {
        crate::bots::WEAPONS
    }
    fn game_folder(&self) -> Option<asset_core::GameFolderFamily> {
        Some(asset_core::GameFolderFamily {
            key: "black_ops",
            title: "Black Ops",
            order: 10,
        })
    }
    fn common_sources(&self) -> assets::CommonSourceFamily {
        crate::lane::common_sources::POLICY
    }
    fn map_scripts(&self) -> assets::MapScriptFamily {
        crate::lane::map_script_policy::POLICY
    }
    fn lane(&self) -> &'static dyn assets::ZoneLane {
        &crate::lane::t5::T5Lane
    }
    fn zone_decoder(&self) -> ZoneDecoder {
        crate::zone::decoder::decode_zone
    }
    fn models(&self) -> ModelFamily {
        crate::model::policy::POLICY
    }
    fn weapons(&self) -> asset_game::WeaponFamilyPolicy {
        crate::weapons::weapon_policy::POLICY
    }
    fn map_presentation(&self) -> asset_game::MapPresentationFamily {
        crate::presentation::map_presentation::POLICY
    }
    fn teams(&self) -> asset_game::TeamFamily {
        crate::presentation::teams::POLICY
    }
    fn impact_fx(&self) -> asset_game::ImpactRowResolver {
        crate::presentation::impact_fx::impact_row
    }
    fn localization(&self) -> asset_game::LocalizationFamily {
        crate::presentation::localize::POLICY
    }
    fn world(&self) -> asset_world::WorldFamily {
        crate::world::policy::POLICY
    }
    fn material_compiler(&self) -> &'static dyn MaterialCompiler {
        &crate::material::compiler::T5Compiler
    }
    fn material_bindings(&self) -> MaterialBindingFamily {
        crate::material::material_bindings::POLICY
    }
    fn material_adapter(&self) -> Option<&'static dyn MaterialAdapter> {
        Some(&render_material::Sm3MaterialAdapter)
    }
    fn audio(&self) -> AudioFamily {
        crate::sound::policy::POLICY
    }
    fn animation(&self) -> AnimationFamily {
        crate::anim::policy::POLICY
    }
    fn script(&self) -> Option<ScriptFamily> {
        Some(crate::script::script_registration())
    }
    fn model_behavior(&self) -> Option<&'static dyn ModelBehavior> {
        Some(&crate::script::destructible::DestructibleBehavior)
    }
}
