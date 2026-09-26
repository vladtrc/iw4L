pub(crate) const RADIUS_AREA_HALF_SCALE: f32 = f32::from_bits(0x3fb5_04f3);

pub fn radius_damage_distance_to_aabb(origin: [f32; 3], center: [f32; 3], half: [f32; 3]) -> f32 {
    let mut sum = 0.0f32;
    for i in 0..3 {
        let mut axis = (origin[i] - center[i]).abs() - half[i];
        if axis < 0.0 {
            axis = 0.0;
        }
        sum += axis * axis;
    }
    libm::sqrtf(sum)
}

pub fn g_radius_damage_area_half_extent(radius: f32) -> f32 {
    let radius = if radius < 1.0 { 1.0 } else { radius };
    radius * RADIUS_AREA_HALF_SCALE
}

pub fn g_radius_damage_amount(
    inner: f32,
    outer: f32,
    radius: f32,
    dist: f32,
    vis_scale: f32,
) -> i32 {
    let radius = if radius < 1.0 { 1.0 } else { radius };
    if !(dist * dist < radius * radius) || vis_scale <= 0.0 {
        return 0;
    }
    let falloff = outer + (1.0 - dist / radius) * (inner - outer);
    libm::roundf(falloff * vis_scale) as i32
}

pub const G_CAN_DAMAGE_CONTENTS_MASK: u32 = 0x802011;

pub(crate) const G_CAN_DAMAGE_HALF_WIDTH: f32 = 15.0;

pub(crate) const G_CAN_DAMAGE_HALF_HEIGHT_SCALE: f32 = 0.5;

pub(crate) const G_CAN_DAMAGE_PARTIAL_DIVISOR: f32 = 3.0;

pub(crate) fn g_can_damage_player_sample_points(
    origin: [f32; 3],
    view_height: f32,
    right: [f32; 3],
) -> [[f32; 3]; 5] {
    let eye = [origin[0], origin[1], origin[2] + view_height];
    let half_height = (eye[2] - origin[2]) * G_CAN_DAMAGE_HALF_HEIGHT_SCALE;
    let mid = [
        (origin[0] + eye[0]) * 0.5,
        (origin[1] + eye[1]) * 0.5,
        (origin[2] + eye[2]) * 0.5,
    ];
    [
        mid,
        [
            mid[0] + right[0] * G_CAN_DAMAGE_HALF_WIDTH,
            mid[1] + right[1] * G_CAN_DAMAGE_HALF_WIDTH,
            mid[2] + right[2] * G_CAN_DAMAGE_HALF_WIDTH,
        ],
        [
            mid[0] - right[0] * G_CAN_DAMAGE_HALF_WIDTH,
            mid[1] - right[1] * G_CAN_DAMAGE_HALF_WIDTH,
            mid[2] - right[2] * G_CAN_DAMAGE_HALF_WIDTH,
        ],
        [mid[0], mid[1], mid[2] + half_height],
        [mid[0], mid[1], mid[2] - half_height],
    ]
}

pub(crate) fn g_can_damage_hits_to_scale(hits: u32) -> f32 {
    if hits == 0 {
        0.0
    } else if hits < 4 {
        hits as f32 / G_CAN_DAMAGE_PARTIAL_DIVISOR
    } else {
        1.0
    }
}

pub fn g_can_damage_player_vis_scale(
    origin: [f32; 3],
    view_height: f32,
    right: [f32; 3],
    inflictor: [f32; 3],
    mut trace_passed: impl FnMut([f32; 3], [f32; 3]) -> bool,
) -> f32 {
    let samples = g_can_damage_player_sample_points(origin, view_height, right);
    let mut hits = 0u32;
    for sample in samples {
        if trace_passed(inflictor, sample) {
            hits += 1;
        }
    }
    g_can_damage_hits_to_scale(hits)
}
