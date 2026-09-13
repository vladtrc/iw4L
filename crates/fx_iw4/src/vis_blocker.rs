pub const FX_VIS_BLOCKER_REC_STRIDE: usize = 0x10;

pub const FX_VIS_BLOCKER_SLOT_CAP: u32 = 0x100;

pub const FX_VIS_BLOCKER_PARAM3_SCALE: f64 = 16.0;

pub const FX_VIS_BLOCKER_PARAM4_INV_SCALE: f64 = 65536.0;

pub const FX_DISTANCE_FADE_SCALE: f64 = 255.0;

pub const FX_DISTANCE_FADE_BIAS: f64 = 0.5;

pub const FX_VIS_BLOCKER_BYTE_TO_UNIT: f64 = f64::from_bits(0x3f70_1010_2000_0000);

pub const FX_VIS_MIN_TRACE_DIST_DEFAULT: f32 = 80.0;

pub const FX_CLIENT_VISIBILITY_THRESHOLD: f32 = 0.0001;

pub const FX_ELEM_FLAG_VIS_BLOCKER: i32 = 0x1000;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FxVisBlockerRec {
    pub origin: [f32; 3],

    pub param3_x16: i16,

    pub one_minus_param4_x16: i16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxVisBlockerBuf {
    pub count: u32,
    pub recs: [FxVisBlockerRec; FX_VIS_BLOCKER_SLOT_CAP as usize],
}

impl Default for FxVisBlockerBuf {
    fn default() -> Self {
        Self {
            count: 0,
            recs: [FxVisBlockerRec::default(); FX_VIS_BLOCKER_SLOT_CAP as usize],
        }
    }
}

#[inline]
fn fx_vis_blocker_chop_i16(x: f32) -> i16 {
    (x as i32) as i16
}

#[inline]
pub fn fx_distance_fade_range(dist: f32, base: f32, amp: f32) -> f32 {
    let d = dist - base;
    if d < 0.0 {
        1.0
    } else if d < amp {
        1.0 - d / amp
    } else {
        0.0
    }
}

#[inline]
pub fn fx_evaluate_distance_fade(
    dist: f32,
    fade_in_base: f32,
    fade_in_amp: f32,
    fade_out_base: f32,
    fade_out_amp: f32,
) -> Option<u32> {
    if fade_in_amp == 0.0 && fade_out_amp == 0.0 {
        return None;
    }
    let mut fade_in = 1.0;
    let mut fade_out = 1.0;
    if fade_in_amp != 0.0 {
        fade_in = fx_distance_fade_range(dist, fade_in_base, fade_in_amp);
    }
    if fade_out_amp != 0.0 {
        fade_out = 1.0 - fx_distance_fade_range(dist, fade_out_base, fade_out_amp);
    }
    let fade = if fade_in < fade_out {
        fade_in
    } else {
        fade_out
    };
    Some((fade * FX_DISTANCE_FADE_SCALE as f32 + FX_DISTANCE_FADE_BIAS as f32) as i32 as u32)
}

#[inline]
pub fn fx_vis_blocker_param4(color_alpha: u8, distance_fade: u32) -> f32 {
    let prod = (u32::from(color_alpha).wrapping_mul(distance_fade)) >> 8;
    (prod as f32) * (FX_VIS_BLOCKER_BYTE_TO_UNIT as f32)
}

#[inline]
pub fn fx_vis_blocker_add_prepared(
    buf: &mut FxVisBlockerBuf,
    flags: i32,
    origin: [f32; 3],
    size0: f32,
    color_alpha: u8,
    fade_in: [f32; 2],
    fade_out: [f32; 2],
    camera: [f32; 3],
) -> bool {
    if (flags & FX_ELEM_FLAG_VIS_BLOCKER) == 0 {
        return false;
    }
    let Some(fade) = fx_evaluate_distance_fade(
        crate::vec::fx_vec3_distance(camera, origin),
        fade_in[0],
        fade_in[1],
        fade_out[0],
        fade_out[1],
    ) else {
        return false;
    };
    fx_vis_blocker_add(buf, origin, size0, fx_vis_blocker_param4(color_alpha, fade))
}

#[inline]
pub fn fx_vis_blocker_add(
    buf: &mut FxVisBlockerBuf,
    origin: [f32; 3],
    param_3: f32,
    param_4: f32,
) -> bool {
    let next = buf.count.wrapping_add(1);
    if next >= FX_VIS_BLOCKER_SLOT_CAP {
        return false;
    }
    buf.recs[next as usize] = FxVisBlockerRec {
        origin,
        param3_x16: fx_vis_blocker_chop_i16(param_3 * FX_VIS_BLOCKER_PARAM3_SCALE as f32),
        one_minus_param4_x16: fx_vis_blocker_chop_i16(
            (1.0 - param_4) * FX_VIS_BLOCKER_PARAM4_INV_SCALE as f32,
        ),
    };
    buf.count = next;
    true
}

#[inline]
pub fn fx_vis_blocker_generate_verts(write: &mut FxVisBlockerBuf, read: &mut FxVisBlockerBuf) {
    core::mem::swap(write, read);
    read.count = 0;
}

#[inline]
pub fn fx_get_client_visibility(
    buf: &FxVisBlockerBuf,
    start: [f32; 3],
    end: [f32; 3],
    min_trace_dist: f32,
) -> f32 {
    if buf.count == 0 {
        return 1.0;
    }
    let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let len = libm::sqrtf(crate::vec::fx_vec3_length_sq(delta));
    if len < min_trace_dist {
        return 1.0;
    }
    let direction = crate::vec::fx_vec3_normalize(delta);
    let half_len = len * 0.5;
    let mut visibility = 1.0;
    let count = buf.count.min(FX_VIS_BLOCKER_SLOT_CAP - 1) as usize;

    for blocker in buf.recs.iter().take(count) {
        let to_blocker = [
            blocker.origin[0] - start[0],
            blocker.origin[1] - start[1],
            blocker.origin[2] - start[2],
        ];
        let projection = to_blocker[0] * direction[0]
            + to_blocker[1] * direction[1]
            + to_blocker[2] * direction[2];
        if (projection - half_len).abs() > half_len {
            continue;
        }
        let nearest = [
            start[0] + projection * direction[0],
            start[1] + projection * direction[1],
            start[2] + projection * direction[2],
        ];
        let offset = [
            nearest[0] - blocker.origin[0],
            nearest[1] - blocker.origin[1],
            nearest[2] - blocker.origin[2],
        ];
        let distance_sq = offset[0] * offset[0] + offset[1] * offset[1] + offset[2] * offset[2];
        let radius = u16::from_ne_bytes(blocker.param3_x16.to_ne_bytes()) as f32
            / FX_VIS_BLOCKER_PARAM3_SCALE as f32;
        if distance_sq < radius * radius {
            let fixed_visibility =
                u16::from_ne_bytes(blocker.one_minus_param4_x16.to_ne_bytes()) as f32;
            visibility *= fixed_visibility / FX_VIS_BLOCKER_PARAM4_INV_SCALE as f32;
        }
    }
    visibility
}
