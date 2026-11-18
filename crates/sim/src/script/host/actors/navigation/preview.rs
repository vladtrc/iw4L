//! A conservative pursuit target on the retained route, independent of animation assets.
use glam::{Vec2, Vec3};

// Preview half a second of travel, with at least one body diameter to start moving.
const PREVIEW_SECONDS: f32 = 0.5;
const BODY_DIAMETER: f32 = 30.0;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Preview {
    pub direction: [f32; 3],
    pub distance: f32,
    pub stairs: bool,
    pub lean: f32,
    pub target: Option<[f32; 3]>,
}

impl Preview {
    pub fn follow(
        &mut self,
        origin: [f32; 3],
        route: &[[f32; 3]],
        velocity: [f32; 3],
        mut supported: impl FnMut([f32; 3]) -> Option<bool>,
    ) {
        let origin = Vec3::from_array(origin);
        let speed = Vec2::new(velocity[0], velocity[1]).length();
        // Bound the collision-query work even for unusually fast actors.
        let horizon = (speed * PREVIEW_SECONDS).clamp(BODY_DIAMETER, BODY_DIAMETER * 10.0);
        let mut remaining = horizon;
        let mut previous = origin;
        self.target = None;
        self.distance = 0.0;
        self.stairs = false;
        'route: for &point in route {
            let end = Vec3::from_array(point);
            let segment = end - previous;
            let length = segment.length();
            if length <= f32::EPSILON {
                continue;
            }
            let travel = length.min(remaining);
            // Sampling at half a body diameter also checks support between nodes.
            let samples = (travel / (BODY_DIAMETER * 0.5)).ceil().max(1.0) as usize;
            for sample in 1..=samples {
                let target =
                    previous + segment * (travel / length * sample as f32 / samples as f32);
                let Some(stairs) = supported(target.to_array()) else {
                    break 'route;
                };
                self.target = Some(target.to_array());
                self.stairs |= stairs;
            }
            remaining -= travel;
            if remaining <= f32::EPSILON {
                break;
            }
            previous = end;
        }
        let mut desired_lean = 0.0;
        if let Some(target) = self.target {
            let delta = Vec3::from_array(target) - origin;
            self.distance = delta.truncate().length();
            if self.distance > f32::EPSILON {
                // GSC uses XY unit direction and Z as the route gradient.
                self.direction = (delta / self.distance).to_array();
                let motion = Vec2::new(velocity[0], velocity[1]).normalize_or_zero();
                let right = Vec2::new(motion.y, -motion.x);
                // Right turns lean right. At rest there is no inertial lean.
                desired_lean = right.dot(delta.truncate() / self.distance)
                    * (speed * PREVIEW_SECONDS / BODY_DIAMETER).min(1.0);
            }
        }
        // A quarter-second response at the simulation's 50 ms cadence.
        const TICK_SECONDS: f32 = 0.05;
        const LEAN_RESPONSE_SECONDS: f32 = 0.25;
        self.lean += (desired_lean - self.lean) * (TICK_SECONDS / LEAN_RESPONSE_SECONDS);
    }
}

/// Test a direct pursuit corridor and its support, including the shortcut between route nodes.
pub(super) fn supported(
    origin: [f32; 3],
    target: [f32; 3],
    mut mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    mut trace: impl FnMut(movement_iw4::GroundTraceInput) -> trace_iw4::Trace,
) -> Option<bool> {
    // Leave room for a single step while sweeping the rest of the body. The
    // movement solver still resolves the actual step and final placement.
    const STEP_HEIGHT: f32 = 18.0;
    const SUPPORT_SPACING: f32 = BODY_DIAMETER * 0.5; // half an actor diameter
    const SURFACE_STAIRS: u32 = 0x200;
    mins[2] += STEP_HEIGHT;
    let sweep = trace(movement_iw4::GroundTraceInput {
        start: origin,
        end: target,
        mins,
        maxs,
        tracemask: mask,
    });
    if sweep.startsolid != 0 || sweep.fraction < 1.0 {
        return None;
    }
    let distance = glam::Vec3::from_array(origin).distance(glam::Vec3::from_array(target));
    let samples = (distance / SUPPORT_SPACING).ceil().max(1.0) as usize;
    let mut stairs = false;
    for sample in 1..=samples {
        let fraction = sample as f32 / samples as f32;
        let point: [f32; 3] =
            std::array::from_fn(|i| origin[i] + (target[i] - origin[i]) * fraction);
        let floor = trace(movement_iw4::GroundTraceInput {
            start: [point[0], point[1], point[2] + STEP_HEIGHT],
            end: [point[0], point[1], point[2] - STEP_HEIGHT * 2.0],
            mins: [0.0; 3],
            maxs: [0.0; 3],
            tracemask: mask,
        });
        if floor.startsolid != 0
            || floor.fraction == 1.0
            || floor.normal[2] < std::f32::consts::FRAC_1_SQRT_2
        {
            return None;
        }
        stairs |= floor.surface_flags & SURFACE_STAIRS != 0;
    }
    Some(stairs)
}
