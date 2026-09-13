const VIEWWEAPON_LEAN_VIEW_ROLL: f32 = 16.0;

const VIEWWEAPON_LEAN_DIST: f32 = 20.0;

pub fn viewmodel_lighting_origin(
    origin: [f32; 3],
    view_height_current: f32,
    view_yaw: f32,
    leanf: f32,
) -> [f32; 3] {
    math_iw4::add_lean_to_position(
        [origin[0], origin[1], origin[2] + view_height_current],
        view_yaw,
        leanf,
        VIEWWEAPON_LEAN_VIEW_ROLL,
        VIEWWEAPON_LEAN_DIST,
    )
}
