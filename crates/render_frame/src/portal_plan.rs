//! The frame's aperture geometry, selection and fallback decisions. Culling,
//! submission, masks and diagnostics consume this plan rather than re-budgeting.
use crate::portal::{PortalOpening, PortalView};
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalRenderSettings {
    /// None: portals. usize::MAX: physical camera only. Other values: debug view.
    pub only: Option<usize>,
    pub max_views: usize,
    pub depth: u8,
    pub tint: bool,
}
impl Default for PortalRenderSettings {
    fn default() -> Self {
        Self {
            only: None,
            max_views: 6,
            depth: 3,
            tint: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalProjection {
    /// Homogeneous clip coordinates, before the scene depth-range mapping.
    pub corners: [[f32; 4]; 8],
    pub plane: [f32; 4],
    pub rect: [u32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortalFallback {
    Blocked,
    Occluded,
    Budget,
    Depth,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedAperture {
    pub opening: usize,
    pub parent: usize,
    pub child: usize,
    pub depth: u8,
    pub projection: PortalProjection,
    pub fallback: Option<PortalFallback>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PortalPlan {
    pub root: usize,
    pub settings: PortalRenderSettings,
    pub selected: Vec<bool>,
    pub apertures: Vec<PlannedAperture>,
    pub culled_dpvs: u32,
    pub culled_query: u32,
}

pub fn intersect_rect(a: [u32; 4], b: [u32; 4]) -> Option<[u32; 4]> {
    let x = a[0].max(b[0]);
    let y = a[1].max(b[1]);
    let r = (u64::from(a[0]) + u64::from(a[2])).min(u64::from(b[0]) + u64::from(b[2]));
    let bottom = (u64::from(a[1]) + u64::from(a[3])).min(u64::from(b[1]) + u64::from(b[3]));
    (r > u64::from(x) && bottom > u64::from(y)).then(|| {
        [
            x,
            y,
            (r - u64::from(x)) as u32,
            (bottom - u64::from(y)) as u32,
        ]
    })
}

pub fn project_aperture(
    o: &PortalOpening,
    clip: Mat4,
    eye: [f32; 3],
    viewport: [f32; 4],
) -> Option<PortalProjection> {
    let [vx, vy, vw, vh] = viewport;
    if viewport.iter().any(|v| !v.is_finite())
        || !(vx + vw).is_finite()
        || !(vy + vh).is_finite()
        || vw <= 0.0
        || vh <= 0.0
    {
        return None;
    }
    let eye = Vec3::from_array(eye);
    let base = Vec3::from_array(o.base);
    let across = Vec3::from_array(o.across) * o.width * 0.5;
    let up = Vec3::from_array(o.up) * o.height;
    let normal = Vec3::from_array(o.across)
        .cross(Vec3::from_array(o.up))
        .normalize_or_zero();
    let determinant = clip.determinant();
    if !clip.is_finite() || !determinant.is_finite() || determinant == 0.0 {
        return None;
    }
    let inverse = clip.inverse();
    let near = inverse * Vec4::new(1.0, 1.0, 1.0, 1.0);
    let distance = (near.truncate() / near.w - eye).length() + 0.01;
    let delta = eye - base;
    let inside = Vec3::from_array(o.across).dot(delta).abs() <= o.width * 0.5
        && Vec3::from_array(o.up).dot(delta) >= 0.0
        && Vec3::from_array(o.up).dot(delta) <= o.height;
    let distance = if inside && normal.dot(delta).abs() < distance {
        distance
    } else {
        0.0
    };
    let away = normal
        * if normal.dot(eye - base) >= 0.0 {
            -distance
        } else {
            distance
        };
    let front = [
        base - across,
        base + across,
        base + across + up,
        base - across + up,
    ];
    let corners = [
        front[0],
        front[1],
        front[2],
        front[3],
        front[0] + away,
        front[1] + away,
        front[2] + away,
        front[3] + away,
    ]
    .map(|p| clip * p.extend(1.0));
    if corners.iter().any(|p| !p.is_finite()) || corners.iter().all(|p| p.w <= 0.0) {
        return None;
    }
    let mut lo = [vx + vw, vy + vh];
    let mut hi = [vx, vy];
    for p in corners {
        if p.w <= 0.0 {
            lo = [vx, vy];
            hi = [vx + vw, vy + vh];
            break;
        }
        let x = vx + (p.x / p.w * 0.5 + 0.5) * vw;
        let y = vy + (-p.y / p.w * 0.5 + 0.5) * vh;
        lo[0] = lo[0].min(x);
        lo[1] = lo[1].min(y);
        hi[0] = hi[0].max(x);
        hi[1] = hi[1].max(y);
    }
    let x0 = lo[0].floor().clamp(vx, vx + vw) as u32;
    let y0 = lo[1].floor().clamp(vy, vy + vh) as u32;
    let x1 = hi[0].ceil().clamp(vx, vx + vw) as u32;
    let y1 = hi[1].ceil().clamp(vy, vy + vh) as u32;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let plane = inverse.transpose() * normal.extend(-normal.dot(base));
    if !plane.is_finite() {
        return None;
    }
    Some(PortalProjection {
        corners: corners.map(|p| p.to_array()),
        plane: plane.to_array(),
        rect: [x0, y0, x1 - x0, y1 - y0],
    })
}

impl PortalPlan {
    /// PortalView supplies complete projections, already clipped to all ancestors.
    pub(crate) fn new(
        view: &PortalView,
        projections: &[PortalProjection],
        mut settings: PortalRenderSettings,
        eye: [f32; 3],
        visible: &mut dyn FnMut(&PortalView, usize) -> bool,
    ) -> Self {
        settings.max_views = settings.max_views.clamp(1, 8);
        settings.depth = settings.depth.min(3);
        let root = settings
            .only
            .filter(|&i| i != usize::MAX && view.moves.get(i).is_some_and(Option::is_some))
            .unwrap_or(view.camera_world);
        let mut selected = vec![false; view.moves.len()];
        selected[root] = true;
        let mut out = Self {
            root,
            settings,
            selected,
            apertures: Vec::new(),
            culled_dpvs: 0,
            culled_query: 0,
        };
        if settings.only.is_some() {
            return out;
        }
        let mut frontier = std::collections::BinaryHeap::<(
            std::cmp::Reverse<u32>,
            u64,
            std::cmp::Reverse<usize>,
            u8,
        )>::new();
        let mut add = |parent: usize,
                       depth: u8,
                       frontier: &mut std::collections::BinaryHeap<_>,
                       culled: &mut u32| {
            for (index, opening) in view
                .openings
                .iter()
                .enumerate()
                .filter(|(_, o)| o.worlds[0] == parent)
            {
                if !visible(view, index) {
                    *culled += 1;
                    continue;
                }
                let projection = projections[index];
                let delta = Vec3::from_array(eye) - Vec3::from_array(opening.base);
                let across = Vec3::from_array(opening.across);
                let up = Vec3::from_array(opening.up);
                let nearest = across
                    * delta
                        .dot(across)
                        .clamp(-opening.width * 0.5, opening.width * 0.5)
                    + up * delta.dot(up).clamp(0.0, opening.height);
                let distance = (delta - nearest).length_squared();
                frontier.push((
                    std::cmp::Reverse(distance.to_bits()),
                    u64::from(projection.rect[2]) * u64::from(projection.rect[3]),
                    std::cmp::Reverse(index),
                    depth + 1,
                ));
            }
        };
        add(root, 0, &mut frontier, &mut out.culled_dpvs);
        let mut used = 1;
        while let Some((std::cmp::Reverse(distance), _, std::cmp::Reverse(index), depth)) =
            frontier.pop()
        {
            let o = view.openings[index];
            let [parent, child] = o.worlds;
            let projection = projections[index];
            let fallback = if view.opening_blocked.get(index).copied().unwrap_or(false) {
                Some(PortalFallback::Blocked)
            } else if view
                .occlusion
                .as_ref()
                .is_some_and(|q| q.hidden(&view.query_keys[index]))
                && f32::from_bits(distance) > 64.0 * 64.0
            {
                out.culled_query += 1;
                Some(PortalFallback::Occluded)
            } else if depth > settings.depth {
                Some(PortalFallback::Depth)
            } else if used >= settings.max_views {
                Some(PortalFallback::Budget)
            } else {
                None
            };
            if fallback.is_none() {
                used += 1;
                out.selected[child] = true;
                add(child, depth, &mut frontier, &mut out.culled_dpvs);
            }
            out.apertures.push(PlannedAperture {
                opening: index,
                parent,
                child,
                depth,
                projection,
                fallback,
            });
        }
        out
    }
}
