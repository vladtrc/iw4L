//! The caster bound of each sun cascade.
//!
//! A cascade's orthographic matrix says where casters land in the atlas, not
//! which ones are worth drawing: its depth range spans the whole map, so using
//! its six frustum planes as the collection volume admits the entire world
//! swept along the sun. The real bound is narrower and built here — the arc the
//! camera frustum sweeps as seen from the sun, the cascade's own usable
//! rectangle, and, for the far cascade, the seam where the near one ends.

use bevy::math::{Vec2, Vec3};
use render_frame::SunShadowClipPlanes;

/// Below this, a cross product between neighbouring frustum rays is treated as
/// zero and the edge counts as neither turning left nor right.
const ARC_CLASSIFICATION_EPSILON: f32 = 0.001;

/// A boundary edge shorter than this fraction of the map half-extent has been
/// clipped away to nothing by the planes already in the set, so its plane adds
/// nothing.
const BOUNDARY_LENGTH_FRACTION: f32 = 0.02;

/// How far the culling rectangles are pulled in from the cascade rectangles
/// before the seam is fitted between them.
const CULLING_RECTANGLE_GUARD_FRACTION: f32 = 0.0625;

/// Slack when deciding whether two ray intervals along the cascade rectangles
/// touch.
const RECTANGLE_CONNECTION_EPSILON: f32 = 0.01;

/// Sample-size ratio between the near and far cascades.
const FAR_PARTITION_SAMPLE_SCALE: f32 = 4.0;

/// The four corner rays of the camera frustum seen from the sun, and how they
/// turn around it.
#[derive(Clone, Copy, Debug)]
pub struct SunShadowFrustumRays {
    pub world_rays: [[f32; 3]; 4],

    /// Each world ray projected onto the sun basis.
    pub shadow_rays: [[f32; 2]; 4],

    /// Cross product of each projected ray with the next one round.
    pub sines: [f32; 4],
    pub sin_min: f32,
    pub sin_max: f32,
    pub mins: [f32; 2],
    pub maxs: [f32; 2],

    /// The rays where the swept arc starts and ends, or `None` when the frustum
    /// does not straddle the sun at all.
    pub bounding_arc: Option<(usize, usize)>,
}

/// Projects the camera's corner rays into the sun basis and classifies the arc
/// they sweep.
///
/// `packed_axes` is `[axis0, axis1, light_direction]`.
#[must_use]
pub fn sun_shadow_frustum_rays(
    world_rays: [[f32; 3]; 4],
    packed_axes: [[f32; 3]; 3],
) -> SunShadowFrustumRays {
    let axis0 = Vec3::from_array(packed_axes[0]);
    let axis1 = Vec3::from_array(packed_axes[1]);
    let mut shadow_rays = [[0.0f32; 2]; 4];
    let mut mins = [0.0f32; 2];
    let mut maxs = [0.0f32; 2];
    for (index, ray) in world_rays.iter().enumerate() {
        let ray = Vec3::from_array(*ray);
        let projected = [ray.dot(axis0), ray.dot(axis1)];
        shadow_rays[index] = projected;
        for axis in 0..2 {
            mins[axis] = mins[axis].min(projected[axis]);
            maxs[axis] = maxs[axis].max(projected[axis]);
        }
    }

    let mut sines = [0.0f32; 4];
    for edge in 0..4 {
        let current = Vec2::from_array(shadow_rays[edge]);
        let next = Vec2::from_array(shadow_rays[(edge + 1) & 3]);
        sines[edge] = current.x * next.y - current.y * next.x;
    }
    let sin_min = sines.iter().copied().fold(f32::INFINITY, f32::min);
    let sin_max = sines.iter().copied().fold(f32::NEG_INFINITY, f32::max);

    let mut arc0 = None;
    let mut arc1 = None;
    if (-sin_min).min(sin_max) >= ARC_CLASSIFICATION_EPSILON {
        let mut previous = 3usize;
        for edge in 0..4 {
            let sine = sines[edge];
            let previous_sine = sines[previous];
            if sine > ARC_CLASSIFICATION_EPSILON && previous_sine <= ARC_CLASSIFICATION_EPSILON {
                arc0 = Some(edge);
            } else if sine < -ARC_CLASSIFICATION_EPSILON
                && previous_sine >= -ARC_CLASSIFICATION_EPSILON
            {
                arc1 = Some(edge);
            }
            previous = edge;
        }
    }
    // One side without the other is not an arc; the frustum is treated as not
    // straddling the sun rather than being clipped by half a boundary.
    let bounding_arc = arc0.zip(arc1);

    SunShadowFrustumRays {
        world_rays,
        shadow_rays,
        sines,
        sin_min,
        sin_max,
        mins,
        maxs,
        bounding_arc,
    }
}

/// Everything the plane set needs from the cascade projection.
#[derive(Clone, Copy, Debug)]
pub struct SunShadowClipInput {
    /// `[axis0, axis1, light_direction]`.
    pub packed_axes: [[f32; 3]; 3],
    pub camera_origin: [f32; 3],
    pub camera_forward: [f32; 3],

    /// Distance from the eye to the near clip plane.
    pub camera_near_distance: f32,

    /// Where the near cascade stops resolving, along the camera's forward axis.
    pub near_shadow_min_distance: f32,

    /// The eye in the sun basis, before snapping.
    pub shadow_origin: [f32; 2],

    /// Where the eye sits inside the cascade rectangle, in texels.
    pub shadow_origin_pixel_center: [f32; 2],
    pub snapped_shadow_origin: [[f32; 2]; 2],
    pub sample_size: [f32; 2],

    /// Width of a cascade rectangle in texels.
    pub useful_size: u32,
}

impl SunShadowClipInput {
    fn axis(&self, index: usize) -> Vec3 {
        Vec3::from_array(self.packed_axes[index])
    }

    fn rectangle(&self, partition: usize) -> [f32; 4] {
        let sample = self.sample_size[partition];
        let snapped = self.snapped_shadow_origin[partition];
        let min_x = snapped[0] - sample * self.shadow_origin_pixel_center[0];
        let min_y = snapped[1] - sample * self.shadow_origin_pixel_center[1];
        let span = self.useful_size as f32 * sample;
        [min_x, min_y, min_x + span, min_y + span]
    }
}

/// Builds both cascades' caster bounds as world-space planes, each holding for
/// the points inside it.
#[must_use]
pub fn sun_shadow_clip_planes(
    input: &SunShadowClipInput,
    rays: &SunShadowFrustumRays,
) -> [SunShadowClipPlanes; 2] {
    let mut planes = [SunShadowClipPlanes::EMPTY; 2];
    add_initial_planes(&mut planes, input, rays);
    add_map_boundary_planes(&mut planes[0], input, rays, 0);
    add_far_near_plane(&mut planes[1], input, rays);
    add_map_boundary_planes(&mut planes[1], input, rays, 1);
    planes
}

/// The sides of the arc the camera frustum sweeps around the sun, or — when it
/// sweeps none because it points away — a pair of planes at the near clip and
/// at the end of the near cascade.
fn add_initial_planes(
    planes: &mut [SunShadowClipPlanes; 2],
    input: &SunShadowClipInput,
    rays: &SunShadowFrustumRays,
) {
    let Some((arc0, arc1)) = rays.bounding_arc else {
        if rays.sin_max < -ARC_CLASSIFICATION_EPSILON {
            let normal = Vec3::from_array(input.camera_forward);
            let base = -normal.dot(Vec3::from_array(input.camera_origin));
            push(&mut planes[0], normal, base - input.camera_near_distance);
            push(
                &mut planes[1],
                normal,
                base - input.near_shadow_min_distance,
            );
        }
        return;
    };
    let Some(first) = normalize2(rays.shadow_rays[arc0]) else {
        return;
    };
    let Some(second) = normalize2(rays.shadow_rays[arc1]) else {
        return;
    };
    add_bounding_arc_plane(planes, input, [-first.y, first.x]);
    add_bounding_arc_plane(planes, input, [second.y, -second.x]);
}

/// One side of the arc, padded outwards by half a texel so a caster straddling
/// the boundary still reaches the cascade it shadows.
fn add_bounding_arc_plane(
    planes: &mut [SunShadowClipPlanes; 2],
    input: &SunShadowClipInput,
    shadow_normal: [f32; 2],
) {
    let world_normal = input.axis(0) * shadow_normal[0] + input.axis(1) * shadow_normal[1];
    let base = -world_normal.dot(Vec3::from_array(input.camera_origin));
    let padding = 0.5 * input.sample_size[0] * (shadow_normal[0].abs() + shadow_normal[1].abs());
    push(&mut planes[0], world_normal, base + padding);
    push(
        &mut planes[1],
        world_normal,
        base + FAR_PARTITION_SAMPLE_SCALE * padding,
    );
}

/// The four sides of the cascade's own rectangle.
///
/// Only the sides that actually cut the volume are kept: a side whose segment
/// is already clipped away by the arc planes would only cost a plane slot.
fn add_map_boundary_planes(
    planes: &mut SunShadowClipPlanes,
    input: &SunShadowClipInput,
    rays: &SunShadowFrustumRays,
    partition: usize,
) {
    let sample = input.sample_size[partition];
    let useful_world_size = input.useful_size as f32 * sample;
    let half_extent = useful_world_size * 0.5;
    let snapped = input.snapped_shadow_origin[partition];

    let mut candidates = [[0.0f32; 4]; 4];
    // Start with the axis across the wider side of the frustum footprint.
    let axis1_first = rays.maxs[1] - rays.mins[1] < rays.maxs[0] - rays.mins[0];
    for iteration in 0..2 {
        let axis_index = usize::from(axis1_first) ^ iteration;
        let normal = input.axis(axis_index);
        let boundary = input.shadow_origin_pixel_center[axis_index] * sample - snapped[axis_index];
        let positive = iteration * 2 + usize::from(boundary >= half_extent);
        candidates[positive] = plane(normal, boundary);
        candidates[positive ^ 1] = plane(-normal, useful_world_size - boundary);
    }

    let light = input.axis(2);
    let first_planar = planes
        .as_slice()
        .iter()
        .position(|p| light.dot(normal_of(*p)).abs() < ARC_CLASSIFICATION_EPSILON);

    let Some(first_planar) = first_planar else {
        let first_pair = usize::from(candidates[0][3] >= candidates[2][3]) * 2;
        let second_pair = first_pair ^ 2;
        for index in [first_pair, first_pair + 1, second_pair, second_pair + 1] {
            planes.push(candidates[index]);
        }
        return;
    };

    let map_center = Vec2::new(
        half_extent - sample * input.shadow_origin_pixel_center[0] + snapped[0],
        half_extent - sample * input.shadow_origin_pixel_center[1] + snapped[1],
    );
    let minimum_length = half_extent * BOUNDARY_LENGTH_FRACTION;
    let existing: Vec<[f32; 4]> = planes.as_slice().to_vec();
    for index in 0..4 {
        let candidate = candidates[index];
        let perpendicular = candidates[index ^ 2];
        let centre = input.axis(0) * map_center.x + input.axis(1) * map_center.y
            - normal_of(candidate) * half_extent;
        let extent = normal_of(perpendicular) * half_extent;
        let mut start = centre - extent;
        let mut end = centre + extent;
        if !clip_segment(&mut start, &mut end, &existing[first_planar..]) {
            continue;
        }
        if start.distance_squared(end) >= minimum_length * minimum_length {
            planes.push(candidate);
        }
    }
}

/// Trims a segment to the half-spaces of `planes`, reporting whether anything
/// survived.
fn clip_segment(start: &mut Vec3, end: &mut Vec3, planes: &[[f32; 4]]) -> bool {
    for plane in planes {
        let start_distance = evaluate(*plane, *start);
        let end_distance = evaluate(*plane, *end);
        if start_distance <= 0.0 {
            if end_distance <= 0.0 {
                return false;
            }
            let denominator = end_distance - start_distance;
            *start = (*start * end_distance - *end * start_distance) / denominator;
        } else if end_distance <= 0.0 {
            let denominator = start_distance - end_distance;
            *end = (*end * start_distance - *start * end_distance) / denominator;
        }
    }
    true
}

/// The seam: one plane cutting the far cascade off where the near one takes
/// over, laid along the two arc rays where they leave the near rectangle.
fn add_far_near_plane(
    planes: &mut SunShadowClipPlanes,
    input: &SunShadowClipInput,
    rays: &SunShadowFrustumRays,
) {
    let Some((arc0, arc1)) = rays.bounding_arc else {
        return;
    };
    let (Some(ray0), Some(ray1)) = (
        normalize2(rays.shadow_rays[arc0]),
        normalize2(rays.shadow_rays[arc1]),
    ) else {
        return;
    };
    let rectangles = culling_rectangles(input);
    let origin = Vec2::from_array(input.shadow_origin);
    let (Some(range0), Some(range1)) = (
        ray_range_inside_rectangles(origin, ray0, &rectangles),
        ray_range_inside_rectangles(origin, ray1, &rectangles),
    ) else {
        return;
    };

    let mut point0 = origin + ray0 * range0.x;
    let mut point1 = origin + ray1 * range1.x;
    if rectangles.len() != 1 && !segment_inside_rectangles(point0, point1, &rectangles) {
        let alternate0 = origin + ray0 * range0.y;
        let alternate1 = origin + ray1 * range1.y;
        if range0.x <= range0.y {
            if range1.x <= range1.y || !segment_inside_rectangles(point0, alternate1, &rectangles) {
                return;
            }
            point1 = alternate1;
        } else if range1.x <= range1.y {
            if !segment_inside_rectangles(alternate0, point1, &rectangles) {
                return;
            }
            point0 = alternate0;
        } else {
            let second_first = range0.y * range1.x < range1.y * range0.x;
            let try_pair = |a: Vec2, b: Vec2| segment_inside_rectangles(a, b, &rectangles);
            if if second_first {
                try_pair(point0, alternate1)
            } else {
                try_pair(alternate0, point1)
            } {
                if second_first {
                    point1 = alternate1;
                } else {
                    point0 = alternate0;
                }
            } else if if second_first {
                try_pair(alternate0, point1)
            } else {
                try_pair(point0, alternate1)
            } {
                if second_first {
                    point0 = alternate0;
                } else {
                    point1 = alternate1;
                }
            } else if try_pair(alternate0, alternate1) {
                point0 = alternate0;
                point1 = alternate1;
            } else {
                return;
            }
        }
    }

    let Some(edge_normal) = normalize2([point1.y - point0.y, point0.x - point1.x]) else {
        return;
    };
    let world_normal = input.axis(0) * edge_normal.x + input.axis(1) * edge_normal.y;
    let coefficient = 0.5 * input.sample_size[1] * (edge_normal.x.abs() + edge_normal.y.abs())
        - edge_normal.dot(point0);
    push(planes, world_normal, coefficient);
}

/// The near rectangle, plus whatever strips of it the far rectangle does not
/// already reach, each pulled in by a guard band.
fn culling_rectangles(input: &SunShadowClipInput) -> Vec<[f32; 4]> {
    let near_outer = input.rectangle(0);
    let far_outer = input.rectangle(1);
    let useful = input.useful_size as f32;
    let near_inner = inset(
        near_outer,
        useful * input.sample_size[0] * CULLING_RECTANGLE_GUARD_FRACTION,
    );
    let far_inner = inset(
        far_outer,
        useful * input.sample_size[1] * CULLING_RECTANGLE_GUARD_FRACTION,
    );
    let mut rectangles = Vec::with_capacity(3);
    rectangles.push(near_inner);

    if far_inner[0] > near_outer[0] {
        rectangles.push([near_outer[0], near_outer[1], far_inner[0], near_outer[3]]);
    } else if far_inner[2] < near_outer[2] {
        rectangles.push([far_inner[2], near_outer[1], near_outer[2], near_outer[3]]);
    }

    if far_inner[1] > near_outer[1] {
        rectangles.push([near_outer[0], near_outer[1], near_outer[2], far_inner[1]]);
    } else if far_inner[3] < near_outer[3] {
        rectangles.push([near_outer[0], far_inner[3], near_outer[2], near_outer[3]]);
    }
    rectangles
}

fn inset(rectangle: [f32; 4], amount: f32) -> [f32; 4] {
    [
        rectangle[0] + amount,
        rectangle[1] + amount,
        rectangle[2] - amount,
        rectangle[3] - amount,
    ]
}

/// How far a ray from the eye stays inside the connected run of rectangles:
/// `x` is where it leaves that run, `y` where it leaves the first rectangle.
fn ray_range_inside_rectangles(
    origin: Vec2,
    direction: Vec2,
    rectangles: &[[f32; 4]],
) -> Option<Vec2> {
    let mut intervals = Vec::with_capacity(rectangles.len());
    let mut first_exit = f32::MAX * 0.5;
    for (index, rectangle) in rectangles.iter().enumerate() {
        let interval = ray_rectangle_interval(origin, direction, *rectangle);
        if interval.x < interval.y && interval.y >= RECTANGLE_CONNECTION_EPSILON {
            if index == 0 {
                first_exit = interval.y;
            }
            intervals.push(interval);
        }
    }
    let connected_end = connected_end(&mut intervals)?;
    Some(Vec2::new(connected_end, first_exit.min(connected_end)))
}

fn segment_inside_rectangles(start: Vec2, end: Vec2, rectangles: &[[f32; 4]]) -> bool {
    let delta = end - start;
    let length = delta.length();
    if length <= 0.0 || !length.is_finite() {
        return false;
    }
    let direction = delta / length;
    let mut intervals = Vec::with_capacity(rectangles.len());
    for rectangle in rectangles {
        let interval = ray_rectangle_interval(start, direction, *rectangle);
        if interval.x < interval.y && interval.y >= RECTANGLE_CONNECTION_EPSILON {
            intervals.push(interval);
        }
    }
    let Some(connected_end) = connected_end(&mut intervals) else {
        return false;
    };
    length - RECTANGLE_CONNECTION_EPSILON <= connected_end
}

/// Merges the intervals that touch, starting from the one that contains the
/// origin; `None` when nothing does.
fn connected_end(intervals: &mut [Vec2]) -> Option<f32> {
    intervals.sort_by(|left, right| left.x.total_cmp(&right.x));
    let first = intervals.first()?;
    if first.x > RECTANGLE_CONNECTION_EPSILON {
        return None;
    }
    let mut end = first.y;
    for interval in intervals.iter().skip(1) {
        if interval.x >= end + RECTANGLE_CONNECTION_EPSILON {
            break;
        }
        end = end.max(interval.y);
    }
    Some(end)
}

fn ray_rectangle_interval(origin: Vec2, direction: Vec2, rectangle: [f32; 4]) -> Vec2 {
    if direction.x.abs() < ARC_CLASSIFICATION_EPSILON {
        return if direction.y < 0.0 {
            Vec2::new(origin.y - rectangle[3], origin.y - rectangle[1])
        } else {
            Vec2::new(rectangle[1] - origin.y, rectangle[3] - origin.y)
        };
    }
    if direction.y.abs() < ARC_CLASSIFICATION_EPSILON {
        return if direction.x < 0.0 {
            Vec2::new(origin.x - rectangle[2], origin.x - rectangle[0])
        } else {
            Vec2::new(rectangle[0] - origin.x, rectangle[2] - origin.x)
        };
    }
    let near_x = (if direction.x < 0.0 {
        rectangle[2]
    } else {
        rectangle[0]
    } - origin.x)
        / direction.x;
    let near_y = (if direction.y < 0.0 {
        rectangle[3]
    } else {
        rectangle[1]
    } - origin.y)
        / direction.y;
    let far_x = (if direction.x < 0.0 {
        rectangle[0]
    } else {
        rectangle[2]
    } - origin.x)
        / direction.x;
    let far_y = (if direction.y < 0.0 {
        rectangle[1]
    } else {
        rectangle[3]
    } - origin.y)
        / direction.y;
    Vec2::new(near_x.max(near_y), far_x.min(far_y))
}

fn plane(normal: Vec3, coefficient: f32) -> [f32; 4] {
    [normal.x, normal.y, normal.z, coefficient]
}

fn normal_of(plane: [f32; 4]) -> Vec3 {
    Vec3::new(plane[0], plane[1], plane[2])
}

fn evaluate(plane: [f32; 4], point: Vec3) -> f32 {
    normal_of(plane).dot(point) + plane[3]
}

fn normalize2(value: [f32; 2]) -> Option<Vec2> {
    let value = Vec2::from_array(value);
    let length = value.length();
    (length > 0.0 && length.is_finite()).then(|| value / length)
}

fn push(planes: &mut SunShadowClipPlanes, normal: Vec3, coefficient: f32) {
    let candidate = plane(normal, coefficient);
    if candidate.iter().all(|c| c.is_finite()) {
        planes.push(candidate);
    }
}
