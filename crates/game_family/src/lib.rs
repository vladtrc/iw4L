//! The one extension point for a game. A family is a single value that
//! answers every hook; the builder hands the list to [`install`], which fills
//! each subsystem's registry. Adding a family is one crate and one entry in
//! that list — a hook the family forgot is a compile error, not a refusal at
//! render time.
//!
//! Hooks without a default are deliberate: when a family reuses the
//! reference behaviour it says so by returning the `REFERENCE_*` value, and
//! when a hook does not apply it returns `None` with the reason on the method.

use asset_anim::AnimationFamily;
use asset_audio::AudioFamily;
use asset_core::{FamilyId, FamilyRegistry, ZoneFormat};
use asset_material::material_bindings::MaterialBindingFamily;
use asset_material::material_compile::compiler::MaterialCompiler;
use asset_model::ModelFamily;
use asset_transport::zone::ZoneDecoder;
use render_material::MaterialAdapter;
use sim::model_behaviors::ModelBehavior;
use sim::script::families::ScriptFamily;

pub trait GameFamily: ZoneFormat {
    fn bot_weapons(&self) -> &'static [bots::unique_loadout::UniqueWeapon];
    /// None when the launcher provides this family's root directly.
    fn game_folder(&self) -> Option<asset_core::GameFolderFamily>;
    fn common_sources(&self) -> assets::CommonSourceFamily;
    fn map_scripts(&self) -> assets::MapScriptFamily;
    fn lane(&self) -> &'static dyn assets::ZoneLane;
    fn zone_decoder(&self) -> ZoneDecoder;
    fn models(&self) -> ModelFamily;
    fn world(&self) -> asset_world::WorldFamily;
    fn localization(&self) -> asset_game::LocalizationFamily;
    fn teams(&self) -> asset_game::TeamFamily;
    fn map_presentation(&self) -> asset_game::MapPresentationFamily;
    fn impact_fx(&self) -> asset_game::ImpactRowResolver;
    fn weapons(&self) -> asset_game::WeaponFamilyPolicy;
    fn material_compiler(&self) -> &'static dyn MaterialCompiler;
    fn material_bindings(&self) -> MaterialBindingFamily;
    /// Adapters are keyed by the owner of a vertex layout. `None` when this
    /// family's declarations use another family's layout, whose adapter then
    /// prepares them (IW5 captures with the IW4 layout).
    fn material_adapter(&self) -> Option<&'static dyn MaterialAdapter>;
    fn audio(&self) -> AudioFamily;
    fn animation(&self) -> AnimationFamily;
    /// `None` when the family ships no script catalog; its maps run under
    /// another family's rules.
    fn script(&self) -> Option<ScriptFamily>;
    /// Entity behaviour beyond the reference rules (T5 destructibles).
    fn model_behavior(&self) -> Option<&'static dyn ModelBehavior>;
}

/// IW4 is the reference family: it goes through the same install as every
/// other game, with the reference value for each hook.
pub struct Iw4Family;

impl ZoneFormat for Iw4Family {
    fn id(&self) -> FamilyId {
        FamilyId::IW4
    }
    fn zone_version(&self) -> u32 {
        0x114
    }
    fn accepts_zone_header(&self, header: &[u8]) -> bool {
        header.len() >= 12
            && header.starts_with(b"IWff")
            && header[8..12] == self.zone_version().to_le_bytes()
    }
    fn anchor_zone(&self) -> &'static str {
        "common_mp"
    }
    fn localized_zones(&self, _: &str) -> Vec<String> {
        vec!["localized_common_mp".to_owned()]
    }
}

impl GameFamily for Iw4Family {
    fn bot_weapons(&self) -> &'static [bots::unique_loadout::UniqueWeapon] {
        bots::unique_loadout::REFERENCE_BOT_WEAPONS
    }
    fn game_folder(&self) -> Option<asset_core::GameFolderFamily> {
        None
    }
    fn common_sources(&self) -> assets::CommonSourceFamily {
        assets::REFERENCE_COMMON_SOURCE_FAMILY
    }
    fn map_scripts(&self) -> assets::MapScriptFamily {
        assets::REFERENCE_MAP_SCRIPT_FAMILY
    }
    fn lane(&self) -> &'static dyn assets::ZoneLane {
        assets::REFERENCE_ZONE_LANE
    }
    fn zone_decoder(&self) -> ZoneDecoder {
        asset_transport::zone::parse_iw4_zone_image
    }
    fn models(&self) -> ModelFamily {
        asset_model::REFERENCE_MODEL_FAMILY
    }
    fn weapons(&self) -> asset_game::WeaponFamilyPolicy {
        asset_game::REFERENCE_WEAPON_POLICY
    }
    fn map_presentation(&self) -> asset_game::MapPresentationFamily {
        asset_game::REFERENCE_MAP_PRESENTATION
    }
    fn teams(&self) -> asset_game::TeamFamily {
        asset_game::REFERENCE_TEAM_FAMILY
    }
    fn impact_fx(&self) -> asset_game::ImpactRowResolver {
        asset_game::REFERENCE_IMPACT_ROW_RESOLVER
    }
    fn localization(&self) -> asset_game::LocalizationFamily {
        asset_game::REFERENCE_LOCALIZATION_FAMILY
    }
    fn world(&self) -> asset_world::WorldFamily {
        asset_world::REFERENCE_WORLD_FAMILY
    }
    fn material_compiler(&self) -> &'static dyn MaterialCompiler {
        &asset_material::material_compile::compiler::iw4::Iw4Compiler
    }
    fn material_bindings(&self) -> MaterialBindingFamily {
        asset_material::material_bindings::REFERENCE_MATERIAL_BINDINGS
    }
    fn material_adapter(&self) -> Option<&'static dyn MaterialAdapter> {
        Some(&render_material::Sm3MaterialAdapter)
    }
    fn audio(&self) -> AudioFamily {
        asset_audio::REFERENCE_AUDIO_FAMILY
    }
    fn animation(&self) -> AnimationFamily {
        asset_anim::REFERENCE_ANIMATION_FAMILY
    }
    fn script(&self) -> Option<ScriptFamily> {
        Some(ScriptFamily::iw4())
    }
    fn model_behavior(&self) -> Option<&'static dyn ModelBehavior> {
        None
    }
}

/// Fan the families out to every subsystem registry. Called once by the
/// builder before discovery or any asset load; the per-subsystem `install_*`
/// functions are not called from anywhere else.
pub fn install(families: &[&'static dyn GameFamily]) -> Result<(), &'static str> {
    let mut bot_weapons = bots::unique_loadout::BotWeaponFamilies::default();
    let mut common_sources = assets::CommonSourceFamilies::default();
    let mut map_scripts = assets::MapScriptFamilies::default();
    let mut lanes = assets::ZoneLanes::default();
    let mut zones = FamilyRegistry::default();
    let mut folders = asset_core::GameFolderFamilies::default();
    let mut decoders = asset_transport::ZoneDecoders::default();
    let mut models = asset_model::ModelFamilies::default();
    let mut worlds = asset_world::WorldFamilies::default();
    let mut localization = asset_game::LocalizationFamilies::default();
    let mut teams = asset_game::TeamFamilies::default();
    let mut map_presentation = asset_game::MapPresentationFamilies::default();
    let mut impacts = asset_game::ImpactFxFamilies::default();
    let mut weapons = asset_game::WeaponPolicies::default();
    let mut compilers = asset_material::material_compile::compiler::MaterialCompilers::default();
    let mut bindings = asset_material::material_bindings::MaterialBindingFamilies::default();
    let mut adapters = render_material::MaterialAdapters::default();
    let mut audio = asset_audio::AudioFamilies::default();
    let mut animation = asset_anim::AnimationFamilies::default();
    let mut scripts = sim::script::families::ScriptFamilies::default();
    let mut behaviors = sim::model_behaviors::ModelBehaviors::default();
    for &family in families {
        let id = family.id();
        bot_weapons.register(id, family.bot_weapons())?;
        if let Some(policy) = family.game_folder() {
            folders.register(id, policy);
        }
        common_sources.register(id, family.common_sources());
        map_scripts.register(id, family.map_scripts());
        lanes.register(id, family.lane())?;
        zones.register(family);
        decoders.register(id, family.zone_decoder());
        models.register(id, family.models());
        worlds.register(id, family.world());
        localization.register(id, family.localization());
        teams.register(id, family.teams());
        map_presentation.register(id, family.map_presentation());
        impacts.register(id, family.impact_fx());
        weapons.register(id, family.weapons());
        compilers.register(id, family.material_compiler());
        bindings.register(id, family.material_bindings());
        if let Some(adapter) = family.material_adapter() {
            adapters.register(id, adapter);
        }
        audio.register(id, family.audio());
        animation.register(id, family.animation());
        if let Some(script) = family.script() {
            scripts.register(id, script);
        }
        if let Some(behavior) = family.model_behavior() {
            behaviors.register(id, behavior);
        }
    }
    bots::unique_loadout::install_bot_weapon_families(bot_weapons)?;
    assets::install_common_source_families(common_sources)?;
    assets::install_map_script_families(map_scripts)?;
    assets::install_zone_lanes(lanes)?;
    asset_core::install_families(zones)?;
    asset_core::install_game_folder_families(folders)?;
    asset_transport::install_zone_decoders(decoders)?;
    asset_model::install_model_families(models)?;
    asset_world::install_world_families(worlds)?;
    asset_game::install_localization_families(localization)?;
    asset_game::install_team_families(teams)?;
    asset_game::install_map_presentation_families(map_presentation)?;
    asset_game::install_impact_fx_families(impacts)?;
    asset_game::install_weapon_policies(weapons)?;
    asset_material::material_compile::compiler::install_material_compilers(compilers)?;
    asset_material::material_bindings::install_material_binding_families(bindings)?;
    render_material::install_material_adapters(adapters)?;
    asset_audio::install_audio_families(audio)?;
    asset_anim::install_animation_families(animation)?;
    sim::script::families::install_script_families(scripts)?;
    sim::model_behaviors::install_model_behaviors(behaviors)?;
    Ok(())
}
