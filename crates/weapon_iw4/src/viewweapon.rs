use math_iw4::{angle_normalize_360, angle_vectors, vect_to_angles};

#[must_use]
pub fn viewweapon_iron_ads_saves_composed_axis(
    aim_down_sight: bool,
    weapon_pos_frac: f32,
    overlay_reticle: i32,
) -> bool {
    aim_down_sight && weapon_pos_frac != 0.0 && overlay_reticle == 0
}

#[must_use]
pub fn viewweapon_save_offset_movement(
    aim_down_sight: bool,
    weapon_pos_frac: f32,
    world_delta: [f32; 3],
) -> [f32; 3] {
    if !aim_down_sight {
        return [0.0; 3];
    }
    [
        world_delta[0] * weapon_pos_frac,
        world_delta[1] * weapon_pos_frac,
        world_delta[2] * weapon_pos_frac,
    ]
}

#[must_use]
pub fn viewweapon_view_to_world_delta(origin: [f32; 3], view_angles: [f32; 3]) -> [f32; 3] {
    let (fwd, right, up) = angle_vectors(view_angles);
    [
        origin[0] * fwd[0] + origin[1] * right[0] + origin[2] * up[0],
        origin[0] * fwd[1] + origin[1] * right[1] + origin[2] * up[1],
        origin[0] * fwd[2] + origin[1] * right[2] + origin[2] * up[2],
    ]
}

#[must_use]
pub fn viewweapon_composed_world_forward(gun_angles: [f32; 3], view_angles: [f32; 3]) -> [f32; 3] {
    let (gf, _, _) = angle_vectors(gun_angles);
    let (vf, vr, vu) = angle_vectors(view_angles);
    [
        gf[0] * vf[0] + gf[1] * vr[0] + gf[2] * vu[0],
        gf[0] * vf[1] + gf[1] * vr[1] + gf[2] * vu[1],
        gf[0] * vf[2] + gf[1] * vr[2] + gf[2] * vu[2],
    ]
}

#[must_use]
pub fn viewweapon_save_gun_pitch_yaw(
    gun_angles: [f32; 3],
    view_angles: [f32; 3],
    aim_down_sight: bool,
    weapon_pos_frac: f32,
    overlay_reticle: i32,
) -> [f32; 2] {
    if !viewweapon_iron_ads_saves_composed_axis(aim_down_sight, weapon_pos_frac, overlay_reticle) {
        return [view_angles[0], view_angles[1]];
    }
    let world_fwd = viewweapon_composed_world_forward(gun_angles, view_angles);
    let [pitch, yaw, _] = vect_to_angles(world_fwd);
    [angle_normalize_360(pitch), angle_normalize_360(yaw)]
}
