use asset_game::{
    WeaponAttachmentData, WeaponAttachmentPolicy, WeaponAttachments, WeaponReticleAssets,
};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Attachment {
    pub display_name: Option<String>,
    pub weapon_type: i32,
    pub weapon_class: i32,
    pub overlay: Option<String>,
    pub view_models: [Option<String>; ext_iw5_fastfile::size::ATTACH_MODEL_COUNT],
    pub world_models: [Option<String>; ext_iw5_fastfile::size::ATTACH_MODEL_COUNT],
    pub reticle_models: [Option<String>; ext_iw5_fastfile::size::ATTACH_RETICLE_COUNT],
    pub overlay_reticle: i32,
    pub thermal: bool,
    pub width: f32,
    pub height: f32,
    pub sight: Option<ext_iw5_fastfile::AttachmentSight>,
    pub ammo_general: Option<ext_iw5_fastfile::AttachmentAmmoGeneral>,
    pub reload: Option<ext_iw5_fastfile::AttachmentReload>,
    pub add_ons: Option<ext_iw5_fastfile::AttachmentAddOns>,
    pub general: Option<ext_iw5_fastfile::AttachmentGeneral>,
    pub ammunition: Option<ext_iw5_fastfile::AttachmentAmmunition>,
    pub damage: Option<ext_iw5_fastfile::AttachmentDamage>,
    pub projectile: Option<ext_iw5_fastfile::AttachmentProjectile>,
    pub projectile_model: Option<String>,
    pub reticle: Option<WeaponReticleAssets>,
    pub projectile_explosion_fx: Option<String>,
    pub projectile_trail_fx: Option<String>,
    pub projectile_ignition_fx: Option<String>,
    pub projectile_explosion_sound: Option<String>,
    pub projectile_ignition_sound: Option<String>,
    pub location_damage: Option<[f32; 19]>,
    pub idle_settings: Option<ext_iw5_fastfile::AttachmentIdleSettings>,
    pub ads_settings: Option<ext_iw5_fastfile::AttachmentAdsSettings>,
    pub ads_settings_main: Option<ext_iw5_fastfile::AttachmentAdsSettings>,
    pub hip_spread: Option<ext_iw5_fastfile::AttachmentHipSpread>,
    pub gun_kick: Option<ext_iw5_fastfile::AttachmentGunKick>,
    pub view_kick: Option<[f32; 10]>,
    pub scales: ext_iw5_fastfile::AttachmentScales,
    pub share_ammo_with_alt: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AttachmentLibrary {
    pub entries: HashMap<String, Attachment>,
}
impl WeaponAttachmentData for AttachmentLibrary {
    fn clone_box(&self) -> Box<dyn WeaponAttachmentData> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any + Send + Sync> {
        self
    }
    fn absorb(&mut self, donor: Box<dyn WeaponAttachmentData>) {
        let donor = donor
            .into_any()
            .downcast::<Self>()
            .unwrap_or_else(|_| panic!("IW5 attachment schema mismatch"));
        self.entries.extend(donor.entries);
    }
}
pub const POLICY: WeaponAttachmentPolicy = WeaponAttachmentPolicy {
    default_models,
    shared_ammo: |store, name| {
        store
            .get::<AttachmentLibrary>(crate::ID)
            .and_then(|library| library.entries.get(name))
            .is_some_and(|asset| asset.share_ammo_with_alt)
    },
};
fn default_models(
    store: &WeaponAttachments,
    slots: &[Option<String>],
) -> Option<(Option<String>, Option<String>)> {
    let library = store.get::<AttachmentLibrary>(crate::ID)?;
    slots
        .get(..6)?
        .iter()
        .filter_map(Option::as_deref)
        .find_map(|name| {
            if !name.ends_with("scope") || name.ends_with("vzscope") {
                return None;
            }
            let asset = library.entries.get(name)?;
            let view = asset.view_models[0].clone();
            let world = asset.world_models[0].clone();
            (view.is_some() || world.is_some()).then_some((view, world))
        })
}
pub fn capture(
    store: &mut WeaponAttachments,
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::AttachmentGeometry,
    fx_name_at_slot: &dyn Fn(ext_iw5_fastfile::Ptr) -> Option<String>,
) {
    let Some(name) = geometry.name.and_then(|ptr| read_string(stream, ptr)) else {
        return;
    };
    let facts = &geometry.facts;
    let projectile_fx = |x86, x64| {
        let body = facts.projectile?.body;
        match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
            ext_iw5_fastfile::ZonePtr::Offset(slot) => fx_name_at_slot(stream.resolve_alias(slot)),
            _ => None,
        }
    };
    let projectile_sound = |x86, x64| sound_alias(stream, facts.projectile?.body, x86, x64);
    store
        .get_or_insert::<AttachmentLibrary>(crate::ID)
        .entries
        .insert(
            name,
            Attachment {
                display_name: facts.display_name.and_then(|ptr| read_string(stream, ptr)),
                weapon_type: facts.weapon_type,
                weapon_class: facts.weapon_class,
                overlay: geometry.overlay_names[0].and_then(|ptr| read_string(stream, ptr)),
                view_models: geometry
                    .view_model_names
                    .map(|ptr| ptr.and_then(|p| read_string(stream, p))),
                world_models: geometry
                    .world_model_names
                    .map(|ptr| ptr.and_then(|p| read_string(stream, p))),
                reticle_models: geometry
                    .reticle_model_names
                    .map(|ptr| ptr.and_then(|p| read_string(stream, p))),
                overlay_reticle: geometry.overlay_reticle,
                thermal: geometry.thermal,
                width: geometry.overlay_width,
                height: geometry.overlay_height,
                sight: facts.sight,
                ammo_general: facts.ammo_general,
                reload: facts.reload,
                add_ons: facts.add_ons,
                general: facts.general,
                reticle: facts.general.and_then(|general| general.body).map(|body| {
                    let center = geometry
                        .reticle_center_name
                        .and_then(|name| read_string(stream, name));
                    let side = geometry
                        .reticle_side_name
                        .and_then(|name| read_string(stream, name));
                    WeaponReticleAssets {
                        center_authored: center.is_some(),
                        side_authored: side.is_some(),
                        center_material: center,
                        side_material: side,
                        center_size: stream.i32_at(body, stream.layout(16, 24)).unwrap_or(0),
                        side_size: stream.i32_at(body, stream.layout(20, 28)).unwrap_or(0),
                        ..Default::default()
                    }
                }),
                ammunition: facts.ammunition,
                damage: facts.damage,
                projectile: facts.projectile,
                projectile_explosion_fx: projectile_fx(40, 48),
                projectile_trail_fx: projectile_fx(76, 104),
                projectile_ignition_fx: projectile_fx(84, 120),
                projectile_explosion_sound: projectile_sound(48, 64),
                projectile_ignition_sound: projectile_sound(88, 128),
                projectile_model: facts.projectile.and_then(|p| p.model).and_then(|model| {
                    match stream.ptr_at(model, 0).ok()? {
                        ext_iw5_fastfile::ZonePtr::Offset(name) => {
                            read_string(stream, stream.resolve_alias(name))
                        }
                        _ => None,
                    }
                }),
                location_damage: facts.location_damage,
                idle_settings: facts.idle_settings,
                ads_settings: facts.ads_settings,
                ads_settings_main: facts.ads_settings_main,
                hip_spread: facts.hip_spread,
                gun_kick: facts.gun_kick,
                view_kick: facts.view_kick,
                scales: facts.scales,
                share_ammo_with_alt: facts.share_ammo_with_alt,
            },
        );
}
fn read_string(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    ptr: ext_iw5_fastfile::Ptr,
) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}
fn sound_alias(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> Option<String> {
    let wrapper = match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
        ext_iw5_fastfile::ZonePtr::Offset(ptr) => stream.resolve_alias(ptr),
        _ => return None,
    };
    let name = match stream.ptr_at(wrapper, 0).ok()? {
        ext_iw5_fastfile::ZonePtr::Offset(ptr) => stream.resolve_alias(ptr),
        _ => wrapper,
    };
    read_string(stream, name)
}
