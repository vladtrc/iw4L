use crate::size as sz;
use crate::zone::{Ptr, ZonePtr, ZoneStream};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentFacts {
    pub display_name: Option<Ptr>,
    pub attachment_type: i32,
    pub weapon_type: i32,
    pub weapon_class: i32,
    pub load_index: i32,
    pub sight: Option<AttachmentSight>,
    pub ammo_general: Option<AttachmentAmmoGeneral>,
    pub reload: Option<AttachmentReload>,
    pub add_ons: Option<AttachmentAddOns>,
    pub general: Option<AttachmentGeneral>,
    pub aim_assist: Option<AttachmentAimAssist>,
    pub ammunition: Option<AttachmentAmmunition>,
    pub damage: Option<AttachmentDamage>,
    pub projectile: Option<AttachmentProjectile>,
    pub location_damage: Option<[f32; 19]>,
    pub idle_settings: Option<AttachmentIdleSettings>,
    pub ads_settings: Option<AttachmentAdsSettings>,
    pub ads_settings_main: Option<AttachmentAdsSettings>,
    pub hip_spread: Option<AttachmentHipSpread>,
    pub gun_kick: Option<AttachmentGunKick>,
    pub view_kick: Option<[f32; 10]>,
    pub scales: AttachmentScales,
    pub hide_iron_sights: bool,
    pub share_ammo_with_alt: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachmentSight {
    pub aim_down_sight: bool,
    pub ads_fire: bool,
    pub rechamber_while_ads: bool,
    pub no_ads_when_mag_empty: bool,
    pub can_hold_breath: bool,
    pub can_variable_zoom: bool,
    pub hide_rail: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentAmmoGeneral {
    pub penetrate_type: i32,
    pub penetrate_multiplier: f32,
    pub impact_type: i32,
    pub fire_type: i32,
    pub rifle_bullet: bool,
    pub armor_piercing: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachmentReload {
    pub no_partial_reload: bool,
    pub segmented_reload: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachmentAddOns {
    pub motion_tracker: bool,
    pub silenced: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentGeneral {
    pub body: Option<Ptr>,
    pub bolt_action: bool,
    pub inherits_perks: bool,
    pub enemy_crosshair_range: f32,
    pub move_speed_scale: f32,
    pub ads_move_speed_scale: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachmentAmmunition {
    pub max_ammo: i32,
    pub start_ammo: i32,
    pub clip_size: i32,
    pub shot_count: i32,
    pub reload_ammo_add: i32,
    pub reload_start_add: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentAimAssist {
    pub auto_aim_range: f32,
    pub aim_assist_range: f32,
    pub aim_assist_range_ads: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentDamage {
    pub damage: i32,
    pub min_damage: i32,
    pub melee_damage: i32,
    pub max_damage_range: f32,
    pub min_damage_range: f32,
    pub player_damage: i32,
    pub min_player_damage: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttachmentProjectile {
    pub body: Ptr,
    pub explosion_radius: i32,
    pub explosion_inner_damage: i32,
    pub explosion_outer_damage: i32,
    pub speed: i32,
    pub speed_up: i32,
    pub activate_distance: i32,
    pub explosion_type: i32,
    pub impact_explode: bool,
    pub model: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentIdleSettings {
    pub hip_idle_amount: f32,
    pub hip_idle_speed: f32,
    pub idle_crouch_factor: f32,
    pub idle_prone_factor: f32,
    pub ads_idle_lerp_start_time: f32,
    pub ads_idle_lerp_time: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentHipSpread {
    pub values: [f32; 12],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentGunKick {
    pub hip_reduced_kick_bullets: i32,
    pub hip: [f32; 9],
    pub ads_reduced_kick_bullets: i32,
    pub ads: [f32; 9],
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentAdsSettings {
    pub ads_spread: f32,
    pub ads_aim_pitch: f32,
    pub ads_trans_in_time: f32,
    pub ads_trans_out_time: f32,
    pub ads_reload_trans_time_ms: i32,
    pub ads_crosshair_in_frac: f32,
    pub ads_crosshair_out_frac: f32,
    pub ads_zoom_fov: f32,
    pub ads_zoom_in_frac: f32,
    pub ads_zoom_out_frac: f32,
    pub ads_bob_factor: f32,
    pub ads_view_bob_mult: f32,
    pub ads_view_error_min: f32,
    pub ads_view_error_max: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AttachmentScales {
    pub ammunition: f32,
    pub damage: f32,
    pub damage_min: f32,
    pub state_timers: f32,
    pub fire_timers: f32,
    pub idle_settings: f32,
    pub ads_settings: f32,
    pub ads_settings_main: f32,
    pub hip_spread: f32,
    pub gun_kick: f32,
    pub view_kick: f32,
    pub view_center: f32,
}

pub(crate) fn read_attachment_facts(s: &ZoneStream<'_>, header: Ptr) -> AttachmentFacts {
    let body_at = |x86, x64| match s.ptr_at(header, s.layout(x86, x64)) {
        Ok(ZonePtr::Offset(ptr)) => Some(s.resolve_alias(ptr)),
        _ => None,
    };
    let i32_at = |x86, x64| s.i32_at(header, s.layout(x86, x64)).unwrap_or(0);
    let ads_at = |x86, x64| body_at(x86, x64).and_then(|body| read_ads(s, body));
    let scales_at = s.layout(sz::ATTACH_SCALES_OFF, 208);
    let scale = |index: usize| s.f32_at(header, scales_at + index * 4).unwrap_or(0.0);
    let flags = s.layout(sz::ATTACH_FLAGS_OFF, 260);
    AttachmentFacts {
        display_name: body_at(4, 8),
        attachment_type: i32_at(8, 16),
        weapon_type: i32_at(12, 20),
        weapon_class: i32_at(16, 24),
        load_index: i32_at(156, 256),
        projectile: body_at(104, 200).and_then(|body| {
            Some(AttachmentProjectile {
                body,
                explosion_radius: s.i32_at(body, 0).ok()?,
                explosion_inner_damage: s.i32_at(body, 4).ok()?,
                explosion_outer_damage: s.i32_at(body, 8).ok()?,
                speed: s.i32_at(body, 16).ok()?,
                speed_up: s.i32_at(body, 20).ok()?,
                activate_distance: s.i32_at(body, 24).ok()?,
                explosion_type: s.i32_at(body, s.layout(36, 40)).ok()?,
                impact_explode: s.u8_at(body, s.layout(60, 88)).ok()? != 0,
                model: match s.ptr_at(body, 32).ok()? {
                    ZonePtr::Offset(p) => Some(s.resolve_alias(p)),
                    _ => None,
                },
            })
        }),
        sight: body_at(sz::ATTACH_SIGHT_OFF, 64).and_then(|body| read_sight(s, body)),
        ammo_general: body_at(32, 56).and_then(|body| read_ammo_general(s, body)),
        reload: body_at(40, 72).and_then(|body| {
            Some(AttachmentReload {
                no_partial_reload: s.u8_at(body, 0).ok()? != 0,
                segmented_reload: s.u8_at(body, 1).ok()? != 0,
            })
        }),
        add_ons: body_at(sz::ATTACH_ADDONS_OFF, 80).and_then(|body| {
            Some(AttachmentAddOns {
                motion_tracker: s.u8_at(body, 0).ok()? != 0,
                silenced: s.u8_at(body, 1).ok()? != 0,
            })
        }),
        general: body_at(sz::ATTACH_GENERAL_OFF, 88).and_then(|body| read_general(s, body)),
        aim_assist: body_at(52, 96).and_then(|body| {
            f32_block::<3>(s, body, 0).map(|v| AttachmentAimAssist {
                auto_aim_range: v[0],
                aim_assist_range: v[1],
                aim_assist_range_ads: v[2],
            })
        }),
        ammunition: body_at(sz::ATTACH_AMMUNITION_OFF, 104)
            .and_then(|body| read_ammunition(s, body)),
        damage: body_at(60, 112).and_then(|body| read_damage(s, body)),
        location_damage: body_at(64, 120).and_then(|body| f32_block::<19>(s, body, 0)),
        idle_settings: body_at(68, 128).and_then(|body| {
            f32_block::<6>(s, body, 0).map(|v| AttachmentIdleSettings {
                hip_idle_amount: v[0],
                hip_idle_speed: v[1],
                idle_crouch_factor: v[2],
                idle_prone_factor: v[3],
                ads_idle_lerp_start_time: v[4],
                ads_idle_lerp_time: v[5],
            })
        }),
        ads_settings: ads_at(sz::ATTACH_ADS_SETTINGS_OFF, 136),
        ads_settings_main: ads_at(sz::ATTACH_ADS_SETTINGS_MAIN_OFF, 144),
        hip_spread: body_at(80, 152)
            .and_then(|body| f32_block::<12>(s, body, 0))
            .map(|values| AttachmentHipSpread { values }),
        gun_kick: body_at(84, 160).and_then(|body| {
            Some(AttachmentGunKick {
                hip_reduced_kick_bullets: s.i32_at(body, 0).ok()?,
                hip: f32_block::<9>(s, body, 4)?,
                ads_reduced_kick_bullets: s.i32_at(body, 40).ok()?,
                ads: f32_block::<9>(s, body, 44)?,
            })
        }),
        view_kick: body_at(88, 168).and_then(|body| f32_block::<10>(s, body, 0)),
        scales: AttachmentScales {
            ammunition: scale(0),
            damage: scale(1),
            damage_min: scale(2),
            state_timers: scale(3),
            fire_timers: scale(4),
            idle_settings: scale(5),
            ads_settings: scale(6),
            ads_settings_main: scale(7),
            hip_spread: scale(8),
            gun_kick: scale(9),
            view_kick: scale(10),
            view_center: scale(11),
        },
        hide_iron_sights: s.u8_at(header, flags).unwrap_or(0) != 0,
        share_ammo_with_alt: s.u8_at(header, flags + 1).unwrap_or(0) != 0,
    }
}

fn f32_block<const N: usize>(s: &ZoneStream<'_>, body: Ptr, first: usize) -> Option<[f32; N]> {
    let mut values = [0.0; N];
    for (index, value) in values.iter_mut().enumerate() {
        *value = s.f32_at(body, first + index * 4).ok()?;
    }
    Some(values)
}

fn read_sight(s: &ZoneStream<'_>, body: Ptr) -> Option<AttachmentSight> {
    let bit = |offset| s.u8_at(body, offset).ok().map(|value| value != 0);
    Some(AttachmentSight {
        aim_down_sight: bit(0)?,
        ads_fire: bit(1)?,
        rechamber_while_ads: bit(2)?,
        no_ads_when_mag_empty: bit(3)?,
        can_hold_breath: bit(4)?,
        can_variable_zoom: bit(5)?,
        hide_rail: bit(6)?,
    })
}

fn read_ammo_general(s: &ZoneStream<'_>, body: Ptr) -> Option<AttachmentAmmoGeneral> {
    let flags = s.layout(20, 24);
    Some(AttachmentAmmoGeneral {
        penetrate_type: s.i32_at(body, 0).ok()?,
        penetrate_multiplier: s.f32_at(body, 4).ok()?,
        impact_type: s.i32_at(body, 8).ok()?,
        fire_type: s.i32_at(body, 12).ok()?,
        rifle_bullet: s.u8_at(body, flags).ok()? != 0,
        armor_piercing: s.u8_at(body, flags + 1).ok()? != 0,
    })
}

fn read_general(s: &ZoneStream<'_>, body: Ptr) -> Option<AttachmentGeneral> {
    Some(AttachmentGeneral {
        body: Some(body),
        bolt_action: s.u8_at(body, 0).ok()? != 0,
        inherits_perks: s.u8_at(body, 1).ok()? != 0,
        enemy_crosshair_range: s.f32_at(body, 4).ok()?,
        move_speed_scale: s.f32_at(body, s.layout(24, 32)).ok()?,
        ads_move_speed_scale: s.f32_at(body, s.layout(28, 36)).ok()?,
    })
}

fn read_ammunition(s: &ZoneStream<'_>, body: Ptr) -> Option<AttachmentAmmunition> {
    Some(AttachmentAmmunition {
        max_ammo: s.i32_at(body, 0).ok()?,
        start_ammo: s.i32_at(body, 4).ok()?,
        clip_size: s.i32_at(body, 8).ok()?,
        shot_count: s.i32_at(body, 12).ok()?,
        reload_ammo_add: s.i32_at(body, 16).ok()?,
        reload_start_add: s.i32_at(body, 20).ok()?,
    })
}

fn read_damage(s: &ZoneStream<'_>, body: Ptr) -> Option<AttachmentDamage> {
    Some(AttachmentDamage {
        damage: s.i32_at(body, 0).ok()?,
        min_damage: s.i32_at(body, 4).ok()?,
        melee_damage: s.i32_at(body, 8).ok()?,
        max_damage_range: s.f32_at(body, 12).ok()?,
        min_damage_range: s.f32_at(body, 16).ok()?,
        player_damage: s.i32_at(body, 20).ok()?,
        min_player_damage: s.i32_at(body, 24).ok()?,
    })
}

fn read_ads(s: &ZoneStream<'_>, body: Ptr) -> Option<AttachmentAdsSettings> {
    let f = |offset| s.f32_at(body, offset).ok();
    Some(AttachmentAdsSettings {
        ads_spread: f(0)?,
        ads_aim_pitch: f(4)?,
        ads_trans_in_time: f(8)?,
        ads_trans_out_time: f(12)?,
        ads_reload_trans_time_ms: s.i32_at(body, 16).ok()?,
        ads_crosshair_in_frac: f(20)?,
        ads_crosshair_out_frac: f(24)?,
        ads_zoom_fov: f(28)?,
        ads_zoom_in_frac: f(32)?,
        ads_zoom_out_frac: f(36)?,
        ads_bob_factor: f(40)?,
        ads_view_bob_mult: f(44)?,
        ads_view_error_min: f(48)?,
        ads_view_error_max: f(52)?,
    })
}
