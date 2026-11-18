use trace_iw4::{Trace, trace_shape};

use crate::{BrushView, TraceExtents};

#[derive(Clone, Copy, Debug)]
pub struct ClipCmodel {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub radius: f32,
    pub first_brush: u32,
    pub num_brushes: u16,
}

pub fn clip_handle_to_model(cmodels: &[ClipCmodel], handle: u32) -> Option<&ClipCmodel> {
    cmodels.get(handle as usize)
}

pub fn transformed_box_sight_trace<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    ext: &TraceExtents,
    origin: [f32; 3],
    angles: [f32; 3],
) -> bool {
    let axis = angles_to_axis(angles);
    let half: [f32; 3] = core::array::from_fn(|i| (ext.maxs[i] - ext.mins[i]) * 0.5);
    let mid: [f32; 3] = core::array::from_fn(|i| (ext.maxs[i] + ext.mins[i]) * 0.5);
    let mut start = core::array::from_fn(|i| ext.start[i] + mid[i] - origin[i]);
    let mut end = core::array::from_fn(|i| ext.end[i] + mid[i] - origin[i]);
    rotate_point(&mut start, &axis);
    rotate_point(&mut end, &axis);
    let shape = ext.shape.map_axes(|mut v| {
        rotate_point(&mut v, &axis);
        v
    });
    let first = cmodel.first_brush as usize;
    let last = first + usize::from(cmodel.num_brushes);
    for &index in leafbrushes.get(first..last).unwrap_or(&[]) {
        let Some(brush) = brushes.get(usize::from(index)) else {
            continue;
        };
        let brush = crate::brush_ref(brush);
        if brush.contents & ext.mask == 0 || brush.planes.is_empty() {
            continue;
        }
        let (mut enter, mut leave) = (0.0f32, 1.0f32);
        let mut outside = false;
        for plane in brush.planes {
            let radius: f32 = match shape {
                trace_iw4::TraceShape::Capsule => (0..3).map(|i| plane[i].abs() * half[i]).sum(),
                trace_iw4::TraceShape::Box { half_size, axes } => (0..3)
                    .map(|i| {
                        (0..3).map(|j| plane[j] * axes[i][j]).sum::<f32>().abs() * half_size[i]
                    })
                    .sum(),
            };
            let a: f32 = (0..3).map(|i| plane[i] * start[i]).sum::<f32>() - plane[3] - radius;
            let b: f32 = (0..3).map(|i| plane[i] * end[i]).sum::<f32>() - plane[3] - radius;
            if a > 0.0 && b > 0.0 {
                outside = true;
                break;
            }
            if a <= 0.0 && b <= 0.0 {
                continue;
            }
            let fraction = a / (a - b);
            if a > b {
                enter = enter.max(fraction);
            } else {
                leave = leave.min(fraction);
            }
            if enter > leave {
                outside = true;
                break;
            }
        }
        if !outside {
            return true;
        }
    }
    false
}

pub fn transformed_capsule_trace<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    ext: &TraceExtents,
    origin: [f32; 3],
    angles: [f32; 3],
) -> Trace {
    if angles[0] == 0.0 && angles[1] == 0.0 && angles[2] == 0.0 {
        return transformed_capsule_trace_origin_only(cmodel, leafbrushes, brushes, ext, origin);
    }
    transformed_capsule_trace_rotated(cmodel, leafbrushes, brushes, ext, origin, angles)
}

fn transformed_capsule_trace_origin_only<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    ext: &TraceExtents,
    origin: [f32; 3],
) -> Trace {
    let local = TraceExtents {
        start: core::array::from_fn(|i| ext.start[i] - origin[i]),
        end: core::array::from_fn(|i| ext.end[i] - origin[i]),
        ..*ext
    };
    let mut hit = capsule_vs_cmodel(cmodel, leafbrushes, brushes, &local);
    hit.endpos = core::array::from_fn(|i| hit.endpos[i] + origin[i]);
    hit
}

fn transformed_capsule_trace_rotated<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    ext: &TraceExtents,
    origin: [f32; 3],
    angles: [f32; 3],
) -> Trace {
    let matrix = angles_to_axis(angles);
    let mut local = TraceExtents {
        start: core::array::from_fn(|i| ext.start[i] - origin[i]),
        end: core::array::from_fn(|i| ext.end[i] - origin[i]),
        ..*ext
    };
    rotate_point(&mut local.start, &matrix);
    rotate_point(&mut local.end, &matrix);
    local.shape = local.shape.map_axes(|mut axis| {
        rotate_point(&mut axis, &matrix);
        axis
    });
    if matches!(local.shape, trace_iw4::TraceShape::Box { .. }) {
        let mid = core::array::from_fn::<_, 3, _>(|i| (ext.mins[i] + ext.maxs[i]) * 0.5);
        let mut rotated_mid = mid;
        rotate_point(&mut rotated_mid, &matrix);
        for i in 0..3 {
            let shift = rotated_mid[i] - mid[i];
            local.mins[i] += shift;
            local.maxs[i] += shift;
        }
    }

    let mut hit = capsule_vs_cmodel(cmodel, leafbrushes, brushes, &local);
    if hit.fraction < 1.0 {
        let transpose = transpose_matrix(&matrix);
        rotate_point(&mut hit.normal, &transpose);
    }
    let inv = transpose_matrix(&matrix);
    rotate_point(&mut hit.endpos, &inv);
    hit.endpos = core::array::from_fn(|i| hit.endpos[i] + origin[i]);
    hit
}

fn capsule_vs_cmodel<B: BrushView>(
    cmodel: &ClipCmodel,
    leafbrushes: &[u16],
    brushes: &[B],
    ext: &TraceExtents,
) -> Trace {
    let first = cmodel.first_brush as usize;
    let last = first.saturating_add(cmodel.num_brushes as usize);
    let ids = leafbrushes.get(first..last).unwrap_or(&[]);
    let selected = ids
        .iter()
        .filter_map(|&id| brushes.get(id as usize))
        .map(crate::brush_ref);
    trace_shape(
        selected, ext.start, ext.end, ext.mins, ext.maxs, ext.mask, ext.shape,
    )
}

fn angles_to_axis(angles: [f32; 3]) -> [[f32; 3]; 3] {
    const DEG2RAD: f32 = 0.01745329238474369_f32;
    let yaw = angles[1] * DEG2RAD;
    let pitch = angles[0] * DEG2RAD;
    let roll = angles[2] * DEG2RAD;
    let cy = libm::cosf(yaw);
    let sy = libm::sinf(yaw);
    let cp = libm::cosf(pitch);
    let sp = libm::sinf(pitch);
    let cr = libm::cosf(roll);
    let sr = libm::sinf(roll);
    [
        [cp * cy, cp * sy, -sp],
        [sr * sp * cy + -sy * cr, sr * sp * sy + cr * cy, sr * cp],
        [cr * sp * cy + -sr * -sy, cr * sp * sy + -sr * cy, cr * cp],
    ]
}

fn rotate_point(point: &mut [f32; 3], mat: &[[f32; 3]; 3]) {
    let x = point[0];
    let y = point[1];
    let z = point[2];
    point[0] = mat[0][0] * x + mat[0][1] * y + mat[0][2] * z;
    point[1] = mat[1][0] * x + mat[1][1] * y + mat[1][2] * z;
    point[2] = mat[2][0] * x + mat[2][1] * y + mat[2][2] * z;
}

fn transpose_matrix(mat: &[[f32; 3]; 3]) -> [[f32; 3]; 3] {
    [
        [mat[0][0], mat[1][0], mat[2][0]],
        [mat[0][1], mat[1][1], mat[2][1]],
        [mat[0][2], mat[1][2], mat[2][2]],
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct CmodelTransform {
    origin: [f32; 3],
    matrix: [[f32; 3]; 3],
}

impl CmodelTransform {
    pub fn new(origin: [f32; 3], angles: [f32; 3]) -> Option<Self> {
        if !origin.iter().chain(angles.iter()).all(|v| v.is_finite()) {
            return None;
        }
        Some(Self {
            origin,
            matrix: angles_to_axis(angles),
        })
    }

    pub fn local_point(self, point: [f32; 3]) -> [f32; 3] {
        let mut local = core::array::from_fn(|i| point[i] - self.origin[i]);
        rotate_point(&mut local, &self.matrix);
        local
    }

    pub fn world_direction(self, mut direction: [f64; 3]) -> [f64; 3] {
        let local = direction;
        for (i, value) in direction.iter_mut().enumerate() {
            *value = (0..3)
                .map(|j| f64::from(self.matrix[j][i]) * local[j])
                .sum();
        }
        direction
    }
}
