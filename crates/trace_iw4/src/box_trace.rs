use crate::Trace;

#[derive(Clone, Copy, Debug)]
pub struct BrushRef<'a> {
    pub planes: &'a [[f32; 4]],
    pub contents: u32,
    pub plane_surface_flags: &'a [u32],

    pub glass_encoded: u16,
}

impl BrushRef<'_> {
    fn flags_for_plane(self, index: usize) -> u32 {
        self.plane_surface_flags.get(index).copied().unwrap_or(0)
    }
}

const SURFACE_CLIP_EPSILON: f32 = 0.125;

const WALKABLE_NORMAL_Z: f32 = 0.7;

#[derive(Clone, Copy, Debug)]
struct CapsuleSize {
    offset: [f32; 3],

    radius: f32,

    offset_z: f32,
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
        }
    }

    fn plane_radius(self, n: [f32; 3]) -> f32 {
        self.radius + n[2].abs() * self.offset_z
    }
}

pub fn trace_box<'a, I>(
    brushes: I,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
) -> Trace
where
    I: IntoIterator<Item = BrushRef<'a>>,
{
    trace_capsule(brushes, start, end, mins, maxs, mask)
}

pub fn trace_capsule<'a, I>(
    brushes: I,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
) -> Trace
where
    I: IntoIterator<Item = BrushRef<'a>>,
{
    trace_capsule_hit(brushes, start, end, mins, maxs, mask).0
}

pub fn trace_capsule_hit<'a, I>(
    brushes: I,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
) -> (Trace, u16)
where
    I: IntoIterator<Item = BrushRef<'a>>,
{
    let mut best = Trace {
        fraction: 1.0,
        endpos: end,
        ..Trace::default()
    };
    let glass_encoded = trace_box_into(brushes, start, end, mins, maxs, mask, &mut best);
    (best, glass_encoded)
}

pub fn trace_box_into<'a, I>(
    brushes: I,
    start: [f32; 3],
    end: [f32; 3],
    mins: [f32; 3],
    maxs: [f32; 3],
    mask: u32,
    best: &mut Trace,
) -> u16
where
    I: IntoIterator<Item = BrushRef<'a>>,
{
    let cap = CapsuleSize::from_bounds(mins, maxs);
    let start = [
        start[0] + cap.offset[0],
        start[1] + cap.offset[1],
        start[2] + cap.offset[2],
    ];
    let end = [
        end[0] + cap.offset[0],
        end[1] + cap.offset[1],
        end[2] + cap.offset[2],
    ];
    let mut glass_encoded = 0u16;

    for brush in brushes {
        if brush.contents & mask == 0 || brush.planes.is_empty() {
            continue;
        }
        trace_through_brush(&brush, start, end, cap, best, &mut glass_encoded);
        if best.allsolid != 0 {
            break;
        }
    }

    if best.fraction < 1.0 {
        best.endpos = [
            start[0] + (end[0] - start[0]) * best.fraction - cap.offset[0],
            start[1] + (end[1] - start[1]) * best.fraction - cap.offset[1],
            start[2] + (end[2] - start[2]) * best.fraction - cap.offset[2],
        ];

        if best.walkable == 0 && best.startsolid == 0 {
            best.walkable = u8::from(best.normal[2] >= WALKABLE_NORMAL_Z);
        }
    } else {
        best.endpos = [
            end[0] - cap.offset[0],
            end[1] - cap.offset[1],
            end[2] - cap.offset[2],
        ];
    }
    glass_encoded
}

fn trace_through_brush(
    brush: &BrushRef<'_>,
    start: [f32; 3],
    end: [f32; 3],
    cap: CapsuleSize,
    trace: &mut Trace,
    glass_encoded: &mut u16,
) {
    let mut enter_frac = 0.0_f32;
    let mut leave_frac = trace.fraction;
    let mut allsolid = true;
    let mut lead_normal: Option<[f32; 3]> = None;

    let mut lead_plane: Option<usize> = None;

    for (plane_i, plane) in brush.planes.iter().enumerate() {
        let n = [plane[0], plane[1], plane[2]];
        let dist = plane[3] + cap.plane_radius(n);
        let d1 = n[0] * start[0] + n[1] * start[1] + n[2] * start[2] - dist;
        let d2 = n[0] * end[0] + n[1] * end[1] + n[2] * end[2] - dist;

        if d1 <= 0.0 {
            if d2 > 0.0 {
                let delta = d1 - d2;
                if d1 > leave_frac * delta {
                    leave_frac = d1 / delta;
                    if leave_frac <= enter_frac {
                        return;
                    }
                }
                allsolid = false;
            }
            continue;
        }

        let front = if SURFACE_CLIP_EPSILON - d1 < 0.0 {
            SURFACE_CLIP_EPSILON
        } else {
            d1
        };
        if front <= d2 {
            return;
        }
        if d2 > 0.0 {
            allsolid = false;
        }
        let delta = d1 - d2;
        let f = d1 - SURFACE_CLIP_EPSILON;
        if f <= enter_frac * delta {
            if lead_normal.is_none() {
                lead_normal = Some(n);
                lead_plane = Some(plane_i);
            }
        } else {
            enter_frac = f / delta;
            if leave_frac <= enter_frac {
                return;
            }
            lead_normal = Some(n);
            lead_plane = Some(plane_i);
        }
    }

    trace.contents = brush.contents;
    if let Some(normal) = lead_normal {
        trace.fraction = enter_frac;
        trace.normal = normal;

        trace.surface_flags = lead_plane.map(|i| brush.flags_for_plane(i)).unwrap_or(0);

        let (hit_type, hit_id) = crate::cm_brush_sweep_hit_kind(brush.glass_encoded);
        trace.hit_type = hit_type;
        trace.hit_id = hit_id;
        trace.walkable = 0;
        *glass_encoded = brush.glass_encoded;
    } else {
        trace.startsolid = 1;
        trace.hit_type = crate::HITTYPE_ENTITY;
        trace.hit_id = crate::ENTITYNUM_WORLD;
        if allsolid {
            trace.allsolid = 1;
            trace.fraction = 0.0;
            trace.surface_flags = 0;
        }
    }
}
