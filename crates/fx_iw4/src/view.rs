use crate::life::fx_life_span_range_from_bytes;

pub const FX_ELEM_DEF_STRIDE: usize = 0xFC;

pub const FX_EFFECT_DEF_SIZE: usize = 0x20;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxElemDefView {
    pub flags: i32,

    pub spawn_a: i32,

    pub spawn_b: i32,
    pub spawn_range_base: f32,
    pub spawn_range_amplitude: f32,

    pub fade_in_range: [f32; 2],

    pub fade_out_range: [f32; 2],
    pub spawn_frustum_cull_radius: f32,
    pub spawn_delay_msec_base: i32,
    pub spawn_delay_msec_amplitude: i32,
    pub life_span_msec_base: i32,
    pub life_span_msec_amplitude: i32,

    pub spawn_origin: [[f32; 2]; 3],
    pub spawn_offset_radius_base: f32,
    pub spawn_offset_radius_amplitude: f32,
    pub spawn_offset_height_base: f32,
    pub spawn_offset_height_amplitude: f32,

    pub spawn_angles: [[f32; 2]; 3],

    pub angular_velocity: [[f32; 2]; 3],

    pub initial_rotation: [f32; 2],
    pub gravity_base: f32,
    pub gravity_amplitude: f32,

    pub reflection_factor: [f32; 2],

    pub coll_mins: [f32; 3],
    pub coll_maxs: [f32; 3],
    pub elem_type: u8,
    pub visual_count: u8,
    pub vel_interval_count: u8,
    pub vis_state_interval_count: u8,
    pub lighting_frac: u8,

    pub use_item_clip: u8,

    pub sort_order: u8,

    pub emit_dist: [f32; 2],

    pub emit_dist_variance: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxEffectDefView {
    pub flags: i32,

    pub msec_looping_life: i32,
    pub looping_count: i32,
    pub one_shot_count: i32,
    pub emission_count: i32,
}

impl FxEffectDefView {
    pub fn total_elem_defs(self) -> i32 {
        self.looping_count
            .saturating_add(self.one_shot_count)
            .saturating_add(self.emission_count)
    }
}

pub fn fx_elem_def_view(bytes: &[u8]) -> Option<FxElemDefView> {
    fx_elem_def_view_layout(bytes, false)
}

pub fn fx_elem_def_view_x64(bytes: &[u8]) -> Option<FxElemDefView> {
    fx_elem_def_view_layout(bytes, true)
}

fn fx_elem_def_view_layout(bytes: &[u8], x64: bool) -> Option<FxElemDefView> {
    if bytes.len() < if x64 { 288 } else { FX_ELEM_DEF_STRIDE } {
        return None;
    }
    let flags = i32::from_le_bytes(bytes[0x00..0x04].try_into().ok()?);
    let spawn = fx_life_span_range_from_bytes(bytes[0x04..0x0c].try_into().ok()?);
    let spawn_range_base = f32::from_le_bytes(bytes[0x0c..0x10].try_into().ok()?);
    let spawn_range_amplitude = f32::from_le_bytes(bytes[0x10..0x14].try_into().ok()?);
    let fade_in_range = [
        f32::from_le_bytes(bytes[0x14..0x18].try_into().ok()?),
        f32::from_le_bytes(bytes[0x18..0x1c].try_into().ok()?),
    ];
    let fade_out_range = [
        f32::from_le_bytes(bytes[0x1c..0x20].try_into().ok()?),
        f32::from_le_bytes(bytes[0x20..0x24].try_into().ok()?),
    ];
    let spawn_frustum_cull_radius = f32::from_le_bytes(bytes[0x24..0x28].try_into().ok()?);
    let delay = fx_life_span_range_from_bytes(bytes[0x28..0x30].try_into().ok()?);
    let life = fx_life_span_range_from_bytes(bytes[0x30..0x38].try_into().ok()?);
    let mut spawn_origin = [[0.0f32; 2]; 3];
    for i in 0..3 {
        let off = 0x38 + i * 8;
        spawn_origin[i][0] = f32::from_le_bytes(bytes[off..off + 4].try_into().ok()?);
        spawn_origin[i][1] = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().ok()?);
    }
    let spawn_offset_radius_base = f32::from_le_bytes(bytes[0x50..0x54].try_into().ok()?);
    let spawn_offset_radius_amplitude = f32::from_le_bytes(bytes[0x54..0x58].try_into().ok()?);
    let spawn_offset_height_base = f32::from_le_bytes(bytes[0x58..0x5c].try_into().ok()?);
    let spawn_offset_height_amplitude = f32::from_le_bytes(bytes[0x5c..0x60].try_into().ok()?);
    let mut spawn_angles = [[0.0f32; 2]; 3];
    for i in 0..3 {
        let off = 0x60 + i * 8;
        spawn_angles[i][0] = f32::from_le_bytes(bytes[off..off + 4].try_into().ok()?);
        spawn_angles[i][1] = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().ok()?);
    }
    let mut angular_velocity = [[0.0f32; 2]; 3];
    for i in 0..3 {
        let off = 0x78 + i * 8;
        angular_velocity[i][0] = f32::from_le_bytes(bytes[off..off + 4].try_into().ok()?);
        angular_velocity[i][1] = f32::from_le_bytes(bytes[off + 4..off + 8].try_into().ok()?);
    }
    let initial_rotation = [
        f32::from_le_bytes(bytes[0x90..0x94].try_into().ok()?),
        f32::from_le_bytes(bytes[0x94..0x98].try_into().ok()?),
    ];
    let gravity_base = f32::from_le_bytes(bytes[0x98..0x9c].try_into().ok()?);
    let gravity_amplitude = f32::from_le_bytes(bytes[0x9c..0xa0].try_into().ok()?);
    let reflection_factor = [
        f32::from_le_bytes(bytes[0xa0..0xa4].try_into().ok()?),
        f32::from_le_bytes(bytes[0xa4..0xa8].try_into().ok()?),
    ];
    let off = |x: usize| {
        if x64 {
            match x {
                192..=216 => x + 16,
                228..=244 => x + 28,
                248..=251 => x + 32,
                _ => x,
            }
        } else {
            x
        }
    };
    let coll_mins = [
        f32::from_le_bytes(bytes[off(0xc0)..off(0xc4)].try_into().ok()?),
        f32::from_le_bytes(bytes[off(0xc4)..off(0xc8)].try_into().ok()?),
        f32::from_le_bytes(bytes[off(0xc8)..off(0xcc)].try_into().ok()?),
    ];
    let coll_maxs = [
        f32::from_le_bytes(bytes[off(0xcc)..off(0xd0)].try_into().ok()?),
        f32::from_le_bytes(bytes[off(0xd0)..off(0xd4)].try_into().ok()?),
        f32::from_le_bytes(bytes[off(0xd4)..off(0xd8)].try_into().ok()?),
    ];
    let emit_dist = [
        f32::from_le_bytes(bytes[off(0xe4)..off(0xe8)].try_into().ok()?),
        f32::from_le_bytes(bytes[off(0xe8)..off(0xec)].try_into().ok()?),
    ];
    let emit_dist_variance = [
        f32::from_le_bytes(bytes[off(0xec)..off(0xf0)].try_into().ok()?),
        f32::from_le_bytes(bytes[off(0xf0)..off(0xf4)].try_into().ok()?),
    ];
    Some(FxElemDefView {
        flags,
        spawn_a: spawn.0,
        spawn_b: spawn.1,
        spawn_range_base,
        spawn_range_amplitude,
        fade_in_range,
        fade_out_range,
        spawn_frustum_cull_radius,
        spawn_delay_msec_base: delay.0,
        spawn_delay_msec_amplitude: delay.1,
        life_span_msec_base: life.0,
        life_span_msec_amplitude: life.1,
        spawn_origin,
        spawn_offset_radius_base,
        spawn_offset_radius_amplitude,
        spawn_offset_height_base,
        spawn_offset_height_amplitude,
        spawn_angles,
        angular_velocity,
        initial_rotation,
        gravity_base,
        gravity_amplitude,
        reflection_factor,
        coll_mins,
        coll_maxs,
        elem_type: bytes[0xb0],
        visual_count: bytes[0xb1],
        vel_interval_count: bytes[0xb2],
        vis_state_interval_count: bytes[0xb3],
        lighting_frac: bytes[off(0xf9)],
        use_item_clip: bytes[off(0xfa)],
        sort_order: bytes[off(0xf8)],
        emit_dist,
        emit_dist_variance,
    })
}

pub fn fx_effect_def_view(bytes: &[u8]) -> Option<FxEffectDefView> {
    if bytes.len() < FX_EFFECT_DEF_SIZE {
        return None;
    }
    Some(FxEffectDefView {
        flags: i32::from_le_bytes(bytes[0x04..0x08].try_into().ok()?),
        msec_looping_life: i32::from_le_bytes(bytes[0x0c..0x10].try_into().ok()?),
        looping_count: i32::from_le_bytes(bytes[0x10..0x14].try_into().ok()?),
        one_shot_count: i32::from_le_bytes(bytes[0x14..0x18].try_into().ok()?),
        emission_count: i32::from_le_bytes(bytes[0x18..0x1c].try_into().ok()?),
    })
}

pub fn fx_elem_def_gravity_accel_z(view: &FxElemDefView, u01: f32) -> f32 {
    let authored = view.gravity_base + view.gravity_amplitude * clamp01(u01);
    crate::gravity::fx_elem_gravity_accel_z(authored)
}

fn clamp01(x: f32) -> f32 {
    if x < 0.0 {
        0.0
    } else if x > 1.0 {
        1.0
    } else {
        x
    }
}
