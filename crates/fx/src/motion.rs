use fx_iw4::{
    FX_ELEM_AT_REST_NONE, FX_PHYS_PIECE_HALF, collide_marks_at_rest, collide_on_ground,
    collide_substep_schedule, collision_reflect_base_vel_delta, elem_dies_on_touch,
    elem_gravity_accel_z, elem_update_has_velocity_graph, elem_uses_collision, elem_uses_vel_local,
    elem_uses_vel_world, get_at_rest_fraction, impact_child_speed_allows,
    integrate_velocity_graph_sampled, orientation_pos_from_world, orientation_pos_to_world,
    sample_reflection_factor, trace_mask, vec3_length_sq,
};

use crate::update::{FxElemMotionQuery, FxElemMotionResult, FxElemTraceHit, FxImpactSpawn};

pub struct FxElemMotion<'a> {
    pub velocity: fx_iw4::FxVelocityField<'a>,
    pub gravity_base: f32,
    pub gravity_amp: f32,
}

pub struct FxElemCollision {
    pub reflection_base: f32,
    pub reflection_amp: f32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub use_item_clip: bool,
    pub has_effect_on_impact: bool,
    pub leaves_start_solid: bool,
}

impl FxElemMotion<'_> {
    pub fn evaluate(&self, q: FxElemMotionQuery<'_>) -> Option<FxElemMotionResult> {
        let flags = self.velocity.flags;
        if elem_uses_collision(flags) {
            return None;
        }

        Some(self.integrate(q))
    }

    pub fn collide(
        &self,
        collision: &FxElemCollision,
        q: FxElemMotionQuery<'_>,
        mut trace: impl FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> FxElemTraceHit,
    ) -> Option<FxElemMotionResult> {
        let flags = self.velocity.flags;
        let &FxElemCollision {
            reflection_base,
            reflection_amp,
            mins: coll_mins,
            maxs: coll_maxs,
            use_item_clip,
            has_effect_on_impact,
            leaves_start_solid,
        } = collision;
        if !elem_uses_collision(flags) {
            return self.evaluate(q);
        }

        if q.at_rest_fraction != FX_ELEM_AT_REST_NONE {
            return Some(FxElemMotionResult {
                origin_delta: [0.0; 3],
                base_vel: q.base_vel,
                remove: false,
                at_rest_fraction: Some(q.at_rest_fraction),
                supported_since_ms: q.supported_since_ms,
                spawn_impact: None,
            });
        }

        let mask = trace_mask(use_item_clip);
        let mut origin = q.origin;
        let mut base_vel = q.base_vel;
        let mut total_delta = [0.0f32; 3];
        let mut remove = false;
        let mut spawn_impact = None;
        let mut at_rest_fraction = None;

        for step in collide_substep_schedule(q.prev_msec, q.msec_now) {
            let mut t0 = step.msec_start;
            let t_end = step.msec_end;

            for _ in 0..8 {
                if t0 >= t_end || remove || at_rest_fraction.is_some() {
                    break;
                }
                let life_ms = q.life_ms.max(1.0);
                let age0 = ((t0.saturating_sub(q.msec_begin)).max(0) as f32) / life_ms;
                let age1 = ((t_end.saturating_sub(q.msec_begin)).max(0) as f32) / life_ms;
                let dt_sec = (t_end.saturating_sub(t0)).max(0) as f32 * 0.001;
                let sub_q = FxElemMotionQuery {
                    def_name: q.def_name,
                    catalog_index: q.catalog_index,
                    def_index: q.def_index,
                    age0,
                    age1,
                    life_ms,
                    dt_sec,
                    base_vel,
                    elem_random_seed: q.elem_random_seed,
                    motion_random: q.motion_random,
                    origin,
                    prev_msec: t0,
                    msec_now: t_end,
                    msec_begin: q.msec_begin,
                    effect_axis: q.effect_axis,
                    orient: q.orient,
                    at_rest_fraction: q.at_rest_fraction,
                    supported_since_ms: q.supported_since_ms,
                };
                let free = self.integrate(sub_q);
                let start_local = origin;
                let end_local = [
                    start_local[0] + free.origin_delta[0],
                    start_local[1] + free.origin_delta[1],
                    start_local[2] + free.origin_delta[2],
                ];
                let (start, end) = (
                    orientation_pos_to_world(q.orient.origin, q.orient.axis, start_local),
                    orientation_pos_to_world(q.orient.origin, q.orient.axis, end_local),
                );
                let hit = trace(start, end, coll_mins, coll_maxs, mask);
                if !hit.complete {
                    return None;
                }
                if hit.allsolid || (hit.startsolid && !leaves_start_solid) {
                    base_vel = [0.0; 3];
                    break;
                }
                if hit.fraction >= 1.0 {
                    total_delta[0] += free.origin_delta[0];
                    total_delta[1] += free.origin_delta[1];
                    total_delta[2] += free.origin_delta[2];
                    origin = end_local;
                    base_vel = free.base_vel;
                    break;
                }

                let hit_world = [
                    start[0] + (end[0] - start[0]) * hit.fraction,
                    start[1] + (end[1] - start[1]) * hit.fraction,
                    start[2] + (end[2] - start[2]) * hit.fraction,
                ];
                let hit_pos = orientation_pos_from_world(q.orient.origin, q.orient.axis, hit_world);
                total_delta[0] += hit_pos[0] - start_local[0];
                total_delta[1] += hit_pos[1] - start_local[1];
                total_delta[2] += hit_pos[2] - start_local[2];
                origin = hit_pos;

                let age_at_hit_msec = (t0.saturating_sub(q.msec_begin)).max(0) as f32
                    + (t_end.saturating_sub(t0)).max(0) as f32 * hit.fraction;
                let pre_vel = self.velocity.at_time_sampled(
                    base_vel,
                    age_at_hit_msec,
                    life_ms,
                    q.effect_axis,
                    q.motion_random.velocity,
                );
                let pre_speed_sq = vec3_length_sq(pre_vel);
                if has_effect_on_impact && impact_child_speed_allows(pre_speed_sq) {
                    spawn_impact = Some(FxImpactSpawn {
                        origin: hit_world,
                        pre_vel,
                    });
                }

                if elem_dies_on_touch(flags) {
                    remove = true;
                    break;
                }

                let reflection =
                    sample_reflection_factor(reflection_base, reflection_amp, q.elem_random_seed);
                let scaled = [
                    pre_vel[0] * reflection,
                    pre_vel[1] * reflection,
                    pre_vel[2] * reflection,
                ];
                let hit_at_start = hit.fraction <= 0.0;
                if hit_at_start && collide_marks_at_rest(scaled, hit.normal[2]) {
                    let recip = 1.0 / life_ms;
                    let frac = get_at_rest_fraction(t0 as f32, q.msec_begin as f32, recip);
                    let u = if frac < 0.0 {
                        0
                    } else if frac > 255.0 {
                        255
                    } else {
                        frac as u8
                    };
                    at_rest_fraction = Some(u);
                    base_vel = [0.0; 3];
                    break;
                }

                let dvel = collision_reflect_base_vel_delta(pre_vel, hit.normal, reflection);
                base_vel = [
                    base_vel[0] + dvel[0],
                    base_vel[1] + dvel[1],
                    base_vel[2] + dvel[2],
                ];

                let span = (t_end.saturating_sub(t0)).max(0) as f32;
                t0 += (span * hit.fraction) as i32;
                if t0 <= step.msec_start {
                    t0 = step.msec_start.saturating_add(1);
                }
            }
            if remove || at_rest_fraction.is_some() {
                break;
            }
        }

        Some(FxElemMotionResult {
            origin_delta: total_delta,
            base_vel,
            remove,
            at_rest_fraction,
            supported_since_ms: None,
            spawn_impact,
        })
    }
}

const FX_PHYS_GRAVITY: f32 = 800.0;
const FX_PHYS_BOUNCE: f32 = 0.2;
const FX_PHYS_FRICTION: f32 = 0.5;
const FX_PHYS_REST_SPEED: f32 = 20.0;
const FX_PHYS_UNSTICK_MAX_LIFT: f32 = 16.0;
const FX_PHYS_CONTACT_SLOP: f32 = 0.125;

pub fn evaluate_elem_physics_motion(
    flags: i32,
    use_item_clip: bool,
    vel_graph_local: &[fx_iw4::FxElemVec3Range],
    vel_graph_world: &[fx_iw4::FxElemVec3Range],
    q: FxElemMotionQuery<'_>,
    mut trace: impl FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> FxElemTraceHit,
) -> FxElemMotionResult {
    let mut landed = (q.at_rest_fraction != FX_ELEM_AT_REST_NONE && q.base_vel == [0.0; 3])
        .then_some(q.at_rest_fraction);
    let mut supported_since =
        if q.at_rest_fraction != FX_ELEM_AT_REST_NONE && q.base_vel != [0.0; 3] {
            None
        } else {
            q.supported_since_ms
        };
    let mut result = FxElemMotionResult {
        origin_delta: [0.0; 3],
        base_vel: q.base_vel,
        remove: false,
        at_rest_fraction: landed,
        supported_since_ms: q.supported_since_ms,
        spawn_impact: None,
    };
    if landed.is_some() && q.base_vel == [0.0; 3] {
        return result;
    }
    let from = q.prev_msec.max(q.msec_begin);
    if q.msec_now <= from {
        return result;
    }
    let mut vel = if q.prev_msec <= q.msec_begin {
        fx_iw4::FxVelocityField {
            flags,
            local_samples: vel_graph_local,
            world_samples: vel_graph_world,
        }
        .launch_velocity(q.orient.axis, q.motion_random.velocity)
    } else {
        q.base_vel
    };
    let mask = trace_mask(use_item_clip);
    let (mins, maxs) = ([-FX_PHYS_PIECE_HALF; 3], [FX_PHYS_PIECE_HALF; 3]);
    let mut pos = orientation_pos_to_world(q.orient.origin, q.orient.axis, q.origin);
    for step in collide_substep_schedule(from, q.msec_now) {
        let dt = (step.msec_end.saturating_sub(step.msec_start)).max(0) as f32 * 0.001;
        vel[2] -= FX_PHYS_GRAVITY * dt;
        let end = [
            pos[0] + vel[0] * dt,
            pos[1] + vel[1] * dt,
            pos[2] + vel[2] * dt,
        ];
        let hit = trace(pos, end, mins, maxs, mask);
        if !hit.complete {
            result.remove = true;
            return result;
        }
        if hit.allsolid {
            supported_since = None;
            match lift_out_of_solid(pos, mins, maxs, mask, &mut trace) {
                Some(lifted) => {
                    pos = lifted;
                    continue;
                }
                None => {
                    result.remove = true;
                    break;
                }
            }
        }
        if hit.fraction >= 1.0 {
            if supported_since.is_some()
                && vec3_length_sq(vel) < FX_PHYS_REST_SPEED * FX_PHYS_REST_SPEED
            {
                let support = trace(end, [end[0], end[1], end[2] - 0.5], mins, maxs, mask);
                if !support.complete
                    || support.allsolid
                    || support.startsolid
                    || support.fraction >= 1.0
                    || !collide_on_ground(support.normal[2])
                {
                    supported_since = None;
                }
            } else {
                supported_since = None;
            }
            pos = end;
            continue;
        }
        pos = [
            pos[0] + (end[0] - pos[0]) * hit.fraction,
            pos[1] + (end[1] - pos[1]) * hit.fraction,
            pos[2] + (end[2] - pos[2]) * hit.fraction,
        ];
        let n = hit.normal;
        for (position, normal) in pos.iter_mut().zip(n) {
            *position += normal * FX_PHYS_CONTACT_SLOP;
        }
        let into = dot(vel, n);
        if into < 0.0 {
            let push = (1.0 + FX_PHYS_BOUNCE) * into;
            for (v, n) in vel.iter_mut().zip(n) {
                *v -= push * n;
            }
            let along = dot(vel, n);
            for (v, n) in vel.iter_mut().zip(n) {
                let normal_part = along * n;
                *v = normal_part + (*v - normal_part) * (1.0 - FX_PHYS_FRICTION);
            }
        }
        if collide_on_ground(n[2]) && vec3_length_sq(vel) < FX_PHYS_REST_SPEED * FX_PHYS_REST_SPEED
        {
            let since = *supported_since.get_or_insert(step.msec_start);
            if step.msec_end.saturating_sub(since) >= 200 {
                landed = Some(at_rest_byte(step.msec_end, q.msec_begin, q.life_ms));
                vel = [0.0; 3];
                break;
            }
        } else {
            supported_since = None;
        }
    }
    let stored = orientation_pos_from_world(q.orient.origin, q.orient.axis, pos);
    result.origin_delta = [
        stored[0] - q.origin[0],
        stored[1] - q.origin[1],
        stored[2] - q.origin[2],
    ];
    result.base_vel = vel;
    result.at_rest_fraction = landed;
    result.supported_since_ms = supported_since;
    result
}

fn lift_out_of_solid(
    pos: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    trace: &mut impl FnMut([f32; 3], [f32; 3], [f32; 3], [f32; 3], u32) -> FxElemTraceHit,
) -> Option<[f32; 3]> {
    let mut dz = -mins[2] + FX_PHYS_CONTACT_SLOP;
    let cap = dz.max(FX_PHYS_UNSTICK_MAX_LIFT);
    while dz <= cap {
        let lifted = [pos[0], pos[1], pos[2] + dz];
        let probe = trace(lifted, lifted, mins, maxs, mask);
        if probe.complete && !probe.startsolid && !probe.allsolid {
            return Some(lifted);
        }
        dz *= 2.0;
    }
    None
}

fn at_rest_byte(msec: i32, msec_begin: i32, life_ms: f32) -> u8 {
    let frac = get_at_rest_fraction(msec as f32, msec_begin as f32, 1.0 / life_ms.max(1.0));
    frac.clamp(0.0, 254.0) as u8
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

impl FxElemMotion<'_> {
    fn integrate(&self, q: FxElemMotionQuery<'_>) -> FxElemMotionResult {
        let &Self {
            gravity_base,
            gravity_amp,
            ..
        } = self;
        let fx_iw4::FxVelocityField {
            flags,
            local_samples,
            world_samples,
        } = self.velocity;
        let mut stored = q.origin;
        if elem_uses_vel_local(flags) && local_samples.len() >= 2 {
            let d = integrate_velocity_graph_sampled(
                local_samples,
                q.age0,
                q.age1,
                q.life_ms,
                q.motion_random.velocity,
            );
            stored[0] += d[0];
            stored[1] += d[1];
            stored[2] += d[2];
        }
        let mut world = orientation_pos_to_world(q.orient.origin, q.orient.axis, stored);
        if elem_uses_vel_world(flags) && world_samples.len() >= 2 {
            let d = integrate_velocity_graph_sampled(
                world_samples,
                q.age0,
                q.age1,
                q.life_ms,
                q.motion_random.velocity,
            );
            world[0] += d[0];
            world[1] += d[1];
            world[2] += d[2];
        }
        let mut base_vel = q.base_vel;
        if elem_update_has_velocity_graph(flags) {
            {
                let dt = q.dt_sec;
                let g = elem_gravity_accel_z(fx_iw4::sample_float_range(
                    gravity_base,
                    gravity_amp,
                    q.motion_random.gravity,
                ));
                world[0] += base_vel[0] * dt;
                world[1] += base_vel[1] * dt;
                world[2] += base_vel[2] * dt;
                world[2] -= g * dt * dt * 0.5;
                base_vel[2] -= g * dt;
            }
            stored = orientation_pos_from_world(q.orient.origin, q.orient.axis, world);
        }
        FxElemMotionResult {
            origin_delta: [
                stored[0] - q.origin[0],
                stored[1] - q.origin[1],
                stored[2] - q.origin[2],
            ],
            base_vel,
            remove: false,
            at_rest_fraction: None,
            supported_since_ms: None,
            spawn_impact: None,
        }
    }
}
