use crate::capsule_query::Hit;

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSweepError {
    Invalid,
    IterationLimit,
}

// Exact closest segment/box distance: each interval between slab crossings
// has a quadratic distance function in the segment parameter.
fn segment_box_separation(a: [f64; 3], b: [f64; 3], half: [f64; 3]) -> [f64; 3] {
    let direction = core::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
    let mut cuts = [0.; 8];
    cuts[1] = 1.;
    let mut count = 2;
    for i in 0..3 {
        if direction[i] == 0. {
            continue;
        }
        for bound in [-half[i], half[i]] {
            let t = (bound - a[i]) / direction[i];
            if t > 0. && t < 1. {
                cuts[count] = t;
                count += 1;
            }
        }
    }
    cuts[..count].sort_by(f64::total_cmp);
    let mut best = [f64::INFINITY; 3];
    let mut distance = f64::INFINITY;
    for pair in cuts[..count].windows(2) {
        let mid = (pair[0] + pair[1]) * 0.5;
        let mut quadratic = 0.;
        let mut linear = 0.;
        for i in 0..3 {
            let point = a[i] + direction[i] * mid;
            let bound = point.clamp(-half[i], half[i]);
            if point != bound {
                quadratic += direction[i] * direction[i];
                linear += (a[i] - bound) * direction[i];
            }
        }
        let t = if quadratic > 0. {
            (-linear / quadratic).clamp(pair[0], pair[1])
        } else {
            mid
        };
        let separation = core::array::from_fn(|i| {
            let point = a[i] + direction[i] * t;
            point - point.clamp(-half[i], half[i])
        });
        let candidate = dot(separation, separation);
        if candidate < distance {
            distance = candidate;
            best = separation;
        }
    }
    best
}

pub fn sweep_box_capsule(
    start: [f64; 3],
    end: [f64; 3],
    half: [f64; 3],
    segment_a: [f64; 3],
    segment_b: [f64; 3],
    radius: f64,
) -> Result<Option<Hit>, BoxSweepError> {
    if [start, end, half, segment_a, segment_b]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite())
        || half.iter().any(|v| *v < 0.)
        || !radius.is_finite()
        || radius < 0.
    {
        return Err(BoxSweepError::Invalid);
    }
    for i in 0..3 {
        if start[i].min(end[i]) - half[i] > segment_a[i].max(segment_b[i]) + radius
            || start[i].max(end[i]) + half[i] < segment_a[i].min(segment_b[i]) - radius
        {
            return Ok(None);
        }
    }
    let delta = core::array::from_fn::<_, 3, _>(|i| end[i] - start[i]);
    if radius == 0. {
        let axes = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let vertices = [segment_a, segment_b].map(|v| core::array::from_fn(|i| v[i] - start[i]));
        let edge = core::array::from_fn(|i| segment_b[i] - segment_a[i]);
        let mut separating = [[0.; 3]; 6];
        separating[..3].copy_from_slice(&axes);
        for i in 0..3 {
            separating[3 + i] = cross(edge, axes[i]);
        }
        return Ok(sweep_axes(vertices, delta, half, axes, &separating, 0.));
    }

    let separation = |at: [f64; 3]| {
        segment_box_separation(
            core::array::from_fn(|i| segment_a[i] - at[i]),
            core::array::from_fn(|i| segment_b[i] - at[i]),
            half,
        )
    };
    let mut t = 0.;
    let mut last_normal = [0.; 3];
    for _ in 0..64 {
        let at = core::array::from_fn(|i| start[i] + delta[i] * t);
        let offset = separation(at);
        let distance = libm::sqrt(dot(offset, offset));
        let gap = distance - radius;
        let normal = if distance > 1e-12 {
            offset.map(|v| -v / distance)
        } else {
            last_normal
        };
        let closing = -dot(delta, normal);
        if gap <= 1e-7 {
            let startsolid = t == 0. && gap < 0.;
            if !startsolid && t == 0. && closing <= 0. {
                return Ok(None);
            }
            let finish = separation(end);
            return Ok(Some(Hit {
                fraction: t,
                normal,
                startsolid,
                allsolid: startsolid && dot(finish, finish) < radius * radius,
            }));
        }
        if closing <= 0. {
            return Ok(None);
        }
        last_normal = normal;
        let next = t + gap / closing;
        if next > 1. {
            return Ok(None);
        }
        if next <= t {
            return Err(BoxSweepError::IterationLimit);
        }
        t = next;
    }
    Err(BoxSweepError::IterationLimit)
}

pub(crate) fn sweep_triangle(
    start: [f32; 3],
    delta: [f32; 3],
    half: [f32; 3],
    axes: [[f32; 3]; 3],
    triangle: [[f32; 3]; 3],
) -> Option<Hit> {
    let vertices =
        triangle.map(|v| core::array::from_fn(|i| f64::from(v[i]) - f64::from(start[i])));
    let edges =
        [0, 1, 2].map(|i| core::array::from_fn(|j| vertices[(i + 1) % 3][j] - vertices[i][j]));
    let normal = cross(edges[0], edges[1]);
    if dot(normal, normal) <= 1e-20 {
        return None;
    }
    let axes = axes.map(|v| v.map(f64::from));
    let delta = delta.map(f64::from);
    let mut separating = [[0.; 3]; 13];
    separating[0] = normal;
    separating[1..4].copy_from_slice(&axes);
    for i in 0..3 {
        for j in 0..3 {
            separating[4 + 3 * i + j] = cross(axes[i], edges[j]);
        }
    }
    sweep_axes(
        vertices,
        delta,
        half.map(f64::from),
        axes,
        &separating,
        0.125,
    )
}

fn sweep_axes<const N: usize>(
    vertices: [[f64; 3]; N],
    delta: [f64; 3],
    half: [f64; 3],
    axes: [[f64; 3]; 3],
    separating: &[[f64; 3]],
    clip_epsilon: f64,
) -> Option<Hit> {
    let mut enter = f64::NEG_INFINITY;
    let mut leave = f64::INFINITY;
    let mut lead = [0.; 3];
    let mut lead_speed = 0.;
    let mut inside = true;
    let mut inside_end = true;
    for &axis in separating {
        let length = libm::sqrt(dot(axis, axis));
        if length <= 1e-12 {
            continue;
        }
        let axis = axis.map(|v| v / length);
        let projections = vertices.map(|v| dot(v, axis));
        let radius: f64 = (0..3).map(|i| dot(axes[i], axis).abs() * half[i]).sum();
        let low = projections.into_iter().fold(f64::INFINITY, f64::min) - radius;
        let high = projections.into_iter().fold(f64::NEG_INFINITY, f64::max) + radius;
        let speed = dot(delta, axis);
        inside &= low < 0. && high > 0.;
        inside_end &= low < speed && high > speed;
        if speed.abs() <= 1e-15 {
            // Sliding along an exact touching plane has no volume overlap.
            if low >= 0. || high <= 0. {
                return None;
            }
            continue;
        }
        let (first, last, facing) = if speed > 0. {
            (low / speed, high / speed, axis.map(|v| -v))
        } else {
            (high / speed, low / speed, axis)
        };
        if first > enter {
            enter = first;
            lead = facing;
            lead_speed = speed.abs();
        }
        leave = leave.min(last);
        if enter > leave {
            return None;
        }
    }
    if inside {
        return Some(Hit {
            fraction: 0.,
            normal: [0.; 3],
            startsolid: true,
            allsolid: inside_end,
        });
    }
    if enter < 0. || enter > 1. || leave < 0. {
        return None;
    }
    Some(Hit {
        fraction: (enter - clip_epsilon / lead_speed).max(0.),
        normal: lead,
        startsolid: false,
        allsolid: false,
    })
}
