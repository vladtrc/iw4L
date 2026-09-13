use trace_iw4::Trace;

const WALKABLE_NORMAL_Z: f32 = 0.7;

const SURFACE_CLIP_EPSILON: f32 = 0.125;

#[derive(Clone, Copy, Debug)]
struct CapsuleSize {
    offset: [f32; 3],
    radius: f32,
    offset_z: f32,
    size: [f32; 3],
}

impl CapsuleSize {
    fn from_bounds(mins: [f32; 3], maxs: [f32; 3]) -> Self {
        let offset = [
            (mins[0] + maxs[0]) * 0.5,
            (mins[1] + maxs[1]) * 0.5,
            (mins[2] + maxs[2]) * 0.5,
        ];
        let size = [
            maxs[0] - offset[0],
            maxs[1] - offset[1],
            maxs[2] - offset[2],
        ];
        let radius = if size[0] <= size[2] { size[0] } else { size[2] };
        let offset_z = size[2] - radius;
        Self {
            offset,
            radius,
            offset_z,
            size,
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "preserves the CM_TransformedBoxTrace moving-hull, temp-model, and contents inputs"
)]
pub fn transformed_temp_capsule_trace(
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    origin: [f32; 3],
    box_mins: [f32; 3],
    box_maxs: [f32; 3],
    contents: u32,
    mask: u32,
) -> Trace {
    let mut hit = Trace {
        fraction: 1.0,
        endpos: end,
        ..Trace::default()
    };
    if contents & mask == 0 {
        return hit;
    }

    let mover = CapsuleSize::from_bounds(mins, maxs);
    let start_l = [
        start[0] + mover.offset[0] - origin[0],
        start[1] + mover.offset[1] - origin[1],
        start[2] + mover.offset[2] - origin[2],
    ];
    let end_l = [
        end[0] + mover.offset[0] - origin[0],
        end[1] + mover.offset[1] - origin[1],
        end[2] + mover.offset[2] - origin[2],
    ];
    let stationary = CapsuleSize::from_bounds(box_mins, box_maxs);

    if start[0] == end[0] && start[1] == end[1] && start[2] == end[2] {
        test_capsule_in_capsule(&mut hit, start_l, mover, stationary, contents);
    } else {
        trace_capsule_through_capsule(&mut hit, start_l, end_l, mover, stationary, contents);
    }

    if hit.walkable == 0 && hit.startsolid == 0 {
        hit.walkable = u8::from(hit.normal[2] >= WALKABLE_NORMAL_Z);
    }
    let t = hit.fraction;
    hit.endpos = [
        start[0] + (end[0] - start[0]) * t,
        start[1] + (end[1] - start[1]) * t,
        start[2] + (end[2] - start[2]) * t,
    ];
    hit
}

fn test_capsule_in_capsule(
    trace: &mut Trace,
    start: [f32; 3],
    mover: CapsuleSize,
    stationary: CapsuleSize,
    contents: u32,
) {
    let top = [start[0], start[1], start[2] + mover.offset_z];
    let bottom = [start[0], start[1], start[2] - mover.offset_z];
    let r = (mover.radius + stationary.radius) * (mover.radius + stationary.radius);
    let p1 = [
        stationary.offset[0],
        stationary.offset[1],
        stationary.offset[2] + stationary.offset_z,
    ];
    let p2 = [
        stationary.offset[0],
        stationary.offset[1],
        stationary.offset[2] - stationary.offset_z,
    ];
    if dist_sq(p1, top) < r
        || dist_sq(p1, bottom) < r
        || dist_sq(p2, top) < r
        || dist_sq(p2, bottom) < r
    {
        mark_allsolid(trace, contents);
        return;
    }
    let height_diff = start[2] - stationary.offset[2];
    let total_half = stationary.offset_z + mover.size[2] - mover.radius;
    if total_half >= height_diff.abs() {
        let a = [top[0], top[1], 0.0];
        let b = [p1[0], p1[1], 0.0];
        if dist_sq(a, b) < r {
            mark_allsolid(trace, contents);
        }
    }
}

fn mark_allsolid(trace: &mut Trace, contents: u32) {
    trace.fraction = 0.0;
    trace.startsolid = 1;
    trace.allsolid = 1;
    trace.surface_flags = 0;
    trace.contents = contents;
    trace.walkable = 0;
}

fn trace_capsule_through_capsule(
    trace: &mut Trace,
    start: [f32; 3],
    end: [f32; 3],
    mover: CapsuleSize,
    stationary: CapsuleSize,
    contents: u32,
) {
    let delta = [end[0] - start[0], end[1] - start[1], end[2] - start[2]];
    let delta_len_sq = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
    let start_top = [start[0], start[1], start[2] + mover.offset_z];
    let start_bottom = [start[0], start[1], start[2] - mover.offset_z];
    let end_top = [end[0], end[1], end[2] + mover.offset_z];
    let end_bottom = [end[0], end[1], end[2] - mover.offset_z];
    let top = [
        stationary.offset[0],
        stationary.offset[1],
        stationary.offset[2] + stationary.offset_z,
    ];
    let bottom = [
        stationary.offset[0],
        stationary.offset[1],
        stationary.offset[2] - stationary.offset_z,
    ];
    let work = SphereWork {
        delta,
        delta_len_sq,
        radius: mover.radius,
        size_z: mover.size[2],
        contents,
    };

    if top[2] >= start_bottom[2] {
        if bottom[2] > start_top[2]
            && (!trace_sphere_through_sphere(
                &work,
                start_top,
                end_top,
                bottom,
                stationary.radius,
                trace,
            ) || delta[2] <= 0.0)
        {
            return;
        }
    } else if !trace_sphere_through_sphere(
        &work,
        start_bottom,
        end_bottom,
        top,
        stationary.radius,
        trace,
    ) || delta[2] >= 0.0
    {
        return;
    }

    if trace_cylinder_through_cylinder(
        &work,
        start,
        stationary.offset,
        stationary.offset_z,
        stationary.radius,
        trace,
    ) {
        if top[2] >= end_bottom[2] {
            if bottom[2] > end_top[2] && bottom[2] <= start_top[2] {
                let _ = trace_sphere_through_sphere(
                    &work,
                    start_top,
                    end_top,
                    bottom,
                    stationary.radius,
                    trace,
                );
            }
        } else if top[2] >= start_bottom[2] {
            let _ = trace_sphere_through_sphere(
                &work,
                start_bottom,
                end_bottom,
                top,
                stationary.radius,
                trace,
            );
        }
    }
}

struct SphereWork {
    delta: [f32; 3],
    delta_len_sq: f32,
    radius: f32,
    size_z: f32,
    contents: u32,
}

fn trace_sphere_through_sphere(
    tw: &SphereWork,
    v_start: [f32; 3],
    v_end: [f32; 3],
    v_stationary: [f32; 3],
    radius: f32,
    trace: &mut Trace,
) -> bool {
    let v_delta = [
        v_start[0] - v_stationary[0],
        v_start[1] - v_stationary[1],
        v_start[2] - v_stationary[2],
    ];
    let radius_sq = (radius + tw.radius) * (radius + tw.radius);
    let f_c = dot(v_delta, v_delta) - radius_sq;
    if f_c <= 0.0 {
        trace.fraction = 0.0;
        trace.startsolid = 1;
        trace.walkable = 0;
        let (_, n) = vec3_normalize_to(v_delta);
        trace.normal = n;
        trace.contents = tw.contents;
        trace.surface_flags = 0;
        let end_delta = [
            v_end[0] - v_stationary[0],
            v_end[1] - v_stationary[1],
            v_end[2] - v_stationary[2],
        ];
        if radius_sq >= dot(end_delta, end_delta) {
            trace.allsolid = 1;
        }
        return false;
    }
    let f_b = dot(tw.delta, v_delta);
    if f_b >= 0.0 || tw.delta_len_sq <= 0.0 {
        return true;
    }
    let f_a = tw.delta_len_sq;
    let disc = f_b * f_b - f_a * f_c;
    if disc < 0.0 {
        return true;
    }
    let (f_delta_len, v_normal) = vec3_normalize_to(v_delta);
    let root = libm::sqrtf(disc);
    let f_entry = (-f_b - root) / f_a + f_delta_len * SURFACE_CLIP_EPSILON / f_b;
    if trace.fraction <= f_entry {
        return true;
    }
    let frac = f_entry.max(0.0);
    trace.fraction = frac.clamp(0.0, 1.0);
    trace.normal = v_normal;
    trace.contents = tw.contents;
    trace.walkable = 0;
    trace.surface_flags = 0;
    false
}

fn trace_cylinder_through_cylinder(
    tw: &SphereWork,
    start: [f32; 3],
    v_stationary: [f32; 3],
    f_stationary_half_height: f32,
    radius: f32,
    trace: &mut Trace,
) -> bool {
    let mut v_delta = [
        start[0] - v_stationary[0],
        start[1] - v_stationary[1],
        start[2] - v_stationary[2],
    ];
    let radius_sq = (radius + tw.radius) * (radius + tw.radius);
    let f_c = v_delta[0] * v_delta[0] + v_delta[1] * v_delta[1] - radius_sq;
    let f_total_height = tw.size_z - tw.radius + f_stationary_half_height;
    if f_c <= 0.0 {
        if f_total_height >= v_delta[2].abs() {
            trace.fraction = 0.0;
            trace.startsolid = 1;
            trace.walkable = 0;
            v_delta[2] = 0.0;
            let (_, n) = vec3_normalize_to(v_delta);
            trace.normal = n;
            trace.contents = tw.contents;
            trace.surface_flags = 0;
            let end_z = start[2] + tw.delta[2] - v_stationary[2];
            if f_total_height >= end_z.abs() {
                trace.allsolid = 1;
            }
            return false;
        }
        return true;
    }
    let f_b = v_delta[0] * tw.delta[0] + v_delta[1] * tw.delta[1];
    if f_b >= 0.0 {
        return true;
    }
    let f_a = tw.delta[0] * tw.delta[0] + tw.delta[1] * tw.delta[1];
    if f_a <= 0.0 {
        return true;
    }
    let disc = f_b * f_b - f_a * f_c;
    if disc < 0.0 {
        return true;
    }
    v_delta[2] = 0.0;
    let (f_delta_len, v_normal) = vec3_normalize_to(v_delta);
    let f_epsilon = f_delta_len * SURFACE_CLIP_EPSILON / f_b;
    let root = libm::sqrtf(disc);
    let f_entry = (-f_b - root) / f_a + f_epsilon;
    if trace.fraction <= f_entry {
        return true;
    }
    let f_hit_height = (f_entry - f_epsilon) * tw.delta[2] + start[2] - v_stationary[2];
    if f_total_height < f_hit_height.abs() {
        return true;
    }
    let frac = f_entry.max(0.0);
    trace.fraction = frac.clamp(0.0, 1.0);
    trace.normal = v_normal;
    trace.contents = tw.contents;
    trace.surface_flags = 0;
    trace.walkable = 0;
    false
}

fn dist_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    dot(d, d)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec3_normalize_to(v: [f32; 3]) -> (f32, [f32; 3]) {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    if len == 0.0 {
        return (0.0, [0.0, 0.0, 1.0]);
    }
    (len, [v[0] / len, v[1] / len, v[2] / len])
}
