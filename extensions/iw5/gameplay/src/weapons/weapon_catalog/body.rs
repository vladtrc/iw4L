use super::captured::remap_iw5_weap_type;
use asset_game::{WeaponBodyFacts, WeaponKickFacts};
use weapon_iw4::WeaponIdleInputs;
pub(super) fn i32_at_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> i32 {
    stream.i32_at(body, stream.layout(x86, x64)).unwrap_or(0)
}

pub(super) fn u8_at_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> u8 {
    stream.u8_at(body, stream.layout(x86, x64)).unwrap_or(0)
}

pub(super) fn f32_at_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    body: ext_iw5_fastfile::Ptr,
    x86: usize,
    x64: usize,
) -> f32 {
    stream.f32_at(body, stream.layout(x86, x64)).unwrap_or(0.0)
}

pub(super) fn capture_iw5_body_facts(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
) -> WeaponBodyFacts {
    use ext_iw5_fastfile::size as sz;
    let mut facts = WeaponBodyFacts {
        body_resolved: geometry.weap_def.is_some(),
        fire_time_ms: geometry.fire_time_ms,
        clip_size: geometry.clip_size,
        weap_type: remap_iw5_weap_type(geometry.weap_type),
        weap_class: geometry.weap_class,
        fire_type: geometry.fire_type,
        move_speed_scale: geometry.move_speed_scale,
        ads_move_speed_scale: geometry.ads_move_speed_scale,
        ads_overlay_width: geometry.ads_overlay_width,
        ads_overlay_height: geometry.ads_overlay_height,
        overlay_reticle: geometry.overlay_reticle,
        overlay_interface: geometry.overlay_interface,
        ads_zoom_fov: geometry.ads_zoom_fov,
        ads_zoom_in_frac: geometry.ads_zoom_in_frac,
        ads_zoom_out_frac: geometry.ads_zoom_out_frac,
        ads_in_rate: geometry.ads_in_rate,
        ads_out_rate: geometry.ads_out_rate,
        impact_type: geometry.impact_type,
        penetrate_multiplier: geometry.penetrate_multiplier,
        motion_tracker: geometry.motion_tracker,
        kick: WeaponKickFacts {
            f_ads_view_kick_center_speed: geometry.ads_view_kick_center_speed,
            f_hip_view_kick_center_speed: geometry.hip_view_kick_center_speed,
            ..Default::default()
        },
        ..WeaponBodyFacts::default()
    };
    let Some(body) = geometry.weap_def else {
        return facts;
    };
    facts.kill_icon_ratio = i32_at_iw5(stream, body, sz::WEAPON_DEF_KILL_ICON_RATIO_OFF, 1588);
    facts.silenced = u8_at_iw5(stream, body, sz::WEAPON_DEF_SILENCED_OFF, 2459) != 0;
    facts.flip_kill_icon = u8_at_iw5(stream, body, sz::WEAPON_DEF_FLIP_KILL_ICON_OFF, 2455) != 0;
    facts.ads_aim_pitch = f32_at_iw5(stream, body, 1400, 1816);
    facts.ads_crosshair_in_frac = f32_at_iw5(stream, body, 1404, 1820);
    facts.ads_crosshair_out_frac = f32_at_iw5(stream, body, 1408, 1824);
    facts.kick.gun_max_pitch = f32_at_iw5(stream, body, sz::WEAPON_DEF_GUN_MAX_PITCH_OFF, 1496);
    facts.kick.gun_max_yaw = f32_at_iw5(stream, body, sz::WEAPON_DEF_GUN_MAX_YAW_OFF, 1500);
    facts.kick.ads_gun_kick_reduced_kick_bullets = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_BULLETS_OFF,
        1828,
    );
    facts.kick.ads_gun_kick_reduced_kick_percent = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_REDUCED_KICK_PERCENT_OFF,
        1832,
    );
    facts.kick.ads_gun_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MIN_OFF,
        1836,
    );
    facts.kick.ads_gun_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_PITCH_MAX_OFF,
        1840,
    );
    facts.kick.ads_gun_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MIN_OFF, 1844);
    facts.kick.ads_gun_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_YAW_MAX_OFF, 1848);
    facts.kick.ads_gun_kick_accel =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_GUN_KICK_ACCEL_OFF, 1852);
    facts.kick.ads_gun_kick_speed_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_MAX_OFF,
        1856,
    );
    facts.kick.ads_gun_kick_speed_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_SPEED_DECAY_OFF,
        1860,
    );
    facts.kick.ads_gun_kick_static_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_GUN_KICK_STATIC_DECAY_OFF,
        1864,
    );
    facts.kick.ads_view_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MIN_OFF,
        1868,
    );
    facts.kick.ads_view_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_ADS_VIEW_KICK_PITCH_MAX_OFF,
        1872,
    );
    facts.kick.ads_view_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MIN_OFF, 1876);
    facts.kick.ads_view_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_VIEW_KICK_YAW_MAX_OFF, 1880);
    facts.kick.hip_gun_kick_reduced_kick_bullets = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_BULLETS_OFF,
        1896,
    );
    facts.kick.hip_gun_kick_reduced_kick_percent = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_REDUCED_KICK_PERCENT_OFF,
        1900,
    );
    facts.kick.hip_gun_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MIN_OFF,
        1904,
    );
    facts.kick.hip_gun_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_PITCH_MAX_OFF,
        1908,
    );
    facts.kick.hip_gun_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MIN_OFF, 1912);
    facts.kick.hip_gun_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_YAW_MAX_OFF, 1916);
    facts.kick.hip_gun_kick_accel =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_GUN_KICK_ACCEL_OFF, 1920);
    facts.kick.hip_gun_kick_speed_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_MAX_OFF,
        1924,
    );
    facts.kick.hip_gun_kick_speed_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_SPEED_DECAY_OFF,
        1928,
    );
    facts.kick.hip_gun_kick_static_decay = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_GUN_KICK_STATIC_DECAY_OFF,
        1932,
    );
    facts.kick.hip_view_kick_pitch_min = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MIN_OFF,
        1936,
    );
    facts.kick.hip_view_kick_pitch_max = f32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_HIP_VIEW_KICK_PITCH_MAX_OFF,
        1940,
    );
    facts.kick.hip_view_kick_yaw_min =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MIN_OFF, 1944);
    facts.kick.hip_view_kick_yaw_max =
        f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_VIEW_KICK_YAW_MAX_OFF, 1948);
    facts.ammo_counter_clip = i32_at_iw5(stream, body, sz::WEAPON_DEF_AMMO_COUNTER_CLIP_OFF, 836);
    facts.start_ammo = i32_at_iw5(stream, body, sz::WEAPON_DEF_START_AMMO_OFF, 840);
    facts.i_reticle_side_size = i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_SIDE_SIZE_OFF, 580);
    facts.i_reticle_min_ofs = i32_at_iw5(stream, body, sz::WEAPON_DEF_RETICLE_MIN_OFS_OFF, 584);
    facts.ammo_index = i32_at_iw5(stream, body, sz::WEAPON_DEF_AMMO_INDEX_OFF, 856);
    facts.clip_index = i32_at_iw5(stream, body, sz::WEAPON_DEF_CLIP_INDEX_OFF, 872);
    facts.max_ammo = i32_at_iw5(stream, body, sz::WEAPON_DEF_MAX_AMMO_OFF, 876);
    facts.shots_per_fire = i32_at_iw5(stream, body, sz::WEAPON_DEF_SHOTS_PER_FIRE_OFF, 880);
    facts.apply_hip_spread(read_hip_spread(
        |off| stream.f32_at(body, off).unwrap_or(0.0),
        stream.layout(sz::WEAPON_DEF_HIP_SPREAD_STAND_MIN_OFF, 1420),
    ));
    facts.melee_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_DAMAGE_OFF, 912);
    facts.melee_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_TIME_OFF, 956);
    facts.melee_delay_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_DELAY_OFF, 924);
    facts.melee_charge_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_CHARGE_TIME_OFF, 960);
    facts.melee_charge_delay_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_MELEE_CHARGE_DELAY_OFF, 928);
    facts.knife_model = u32::from(matches!(
        stream.ptr_at(body, stream.layout(sz::WEAPON_DEF_KNIFE_MODEL_OFF, 776)),
        Ok(ext_iw5_fastfile::ZonePtr::Offset(_))
            | Ok(ext_iw5_fastfile::ZonePtr::Following)
            | Ok(ext_iw5_fastfile::ZonePtr::Insert)
    ));
    facts.fire_delay_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_FIRE_DELAY_OFF, 920);
    facts.rechamber_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_TIME_OFF, 936);
    facts.rechamber_bolt_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_TIME_OFF, 940);
    facts.rechamber_bolt_delay_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_BOLT_DELAY_OFF, 944);
    facts.reload_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_TIME_OFF, 964);
    facts.reload_empty_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_EMPTY_TIME_OFF, 972);
    facts.reload_add_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_ADD_TIME_OFF, 976);
    facts.reload_start_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_TIME_OFF, 980);
    facts.reload_start_add_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_START_ADD_TIME_OFF, 984);
    facts.reload_end_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RELOAD_END_TIME_OFF, 988);
    facts.alternate_raise_time_ms = geometry.alternate_raise_time_ms;
    facts.alternate_drop_time_ms = geometry.alternate_drop_time_ms;
    facts.drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_DROP_TIME_OFF, 992);
    facts.raise_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_RAISE_TIME_OFF, 996);
    facts.quick_drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_QUICK_DROP_TIME_OFF, 1004);
    facts.quick_raise_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_QUICK_RAISE_TIME_OFF, 1008);
    facts.sprint_raise_time_ms =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_RAISE_TIME_OFF, 1024);
    facts.sprint_loop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_LOOP_TIME_OFF, 1028);
    facts.sprint_drop_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_SPRINT_DROP_TIME_OFF, 1032);
    facts.damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_DAMAGE_OFF, 904);
    facts.min_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_OFF, 2144);
    facts.min_player_damage = i32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_PLAYER_DAMAGE_OFF, 2148);
    facts.max_damage_range = f32_at_iw5(stream, body, sz::WEAPON_DEF_MAX_DAMAGE_RANGE_OFF, 2152);
    facts.min_damage_range = f32_at_iw5(stream, body, sz::WEAPON_DEF_MIN_DAMAGE_RANGE_OFF, 2156);
    facts.inherits_perks = u8_at_iw5(stream, body, sz::WEAPON_DEF_INHERITS_PERKS_OFF, 2435) != 0;
    facts.rifle_bullet = u8_at_iw5(stream, body, sz::WEAPON_DEF_RIFLE_BULLET_OFF, 2437) != 0;
    facts.ricochet_chance = f32_at_iw5(stream, body, sz::WEAPON_DEF_RICOCHET_CHANCE_OFF, 1728);
    facts.explosive_bullet =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSIVE_BULLET_OFF, 2444) != 0;
    facts.bolt_action = u8_at_iw5(stream, body, sz::WEAPON_DEF_BOLT_ACTION_OFF, 2439) != 0;
    facts.aim_down_sight = u8_at_iw5(stream, body, sz::WEAPON_DEF_AIM_DOWN_SIGHT_OFF, 2440) != 0;
    facts.can_hold_breath = u8_at_iw5(stream, body, sz::WEAPON_DEF_CAN_HOLD_BREATH_OFF, 2441) != 0;
    if u8_at_iw5(stream, body, sz::WEAPON_DEF_CAN_VARIABLE_ZOOM_OFF, 2442) != 0 {
        facts.scope_zoom = weapon_iw4::ScopeZoom::from_fovs([25.0, 15.0, 8.0]);
    }
    facts.rechamber_while_ads =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_RECHAMBER_WHILE_ADS_OFF, 2443) != 0;
    facts.ads_fire_only = u8_at_iw5(stream, body, sz::WEAPON_DEF_ADS_FIRE_ONLY_OFF, 2448) != 0;
    facts.no_partial_reload =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_NO_PARTIAL_RELOAD_OFF, 2456) != 0;
    facts.segmented_reload =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_SEGMENTED_RELOAD_OFF, 2457) != 0;
    facts.select_requires_ammo = Some(
        u8_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_DISABLE_SWITCH_TO_WHEN_EMPTY_OFF,
            2450,
        ) != 0,
    );
    facts.offhand_hold_is_cancelable = Some(
        u8_at_iw5(
            stream,
            body,
            sz::WEAPON_DEF_OFFHAND_HOLD_IS_CANCELABLE_OFF,
            2478,
        ) != 0,
    );
    facts.idle = WeaponIdleInputs {
        ads_idle_amount: f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_IDLE_AMOUNT_OFF, 1472),
        hip_idle_amount: f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_IDLE_AMOUNT_OFF, 1476),
        ads_idle_speed: f32_at_iw5(stream, body, sz::WEAPON_DEF_ADS_IDLE_SPEED_OFF, 1480),
        hip_idle_speed: f32_at_iw5(stream, body, sz::WEAPON_DEF_HIP_IDLE_SPEED_OFF, 1484),
        idle_crouch_factor: f32_at_iw5(stream, body, sz::WEAPON_DEF_IDLE_CROUCH_FACTOR_OFF, 1488),
        idle_prone_factor: f32_at_iw5(stream, body, sz::WEAPON_DEF_IDLE_PRONE_FACTOR_OFF, 1492),
    };
    facts.offhand_class = i32_at_iw5(stream, body, sz::WEAPON_DEF_OFFHAND_CLASS_OFF, 104);
    facts.hold_fire_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_HOLD_FIRE_TIME_OFF, 948);
    facts.fuse_time_ms = i32_at_iw5(stream, body, sz::WEAPON_DEF_FUSE_TIME_OFF, 1072);
    facts.stickiness = i32_at_iw5(stream, body, sz::WEAPON_DEF_STICKINESS_OFF, 1720);
    facts.has_detonator = u8_at_iw5(stream, body, sz::WEAPON_DEF_HAS_DETONATOR_OFF, 2466) != 0;
    facts.timed_detonation =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_TIMED_DETONATION_OFF, 2468) != 0;
    facts.projectile_rotates = u8_at_iw5(stream, body, sz::WEAPON_DEF_ROTATE_OFF, 2469) != 0;
    facts.cook_off_hold = u8_at_iw5(stream, body, sz::WEAPON_DEF_COOK_OFF_HOLD_OFF, 2445) != 0;
    facts.explosion_radius = i32_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_OFF, 1612);
    facts.explosion_radius_min =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_EXPLOSION_RADIUS_MIN_OFF, 1616);
    facts.explosion_inner_damage = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_EXPLOSION_INNER_DAMAGE_OFF,
        1620,
    );
    facts.explosion_outer_damage = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_EXPLOSION_OUTER_DAMAGE_OFF,
        1624,
    );
    facts.projectile_speed = i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_OFF, 1640);
    facts.projectile_speed_up =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJECTILE_SPEED_UP_OFF, 1644);
    facts.projectile_activate_dist = i32_at_iw5(
        stream,
        body,
        sz::WEAPON_DEF_PROJECTILE_ACTIVATE_DIST_OFF,
        1652,
    );
    let bounce_table = |x86, x64| -> Option<[f32; 31]> {
        let table = match stream.ptr_at(body, stream.layout(x86, x64)).ok()? {
            ext_iw5_fastfile::ZonePtr::Offset(p) => stream.resolve_alias(p),
            _ => return None,
        };
        let mut values = [0.0; 31];
        for (i, value) in values.iter_mut().enumerate() {
            *value = stream.f32_at(table, i * 4).ok()?;
        }
        Some(values)
    };
    facts.parallel_bounce = bounce_table(1352, 1744);
    facts.perpendicular_bounce = bounce_table(1356, 1752);
    facts.missile_guidance = i32_at_iw5(stream, body, 1380, 1788);
    facts.ignition_delay_ms = i32_at_iw5(stream, body, 1388, 1796);
    facts.require_lock_to_fire = u8_at_iw5(stream, body, 1898, 2430) != 0;
    facts.projectile_explosion_type =
        i32_at_iw5(stream, body, sz::WEAPON_DEF_PROJ_EXPLOSION_TYPE_OFF, 1680);
    facts.proj_impact_explode =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_PROJ_IMPACT_EXPLODE_OFF, 2462) != 0;
    facts.stick_to_players =
        u8_at_iw5(stream, body, sz::WEAPON_DEF_STICK_TO_PLAYERS_OFF, 2463) != 0;
    facts
}

fn read_hip_spread(read: impl Fn(usize) -> f32, start: usize) -> [f32; 12] {
    std::array::from_fn(|i| read(start + i * 4))
}
