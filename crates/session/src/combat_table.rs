use asset_game::{WeaponBodyFacts, WeaponRegistry};
use weapon_iw4::{
    CapturedCombatInput, HITLOC_COUNT, LOCATION_DAMAGE_IDENTITY, MissingCombatFacts,
    WeaponCombatFacts, bake_location_damage, location_damage_is_valid,
};

pub(crate) fn validated_facts(
    f: WeaponBodyFacts,
    melee_charge_anim: bool,
    global_location: Option<[f32; HITLOC_COUNT]>,
) -> Result<WeaponCombatFacts, MissingCombatFacts> {
    let fire_type = weapon_iw4::FireType::from_i32(f.fire_type)
        .map_err(|_| MissingCombatFacts::UnknownFireType)?;

    let segmented_reload = f.segmented_reload && f.reload_start_time_ms > 0;
    let reload_ammo_add = if f.reload_ammo_add > 0 {
        f.reload_ammo_add
    } else if segmented_reload {
        1
    } else {
        0
    };
    if let Some(table) = f.location_damage_mult {
        if !location_damage_is_valid(&table) {
            return Err(MissingCombatFacts::LocationDamage);
        }
    }
    WeaponCombatFacts::try_from_captured(CapturedCombatInput {
        dual_wield: f.dual_wield,
        fire_melees: f.fire_melees,
        fire_time_ms: f.fire_time_ms,
        fire_delay_ms: f.fire_delay_ms,
        raise_time_ms: f.raise_time_ms,
        drop_time_ms: f.drop_time_ms,
        alternate_weapon: 0,
        alternate_raise_time_ms: f.alternate_raise_time_ms,
        alternate_drop_time_ms: f.alternate_drop_time_ms,
        first_raise_time_ms: f.first_raise_time_ms,
        reload_time_ms: f.reload_time_ms,
        reload_empty_time_ms: f.reload_empty_time_ms,
        clip_size: f.clip_size,
        start_ammo: f.start_ammo_rounds(),
        max_ammo: f.max_ammo_rounds(),
        ammo_index: f.ammo_index,
        clip_index: f.clip_index,
        fire_type: f.fire_type,
        weap_type: f.weap_type,
        weap_class: f.weap_class,
        player_anim_type: f.player_anim_type,
        inventory_type: f.inventory_type,

        impact_type: f.impact_type,
        shots_per_fire: f.shots_per_fire,

        burst_cooldown_ms: fire_type.is_burst().then_some(200).unwrap_or(0),

        bolt_action: f.bolt_action,
        rechamber_time_ms: f.rechamber_time_ms,
        rechamber_bolt_time_ms: f.rechamber_bolt_time_ms,
        rechamber_bolt_delay_ms: f.rechamber_bolt_delay_ms,
        segmented_reload,
        reload_start_time_ms: f.reload_start_time_ms,
        reload_end_time_ms: f.reload_end_time_ms,
        reload_ammo_add,
        reload_add_time_ms: f.reload_add_time_ms,
        reload_empty_add_time_ms: f.reload_empty_add_time_ms,
        reload_start_add_time_ms: f.reload_start_add_time_ms,
        reload_start_add: f.reload_start_add,
        no_partial_reload: f.no_partial_reload,
        dual_mag: f.dual_mag,
        inherits_perks: f.inherits_perks,
        sprint_raise_time_ms: f.sprint_raise_time_ms,
        sprint_drop_time_ms: f.sprint_drop_time_ms,
        stunned_start_time_ms: f.stunned_start_time_ms,
        stunned_end_time_ms: f.stunned_end_time_ms,
        damage: f.damage,
        min_damage: f.min_damage,
        max_damage_range: f.max_damage_range,
        min_damage_range: f.min_damage_range,
        hip_spread_stand_min: f.hip_spread_stand_min,
        hip_spread_ducked_min: f.hip_spread_ducked_min,
        hip_spread_prone_min: f.hip_spread_prone_min,
        hip_spread_stand_max: f.hip_spread_stand_max,
        hip_spread_ducked_max: f.hip_spread_ducked_max,
        hip_spread_prone_max: f.hip_spread_prone_max,
        hip_spread_decay_rate: f.hip_spread_decay_rate,
        hip_spread_fire_add: f.hip_spread_fire_add,
        hip_spread_turn_add: f.hip_spread_turn_add,
        hip_spread_move_add: f.hip_spread_move_add,
        hip_spread_ducked_decay: f.hip_spread_ducked_decay,
        hip_spread_prone_decay: f.hip_spread_prone_decay,
        ads_spread: f.ads_spread,
        aim_down_sight: f.aim_down_sight,
        no_ads_when_mag_empty: f.no_ads_when_mag_empty,
        ads_reload_trans_time_ms: f.ads_reload_trans_time_ms,
        ads_in_rate: f.ads_in_rate,
        ads_out_rate: f.ads_out_rate,
        rechamber_while_ads: f.rechamber_while_ads,
        ads_fire_only: f.ads_fire_only,
        melee_damage: f.melee_damage,
        can_hold_breath: f.can_hold_breath,
        overlay_reticle: f.overlay_reticle,
        melee_time_ms: f.melee_time_ms,
        melee_delay_ms: f.melee_delay_ms,
        melee_charge_time_ms: f.melee_charge_time_ms,
        melee_charge_delay_ms: f.melee_charge_delay_ms,
        melee_charge_anim,
        knife_model: f.knife_model,
        quick_raise_time_ms: f.quick_raise_time_ms,
        quick_drop_time_ms: f.quick_drop_time_ms,
        select_requires_ammo: f.select_requires_ammo,
        offhand_hold_is_cancelable: f.offhand_hold_is_cancelable,
        ads_gun_kick_reduced_kick_bullets: f.kick.ads_gun_kick_reduced_kick_bullets,
        hip_gun_kick_reduced_kick_bullets: f.kick.hip_gun_kick_reduced_kick_bullets,
        location_damage: bake_location_damage(
            f.weap_type,
            f.weap_class,
            global_location.unwrap_or(LOCATION_DAMAGE_IDENTITY),
            f.location_damage_mult,
        ),
    })
}

pub fn from_registry(
    weapons: &WeaponRegistry,
    global_location: Option<[f32; HITLOC_COUNT]>,
) -> Vec<WeaponCombatFacts> {
    let mut refused = Vec::new();
    let rows = (0..=weapons.len())
        .map(|i| {
            if i == 0 {
                return WeaponCombatFacts::none();
            }
            if !weapons.configuration_supported(i as u32) {
                diag::warn!(
                    Sim,
                    "unsupported T5 dual-hand animation configuration: {}",
                    weapons.name_of(i as u32)
                );
                return WeaponCombatFacts::none();
            }
            let Some(f) = weapons.facts_of(i as u32) else {
                return WeaponCombatFacts::none();
            };
            let charge_anim_of = |w: u32| {
                weapons
                    .sz_xanims_of(w)
                    .and_then(|t| t.get(8))
                    .and_then(|s| s.as_ref())
                    .is_some_and(|s| !s.is_empty())
            };
            let mut charge_anim = charge_anim_of(i as u32);
            let mut f = f;
            let melee_weapon = weapons.melee_weapon_of(i as u32);
            if melee_weapon != i as u32
                && let Some(m) = weapons.facts_of(melee_weapon)
            {
                f.melee_damage = m.melee_damage;
                f.melee_time_ms = m.melee_time_ms;
                f.melee_delay_ms = m.melee_delay_ms;
                f.melee_charge_time_ms = m.melee_charge_time_ms;
                f.melee_charge_delay_ms = m.melee_charge_delay_ms;
                charge_anim = charge_anim_of(melee_weapon);
                // Nonzero sends the gun through MELEE_END's quick raise after the knife.
                f.knife_model = melee_weapon;
            }
            let mut facts =
                validated_facts(f, charge_anim, global_location).unwrap_or_else(|reason| {
                    refused.push(format!("{}({reason:?})", weapons.name_of(i as u32)));
                    WeaponCombatFacts::none()
                });
            facts.alternate_weapon = weapons.alternate_of(i as u32);
            facts.aim_assist = weapon_iw4::AimAssistRanges {
                auto_aim: f.auto_aim_range,
                hip: f.aim_assist_range,
                ads: f.aim_assist_range_ads,
            };
            facts
        })
        .collect();
    if !refused.is_empty() {
        diag::info!(
            Sim,
            "combat facts refused for {} weapons: {}",
            refused.len(),
            refused.join(" ")
        );
    }
    rows
}

const T5_WEAPTYPE_MELEE: i32 = 7;

pub fn melee_only_from_registry(weapons: &WeaponRegistry) -> Vec<bool> {
    (0..=weapons.len())
        .map(|i| {
            weapons.facts_of(i as u32).is_some_and(|f| {
                f.weap_type == T5_WEAPTYPE_MELEE
                    && f.fire_time_ms <= 0
                    && f.raise_time_ms <= 0
                    && f.clip_size <= 0
            })
        })
        .collect()
}

pub fn script_sounds_from_registry(weapons: &WeaponRegistry) -> Vec<sim::WeaponScriptSounds> {
    (0..=weapons.len())
        .map(|i| {
            weapons
                .sounds_of(i as u32)
                .map(|s| sim::WeaponScriptSounds {
                    fire: s.fire.clone(),
                    fire_player: s.fire_player.clone(),
                    pickup: s.pickup.clone(),
                    pickup_player: s.pickup_player.clone(),
                    proj_explosion: s.proj_explosion.clone(),
                })
                .unwrap_or_default()
        })
        .collect()
}

pub fn pen_from_registry(weapons: &WeaponRegistry) -> Vec<weapon_iw4::BulletPenFacts> {
    (0..=weapons.len())
        .map(|i| {
            let Some(f) = weapons.facts_of(i as u32) else {
                return weapon_iw4::BulletPenFacts::default();
            };
            weapon_iw4::BulletPenFacts {
                penetrate_type: f.penetrate_type,
                penetrate_multiplier: f.penetrate_multiplier,
                rifle_bullet: f.rifle_bullet,
                ricochet_chance: f.ricochet_chance,
                explosive_bullet: f.explosive_bullet,
            }
        })
        .collect()
}

pub fn equipment_from_registry(weapons: &WeaponRegistry) -> Vec<sim::EquipmentRuntimeFacts> {
    (0..=weapons.len())
        .map(|index| {
            let Some(f) = weapons.facts_of(index as u32) else {
                return sim::EquipmentRuntimeFacts::default();
            };
            sim::EquipmentRuntimeFacts {
                offhand_class: f.offhand_class,
                start_ammo: f.start_ammo_rounds(),
                clip_size: f.clip_size,
                impact_damage: f.damage,
                impact_payload_weapon: weapons.impact_payload_of(index as u32).unwrap_or(0),
                fuse_time_ms: f.fuse_time_ms,
                hold_fire_time_ms: f.hold_fire_time_ms,
                cook_off_hold: f.cook_off_hold,
                has_detonator: f.has_detonator,
                detonate_delay_ms: f.detonate_delay_ms,
                detonate_time_ms: f.detonate_time_ms,
                projectile_rotates: f.projectile_rotates
                    || weapons.namespace_of(index as u32) != Some(asset_core::AssetNamespace::Iw4),
                stickiness: f.stickiness,
                timed_detonation: f.timed_detonation,
                proj_impact_explode: f.proj_impact_explode,
                stick_to_players: f.stick_to_players,
                ballistic_blade: weapons.namespace_of(index as u32)
                    == Some(asset_core::AssetNamespace::T5)
                    && weapons.name_of(index as u32) == "knife_ballistic_mp",
                explosion_radius: f.explosion_radius,
                explosion_radius_min: f.explosion_radius_min,
                explosion_inner_damage: f.explosion_inner_damage,
                explosion_outer_damage: f.explosion_outer_damage,
                damage_cone_angle: f.damage_cone_angle,
                missile_guidance: f.missile_guidance,
                ignition_delay_ms: f.ignition_delay_ms,
                require_lock_to_fire: f.require_lock_to_fire,
                projectile_speed: f.projectile_speed,
                projectile_speed_up: f.projectile_speed_up,
                projectile_speed_forward: f.projectile_speed_forward,
                projectile_speed_relative_up: f.projectile_speed_relative_up,
                refuses_pickup: f.refuses_pickup,
                projectile_activate_dist: f.projectile_activate_dist,
                projectile_explosion_type: f.projectile_explosion_type,
                weap_type: f.weap_type,
                weap_class: f.weap_class,
                parallel_bounce: f.parallel_bounce,
                perpendicular_bounce: f.perpendicular_bounce,
            }
        })
        .collect()
}
