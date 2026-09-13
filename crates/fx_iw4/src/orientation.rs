use crate::flags::{
    FX_ELEM_RUN_MASK, FX_ELEM_RUN_NONE_ORIGIN, FX_ELEM_RUN_RELATIVE_TO_EFFECT,
    FX_ELEM_RUN_RELATIVE_TO_OFFSET, fx_elem_run_mode,
};
use crate::origin::{
    FX_ELEM_SPAWN_OFFSET_SPHERE, fx_apply_spawn_origin, fx_elem_spawn_relative,
    fx_offset_spawn_origin,
};
use crate::vec::{fx_vec3_length_sq, fx_vec3_normalize, fx_vector_vectors};

pub const FX_ORIENT_UP_DOT_GATE: f64 = 0.999_000_012_874_603_3;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxOrientation {
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxOrientFrame {
    pub origin: [f32; 3],
    pub axis: [[f32; 3]; 3],
}

impl FxOrientFrame {
    pub const IDENTITY: Self = Self {
        origin: [0.0; 3],
        axis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };
}

#[derive(Clone, Copy, Debug)]
pub struct FxOrientSpawnParams {
    pub spawn_origin: [[f32; 2]; 3],
    pub spawn_offset_radius: [f32; 2],
    pub spawn_offset_height: [f32; 2],
    pub seed: u32,
}

#[inline]
pub fn fx_get_orientation(
    flags: i32,
    effect_now: &FxOrientFrame,
    effect_alt: &FxOrientFrame,
    spawn: Option<FxOrientSpawnParams>,
) -> FxOrientation {
    let mode = fx_elem_run_mode(flags);
    match mode {
        0 => FxOrientation {
            origin: [0.0; 3],
            axis: FxOrientFrame::IDENTITY.axis,
        },
        m if m == FX_ELEM_RUN_RELATIVE_TO_EFFECT => FxOrientation {
            origin: effect_now.origin,
            axis: effect_now.axis,
        },
        m if m == FX_ELEM_RUN_RELATIVE_TO_OFFSET => FxOrientation {
            origin: effect_alt.origin,
            axis: effect_alt.axis,
        },
        _ => {
            let Some(sp) = spawn else {
                return FxOrientation {
                    origin: effect_now.origin,
                    axis: effect_now.axis,
                };
            };
            fx_get_orientation_spawn_built(flags, effect_now, sp)
        }
    }
}

fn fx_get_orientation_spawn_built(
    flags: i32,
    effect_now: &FxOrientFrame,
    sp: FxOrientSpawnParams,
) -> FxOrientation {
    let relative = fx_elem_spawn_relative(flags);
    let mut origin = fx_apply_spawn_origin(
        effect_now.origin,
        effect_now.axis,
        sp.spawn_origin,
        sp.seed,
        relative,
    );
    let mut offset = [0.0f32; 3];
    fx_offset_spawn_origin(
        &mut offset,
        effect_now.axis,
        flags,
        sp.spawn_offset_radius[0],
        sp.spawn_offset_radius[1],
        sp.spawn_offset_height[0],
        sp.spawn_offset_height[1],
        sp.seed,
    );
    origin[0] += offset[0];
    origin[1] += offset[1];
    origin[2] += offset[2];

    let mut forward = offset;
    let len_sq = fx_vec3_length_sq(forward);
    if len_sq <= 1e-12 {
        forward = [1.0, 0.0, 0.0];
    } else {
        forward = fx_vec3_normalize(forward);
    }

    let axis = if (flags & 0x30) == FX_ELEM_SPAWN_OFFSET_SPHERE {
        fx_vector_vectors(forward)
    } else {
        let up_guess = if libm::fabsf(forward[2]) < (FX_ORIENT_UP_DOT_GATE as f32) {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        let right = fx_vec3_normalize([
            up_guess[1] * forward[2] - up_guess[2] * forward[1],
            up_guess[2] * forward[0] - up_guess[0] * forward[2],
            up_guess[0] * forward[1] - up_guess[1] * forward[0],
        ]);
        let up = [
            forward[1] * right[2] - forward[2] * right[1],
            forward[2] * right[0] - forward[0] * right[2],
            forward[0] * right[1] - forward[1] * right[0],
        ];
        [forward, right, up]
    };

    let _ = (FX_ELEM_RUN_MASK, FX_ELEM_RUN_NONE_ORIGIN);
    FxOrientation { origin, axis }
}
