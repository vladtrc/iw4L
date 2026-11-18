use trace_iw4::Trace;

type V3 = [f32; 3];

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Ground {
    pub valid: bool,
    pub pitch: Option<[f32; 2]>,
}

fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(v: V3, s: f32) -> V3 {
    v.map(|v| v * s)
}
fn lerp(a: V3, b: V3, f: f32) -> V3 {
    add(a, scale(sub(b, a), f))
}
fn normalized(v: V3) -> (V3, f32) {
    let length = v.iter().map(|v| v * v).sum::<f32>().sqrt();
    (
        scale(v, if length > 0. { length.recip() } else { 1. }),
        length,
    )
}
fn pitch(v: V3) -> f32 {
    let pitch = (-v[2]).atan2(v[0].hypot(v[1])).to_degrees();
    math_iw4::angle_subtract(pitch, 0.)
}

// BG_CheckProneValid's actor branch: radius15, height48, distance50,
// already prone/on ground, server handler2, actor mask0x820015.
pub(super) fn check(
    origin: V3,
    yaw: f32,
    feet_forward: bool,
    trace: &mut impl FnMut(V3, V3, V3, V3) -> Trace,
) -> Ground {
    let invalid = Ground::default();
    if trace(
        origin,
        add(origin, [0., 0., 10.]),
        [0., 0., 24.],
        [15., 15., 24.],
    )
    .allsolid
        != 0
    {
        return invalid;
    }
    let yaw = if feet_forward { yaw - 180. } else { yaw };
    let mut direction = math_iw4::angle_vectors([0., yaw, 0.]).0;
    let mut start = add(origin, [0., 0., 42.]);
    let mut end = add(start, scale(direction, 44.));
    let cube = [6.; 3];
    let mut hit = trace(start, end, [0.; 3], cube);
    let mut obstructed = hit.fraction < 1.;
    let mut reach = 50.;
    if obstructed {
        reach = 44. * hit.fraction + 6.;
        if reach < 17. {
            return invalid;
        }
        if reach < 47.4 {
            end[2] += 22.;
            let (raised, length) = normalized(sub(end, start));
            direction = raised;
            hit = trace(start, end, [0.; 3], cube);
            obstructed = hit.fraction < 1.;
            reach = if obstructed {
                length * hit.fraction + 6.
            } else {
                50.
            };
            if obstructed && reach < 47.4 {
                return invalid;
            }
        }
    }
    let mut foot = lerp(start, end, hit.fraction);
    start = add(add(origin, scale(direction, 18.)), [0., 0., 42.]);
    end = add(start, [0., 0., -73.5]);
    hit = trace(start, end, [0.; 3], cube);
    if hit.fraction == 1. || hit.walkable == 0 {
        return invalid;
    }
    let depth = 73.5 * hit.fraction + 6.;
    let mid = add(lerp(start, end, hit.fraction), [0., 0., -6.]);
    if obstructed {
        if reach - depth < -0.75 * depth {
            return invalid;
        }
        let length = 26.; // distance - cube radius - torso spacing
        let delta = add(sub(foot, mid), scale(direction, 6.));
        let delta = add(delta, [0., 0., 6.]);
        let (slope, _) = normalized(delta);
        end[0] = ((origin[0] + 44. * direction[0]) + (start[0] + length * slope[0])) * 0.5;
        end[1] = ((origin[1] + 44. * direction[1]) + (start[1] + length * slope[1])) * 0.5;
        end[2] = start[2] + length * slope[2];
        hit = trace(start, end, [0.; 3], cube);
        if hit.fraction < 1. {
            let previous = end;
            end[2] += 18.;
            start = add(lerp(start, previous, hit.fraction), [0., 0., 18.]);
            hit = trace(start, end, [0.; 3], cube);
            if hit.fraction < 1. {
                return invalid;
            }
        }
        foot = lerp(start, end, hit.fraction);
    }
    start = foot;
    end = add(foot, [0., 0., -((foot[2] - mid[2]) * 2. + 15.)]);
    hit = trace(start, end, [0.; 3], cube);
    if hit.fraction == 1. || hit.walkable == 0 {
        return invalid;
    }
    foot = add(lerp(start, end, hit.fraction), [0., 0., -6.]);
    let torso_pitch = pitch(if feet_forward {
        sub(origin, mid)
    } else {
        sub(mid, origin)
    });
    let waist_pitch = pitch(if feet_forward {
        sub(mid, foot)
    } else {
        sub(foot, mid)
    });
    let waist_pitch = waist_pitch.min(torso_pitch + 10.);
    let delta = math_iw4::angle_subtract(torso_pitch, waist_pitch);
    let mut valid = (-50. ..=70.).contains(&delta);
    let start = add(origin, [0., 0., 5.]);
    let end = add(mid, [0., 0., 5.]);
    if trace(start, end, [0.; 3], [0.; 3]).fraction < 1. {
        valid = false;
    }
    if trace(end, add(foot, [0., 0., 5.]), [0.; 3], [0.; 3]).fraction < 1. {
        valid = false;
    }
    Ground {
        valid,
        pitch: Some([torso_pitch, waist_pitch]),
    }
}
