use playerstate_iw4::{ENTITYNUM_NONE, PlayerState};

use crate::{
    CollisionBackend, GroundTraceInput, PMF_LADDER, PMF_PRONE, Pml, jump_clear_state,
    jump_get_step_height,
};

const PMF_JUMPING: u32 = 0x2000;

const OVERCLIP: f32 = 1.001;

const PROJECT_NORMAL_Z_EPSILON: f32 = 0.001;

const MAX_CLIP_PLANES: usize = 8;

const STEP_SIZE: f32 = 18.0;

const PRONE_STEP_SIZE: f32 = 10.0;

const STEP_UP_EXTRA: f32 = 1.0;

const SECONDARY_LANDING_NORMAL_Z: f32 = 0.3;

const STEP_GROUND_NORMAL_Z: f32 = 0.9;

const WALKING_STEP_DOWN_EXTRA: f32 = 9.0;

const STEP_PROGRESS_EPSILON: f32 = 0.001;

const DUPLICATE_PLANE_DOT: f64 = 0.999_000_012_874_603_3;

const PLANE_INTO: f32 = 0.1;

pub fn pm_slide_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    collision: &C,
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    gravity: Option<f32>,
) -> bool {
    let mut end_velocity = ps.velocity;
    if let Some(g) = gravity {
        end_velocity[2] -= g * pml.frametime;
        ps.velocity[2] = (ps.velocity[2] + end_velocity[2]) * 0.5;
        if pml.ground_plane != 0 {
            let normal = [
                f32::from_bits(pml.ground_trace[1]),
                f32::from_bits(pml.ground_trace[2]),
                f32::from_bits(pml.ground_trace[3]),
            ];
            let velocity = ps.velocity;
            clip_velocity(&velocity, &normal, &mut ps.velocity);
        }
    }

    let mut planes = [[0.0_f32; 3]; MAX_CLIP_PLANES];
    let mut plane_count = 0usize;
    if pml.ground_plane != 0 {
        planes[0] = [
            f32::from_bits(pml.ground_trace[1]),
            f32::from_bits(pml.ground_trace[2]),
            f32::from_bits(pml.ground_trace[3]),
        ];
        plane_count = 1;
    }
    let mut dir = ps.velocity;
    let _ = normalize(&mut dir);
    if plane_count < MAX_CLIP_PLANES {
        planes[plane_count] = dir;
        plane_count += 1;
    }

    let mut time_left = pml.frametime;
    let mut bumped = false;
    for _ in 0..4 {
        if time_left <= 0.0 {
            break;
        }
        let end = [
            ps.origin[0] + ps.velocity[0] * time_left,
            ps.origin[1] + ps.velocity[1] * time_left,
            ps.origin[2] + ps.velocity[2] * time_left,
        ];
        let trace = collision.trace(GroundTraceInput {
            start: ps.origin,
            end,
            mins,
            maxs,
            tracemask,
        });
        if trace.allsolid != 0 {
            ps.velocity[2] = 0.0;
            return true;
        }
        if trace.fraction > 0.0 {
            if trace.fraction >= 1.0 {
                ps.origin = end;
            } else {
                ps.origin = trace.endpos;
            }
        }
        if trace.fraction >= 1.0 {
            break;
        }
        bumped = true;
        time_left *= 1.0 - trace.fraction;
        if plane_count >= MAX_CLIP_PLANES {
            ps.velocity = [0.0; 3];
            return true;
        }

        let mut duplicate = false;
        for plane in planes.iter().take(plane_count) {
            if f64::from(dot(&trace.normal, plane)) > DUPLICATE_PLANE_DOT {
                duplicate = true;
                break;
            }
        }
        if duplicate {
            let velocity = ps.velocity;
            clip_velocity(&velocity, &trace.normal, &mut ps.velocity);
            ps.velocity[0] += trace.normal[0];
            ps.velocity[1] += trace.normal[1];
            ps.velocity[2] += trace.normal[2];
            continue;
        }
        planes[plane_count] = trace.normal;
        plane_count += 1;

        let mut permutation = [0usize; MAX_CLIP_PLANES];
        let into =
            permute_restrictive_clip_planes(&ps.velocity, &planes[..plane_count], &mut permutation);
        if into < PLANE_INTO {
            let first = permutation[0];
            let mut clip_velocity_out = [0.0_f32; 3];
            let mut end_clip = [0.0_f32; 3];
            clip_velocity(&ps.velocity, &planes[first], &mut clip_velocity_out);
            clip_velocity(&end_velocity, &planes[first], &mut end_clip);
            for j in 1..plane_count {
                let pj = permutation[j];
                if dot(&clip_velocity_out, &planes[pj]) >= PLANE_INTO {
                    continue;
                }
                let current = clip_velocity_out;
                clip_velocity(&current, &planes[pj], &mut clip_velocity_out);
                let current_end = end_clip;
                clip_velocity(&current_end, &planes[pj], &mut end_clip);
                if dot(&clip_velocity_out, &planes[first]) >= 0.0 {
                    continue;
                }
                let mut crease = cross(&planes[first], &planes[pj]);
                let _ = normalize(&mut crease);
                let along = dot(&ps.velocity, &crease);
                clip_velocity_out = [crease[0] * along, crease[1] * along, crease[2] * along];
                let along_end = dot(&end_velocity, &crease);
                end_clip = [
                    crease[0] * along_end,
                    crease[1] * along_end,
                    crease[2] * along_end,
                ];
                for k in 1..plane_count {
                    if k == j {
                        continue;
                    }
                    let pk = permutation[k];
                    if dot(&clip_velocity_out, &planes[pk]) < PLANE_INTO {
                        ps.velocity = [0.0; 3];
                        return true;
                    }
                }
            }
            ps.velocity = clip_velocity_out;
            end_velocity = end_clip;
        }
    }

    if gravity.is_some() {
        ps.velocity = end_velocity;
    }
    bumped
}

pub fn pm_step_slide_move<C: CollisionBackend>(
    ps: &mut PlayerState,
    pml: &Pml,
    collision: &C,
    mins: [f32; 3],
    maxs: [f32; 3],
    tracemask: u32,
    gravity: Option<f32>,
) {
    let had_ground = if (ps.pm_flags & PMF_LADDER) != 0 {
        jump_clear_state(ps);
        false
    } else if pml.ground_plane != 0 {
        true
    } else {
        if (ps.pm_flags & PMF_JUMPING) != 0 && ps.pm_time != 0 {
            jump_clear_state(ps);
        }
        false
    };

    let start_origin = ps.origin;
    let start_velocity = ps.velocity;

    let bumped = pm_slide_move(ps, pml, collision, mins, maxs, tracemask, gravity);
    let down_origin = ps.origin;
    let down_velocity = ps.velocity;

    let mut step_size = if (ps.pm_flags & PMF_PRONE) != 0 {
        PRONE_STEP_SIZE
    } else {
        STEP_SIZE
    };

    if ps.ground_entity_num == ENTITYNUM_NONE {
        if (ps.pm_flags & PMF_JUMPING) != 0 && ps.pm_time != 0 {
            jump_clear_state(ps);
        }
        let jumping = (ps.pm_flags & PMF_JUMPING) != 0;
        let ladder_up = (ps.pm_flags & PMF_LADDER) != 0 && ps.velocity[2] > 0.0;
        if bumped && jumping {
            match jump_get_step_height(ps, start_origin) {
                Some(height) if height < 1.0 => return,
                Some(height) => step_size = height,
                None if !ladder_up => return,
                None => {}
            }
        } else if !ladder_up {
            return;
        }
    }

    let ground_normal_z = f32::from_bits(pml.ground_trace[3]);
    let sloped_ground = pml.ground_plane != 0 && ground_normal_z < STEP_GROUND_NORMAL_Z;
    let mut step_height = 0.0;

    if bumped || sloped_ground {
        let up = collision.trace(GroundTraceInput {
            start: start_origin,
            end: [
                start_origin[0],
                start_origin[1],
                start_origin[2] + step_size + STEP_UP_EXTRA,
            ],
            mins,
            maxs,
            tracemask,
        });

        let step_amount = (step_size + STEP_UP_EXTRA) * up.fraction - STEP_UP_EXTRA;
        if step_amount >= 1.0 {
            step_height = step_amount;
            ps.origin = [
                start_origin[0],
                start_origin[1],
                start_origin[2] + step_amount,
            ];
            ps.velocity = start_velocity;
            let _ = pm_slide_move(ps, pml, collision, mins, maxs, tracemask, gravity);
        }
    }

    if had_ground || step_height != 0.0 {
        let mut descent = step_height;
        if had_ground {
            descent += WALKING_STEP_DOWN_EXTRA;
        }
        let down_target = [ps.origin[0], ps.origin[1], ps.origin[2] - descent];
        let down = collision.trace(GroundTraceInput {
            start: ps.origin,
            end: down_target,
            mins,
            maxs,
            tracemask,
        });
        if down.fraction >= 1.0 {
            if step_height != 0.0 {
                ps.origin[2] -= step_height;
            }
        } else if down.walkable != 0 || down.normal[2] >= SECONDARY_LANDING_NORMAL_Z {
            ps.origin = down.endpos;

            pm_project_velocity(&mut ps.velocity, &down.normal);
        } else {
            ps.origin = down_origin;
            ps.velocity = down_velocity;
            return;
        }
    }

    let gained = start_velocity[0] * (ps.origin[0] - start_origin[0])
        + start_velocity[1] * (ps.origin[1] - start_origin[1]);
    let flat = start_velocity[0] * (down_origin[0] - start_origin[0])
        + start_velocity[1] * (down_origin[1] - start_origin[1]);
    if gained <= flat + STEP_PROGRESS_EPSILON {
        ps.origin = down_origin;
        ps.velocity = down_velocity;

        if had_ground {
            let down_target = [
                ps.origin[0],
                ps.origin[1],
                ps.origin[2] - WALKING_STEP_DOWN_EXTRA,
            ];
            let down = collision.trace(GroundTraceInput {
                start: ps.origin,
                end: down_target,
                mins,
                maxs,
                tracemask,
            });
            if down.fraction < 1.0 {
                ps.origin = down.endpos;
                let velocity = ps.velocity;
                clip_velocity(&velocity, &down.normal, &mut ps.velocity);
            }
        }
    }
}

pub(crate) fn pm_project_velocity(velocity: &mut [f32; 3], normal: &[f32; 3]) {
    let length_sq_2d = velocity[0] * velocity[0] + velocity[1] * velocity[1];
    if libm::fabsf(normal[2]) < PROJECT_NORMAL_Z_EPSILON || length_sq_2d == 0.0 {
        return;
    }
    let new_z = -(normal[1] * velocity[1] + normal[0] * velocity[0]) / normal[2];
    let original_length_sq = velocity[2] * velocity[2] + length_sq_2d;
    let adjusted_length_sq = new_z * new_z + length_sq_2d;
    let length_scale = libm::sqrtf(original_length_sq / adjusted_length_sq);
    if length_scale >= 1.0 && new_z >= 0.0 && velocity[2] <= 0.0 {
        return;
    }
    velocity[0] *= length_scale;
    velocity[1] *= length_scale;
    velocity[2] = new_z * length_scale;
}

fn permute_restrictive_clip_planes(
    velocity: &[f32; 3],
    planes: &[[f32; 3]],
    permutation: &mut [usize],
) -> f32 {
    let mut parallel = [0.0_f32; MAX_CLIP_PLANES];
    for (plane_index, plane) in planes.iter().enumerate() {
        parallel[plane_index] = dot(velocity, plane);
        let mut permuted_index = plane_index;
        while permuted_index > 0
            && parallel[plane_index] <= parallel[permutation[permuted_index - 1]]
        {
            permutation[permuted_index] = permutation[permuted_index - 1];
            permuted_index -= 1;
        }
        permutation[permuted_index] = plane_index;
    }
    parallel[permutation[0]]
}

fn clip_velocity(input: &[f32; 3], normal: &[f32; 3], output: &mut [f32; 3]) {
    let d = input[0] * normal[0] + input[1] * normal[1] + input[2] * normal[2];
    let backoff = -(d - d.abs() * (OVERCLIP - 1.0));
    output[0] = input[0] + backoff * normal[0];
    output[1] = input[1] + backoff * normal[1];
    output[2] = input[2] + backoff * normal[2];
}

fn dot(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: &[f32; 3], b: &[f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(v: &mut [f32; 3]) -> f32 {
    let len = libm::sqrtf(v[0] * v[0] + v[1] * v[1] + v[2] * v[2]);
    let scale = if len <= 0.0 { 1.0 } else { 1.0 / len };
    v[0] *= scale;
    v[1] *= scale;
    v[2] *= scale;
    len
}
