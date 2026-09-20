use crate::scene_light::GFX_LIGHT_TYPE_SPOT;

pub const COM_PRIMARY_LIGHT_COS_HALF_FOV_EXPANDED: usize = 0x34;

pub const COM_PRIMARY_LIGHT_ORIGIN: usize = 0x1c;

pub const COM_PRIMARY_LIGHT_DIR: usize = 0x10;

pub const COM_PRIMARY_LIGHT_RADIUS: usize = 0x28;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ComPrimaryLightCull {
    pub light_type: u8,
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub radius: f32,

    pub cos_half_fov_expanded: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightRegionAxis {
    pub dir: [f32; 3],
    pub mid: f32,
    pub half: f32,
}

pub fn lighting_query_box_half(dobj_bounds_radius: f32) -> [f32; 3] {
    [dobj_bounds_radius, dobj_bounds_radius, dobj_bounds_radius]
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightRegionHull<'a> {
    pub kdop_mid: [f32; 9],
    pub kdop_half: [f32; 9],
    pub axes: &'a [LightRegionAxis],
}

pub type LightRegionHulls<'a> = &'a [LightRegionHull<'a>];

pub fn cull_box_from_sphere(
    origin: [f32; 3],
    radius: f32,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    let mut d2 = 0.0_f32;
    for i in 0..3 {
        let mut a = (origin[i] - box_mid[i]).abs() - box_half[i];
        if a < 0.0 {
            a = 0.0;
        }
        d2 += a * a;
    }
    let r2 = radius * radius;
    d2 > r2
}

fn dir_sign(v: f32) -> f32 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

pub fn cull_box_from_cone(
    cone_org: [f32; 3],
    cone_dir: [f32; 3],
    cos_half_fov: f32,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    let delta = [
        box_mid[0] - cone_org[0],
        box_mid[1] - cone_org[1],
        box_mid[2] - cone_org[2],
    ];
    let sx = dir_sign(cone_dir[0]);
    let sy = dir_sign(cone_dir[1]);
    let sz = dir_sign(cone_dir[2]);
    let dist_mid = delta[0] * cone_dir[0] + delta[1] * cone_dir[1] + delta[2] * cone_dir[2];
    let support = cone_dir[0] * sx * box_half[0]
        + cone_dir[1] * sy * box_half[1]
        + cone_dir[2] * sz * box_half[2];
    if support <= dist_mid {
        return true;
    }
    let perp = [
        -dist_mid * cone_dir[0] + delta[0],
        -dist_mid * cone_dir[1] + delta[1],
        -dist_mid * cone_dir[2] + delta[2],
    ];
    let perp_len_sq = perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2];
    let cos_sq = cos_half_fov * cos_half_fov;
    let sin_sq = 1.0 - cos_sq;
    let a = perp_len_sq * cos_sq;
    let b = sin_sq * dist_mid * dist_mid;
    if a <= b {
        return false;
    }
    let hypot = libm::sqrtf(perp_len_sq * sin_sq);
    if hypot == 0.0 {
        return true;
    }
    let scale = cos_half_fov / hypot;
    let axis = [
        cone_dir[0] + scale * perp[0],
        cone_dir[1] + scale * perp[1],
        cone_dir[2] + scale * perp[2],
    ];
    let mut sep = axis[0] * delta[0] + axis[1] * delta[1] + axis[2] * delta[2];
    sep -= (axis[0] * box_half[0]).abs();
    sep -= (axis[1] * box_half[1]).abs();
    sep -= (axis[2] * box_half[2]).abs();
    !(sep < 0.0)
}

pub fn cull_box_from_conic_section_of_sphere(
    cone_org: [f32; 3],
    cone_dir: [f32; 3],
    cos_half_fov: f32,
    radius: f32,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    if cull_box_from_sphere(cone_org, radius, box_mid, box_half) {
        return true;
    }
    cull_box_from_cone(cone_org, cone_dir, cos_half_fov, box_mid, box_half)
}

pub fn cull_box_from_primary_light(
    light: &ComPrimaryLightCull,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    let expanded = light.cos_half_fov_expanded;
    let use_sphere = light.light_type != GFX_LIGHT_TYPE_SPOT || expanded.is_nan() || expanded < 0.0;
    if use_sphere {
        cull_box_from_sphere(light.origin, light.radius, box_mid, box_half)
    } else {
        cull_box_from_conic_section_of_sphere(
            light.origin,
            light.direction,
            expanded,
            light.radius,
            box_mid,
            box_half,
        )
    }
}

pub fn cull_box_from_light_region_hull(
    hull: &LightRegionHull<'_>,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    for i in 0..3 {
        let d = (box_mid[i] - hull.kdop_mid[i]).abs();
        if d >= box_half[i] + hull.kdop_half[i] {
            return true;
        }
    }

    let h_xy = box_half[0] + box_half[1];
    let d3 = (box_mid[0] + box_mid[1] - hull.kdop_mid[3]).abs();
    if d3 >= h_xy + hull.kdop_half[3] {
        return true;
    }
    let d4 = (box_mid[0] - box_mid[1] - hull.kdop_mid[4]).abs();
    if d4 >= h_xy + hull.kdop_half[4] {
        return true;
    }

    let h_xz = box_half[0] + box_half[2];
    let d5 = (box_mid[0] + box_mid[2] - hull.kdop_mid[5]).abs();
    if d5 >= h_xz + hull.kdop_half[5] {
        return true;
    }
    let d6 = (box_mid[0] - box_mid[2] - hull.kdop_mid[6]).abs();
    if d6 >= h_xz + hull.kdop_half[6] {
        return true;
    }

    let h_yz = box_half[1] + box_half[2];
    let d7 = (box_mid[1] + box_mid[2] - hull.kdop_mid[7]).abs();
    if d7 >= h_yz + hull.kdop_half[7] {
        return true;
    }
    let d8 = (box_mid[1] - box_mid[2] - hull.kdop_mid[8]).abs();
    if d8 >= h_yz + hull.kdop_half[8] {
        return true;
    }
    for axis in hull.axes {
        let support = box_half[0] * axis.dir[0].abs()
            + box_half[1] * axis.dir[1].abs()
            + box_half[2] * axis.dir[2].abs();
        let mid = box_mid[0] * axis.dir[0] + box_mid[1] * axis.dir[1] + box_mid[2] * axis.dir[2];
        if (mid - axis.mid).abs() >= support + axis.half {
            return true;
        }
    }
    false
}

pub fn cull_point_from_cone_expanded(
    origin: [f32; 3],
    dir: [f32; 3],
    cos_half_fov_expanded: f32,
    point: [f32; 3],
    extra: f32,
) -> bool {
    let delta = [
        point[0] - origin[0],
        point[1] - origin[1],
        point[2] - origin[2],
    ];
    let along = delta[0] * dir[0] + delta[1] * dir[1] + delta[2] * dir[2];
    if extra <= along {
        return true;
    }
    let perp = [
        -along * dir[0] + delta[0],
        -along * dir[1] + delta[1],
        -along * dir[2] + delta[2],
    ];
    let perp_len_sq = perp[0] * perp[0] + perp[1] * perp[1] + perp[2] * perp[2];
    let perp_len = libm::sqrtf(perp_len_sq);
    let a = perp_len * along - extra;
    let lhs = perp_len_sq * cos_half_fov_expanded * cos_half_fov_expanded;
    lhs >= a * a
}

pub fn cull_point_from_light_region_hull(
    hull: &LightRegionHull<'_>,
    local: [f32; 3],
    extra: f32,
) -> bool {
    for i in 0..3 {
        if (local[i] - hull.kdop_mid[i]).abs() >= extra + hull.kdop_half[i] {
            return true;
        }
    }
    let d3 = (local[0] + local[1] - hull.kdop_mid[3]).abs();
    if d3 >= extra + hull.kdop_half[3] {
        return true;
    }
    let d4 = (local[0] - local[1] - hull.kdop_mid[4]).abs();
    if d4 >= extra + hull.kdop_half[4] {
        return true;
    }
    let d5 = (local[0] + local[2] - hull.kdop_mid[5]).abs();
    if d5 >= extra + hull.kdop_half[5] {
        return true;
    }
    let d6 = (local[0] - local[2] - hull.kdop_mid[6]).abs();
    if d6 >= extra + hull.kdop_half[6] {
        return true;
    }
    let d7 = (local[1] + local[2] - hull.kdop_mid[7]).abs();
    if d7 >= extra + hull.kdop_half[7] {
        return true;
    }
    let d8 = (local[1] - local[2] - hull.kdop_mid[8]).abs();
    if d8 >= extra + hull.kdop_half[8] {
        return true;
    }
    for axis in hull.axes {
        let mid = local[0] * axis.dir[0] + local[1] * axis.dir[1] + local[2] * axis.dir[2];
        if (mid - axis.mid).abs() >= extra + axis.half {
            return true;
        }
    }
    false
}

pub fn light_region_culls_point(
    hulls: &[LightRegionHull<'_>],
    light_origin: [f32; 3],
    point: [f32; 3],
    extra: f32,
) -> bool {
    if hulls.is_empty() {
        return false;
    }
    let local = [
        point[0] - light_origin[0],
        point[1] - light_origin[1],
        point[2] - light_origin[2],
    ];
    for hull in hulls {
        if !cull_point_from_light_region_hull(hull, local, extra) {
            return false;
        }
    }
    true
}

pub fn light_region_culls_box<'a, H: core::borrow::Borrow<LightRegionHull<'a>>>(
    hulls: impl IntoIterator<Item = H>,
    light_origin: [f32; 3],
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    let local = [
        box_mid[0] - light_origin[0],
        box_mid[1] - light_origin[1],
        box_mid[2] - light_origin[2],
    ];
    let mut any = false;
    for hull in hulls {
        any = true;
        if !cull_box_from_light_region_hull(hull.borrow(), local, box_half) {
            return false;
        }
    }
    any
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NonSunPrimaryWalkTrace {
    pub walk: u32,
    pub first_volume_hit: u32,
    pub first_region_hit: u32,
}

pub fn non_sun_primary_light_walk_trace(
    lights: &[ComPrimaryLightCull],
    sun_primary_index: u32,
    regions: Option<&[LightRegionHulls<'_>]>,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> NonSunPrimaryWalkTrace {
    let Some(regions) = regions else {
        return NonSunPrimaryWalkTrace::default();
    };
    let start = sun_primary_index.saturating_add(1) as usize;
    let n = lights.len().min(regions.len());
    let mut first_volume_hit = 0u32;
    let mut first_region_hit = 0u32;
    let mut walk = 0u32;
    for i in start..n {
        let volume_cull = cull_box_from_primary_light(&lights[i], box_mid, box_half);
        if !volume_cull && first_volume_hit == 0 {
            first_volume_hit = i as u32;
        }
        let region_cull = light_region_culls_box(regions[i], lights[i].origin, box_mid, box_half);
        if !region_cull && first_region_hit == 0 {
            first_region_hit = i as u32;
        }
        if walk == 0 && !volume_cull && !region_cull {
            walk = i as u32;
        }
    }
    NonSunPrimaryWalkTrace {
        walk,
        first_volume_hit,
        first_region_hit,
    }
}

pub const DYN_ENT_PRIMARY_LIGHT_LINK_DIST2_INIT: f32 = f32::MAX;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DynEntPrimaryLightLink {
    pub closest: u8,
}

#[must_use]
pub fn dyn_ent_links_to_primary_light(
    light: &ComPrimaryLightCull,
    region: Option<&[LightRegionHull<'_>]>,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> bool {
    if cull_box_from_primary_light(light, box_mid, box_half) {
        return false;
    }
    let Some(hulls) = region else {
        return false;
    };
    !light_region_culls_box(hulls, light.origin, box_mid, box_half)
}

#[must_use]
pub fn dyn_ent_primary_light_link_dist2(origin: [f32; 3], box_mid: [f32; 3]) -> f32 {
    let dx = box_mid[0] - origin[0];
    let dy = box_mid[1] - origin[1];
    let dz = box_mid[2] - origin[2];
    dx * dx + dy * dy + dz * dz
}

pub fn dyn_ent_primary_light_link(
    lights: &[ComPrimaryLightCull],
    sun_primary: u32,
    regions: Option<&[LightRegionHulls<'_>]>,
    box_mid: [f32; 3],
    box_half: [f32; 3],
    mut visit: impl FnMut(u32, bool),
) -> DynEntPrimaryLightLink {
    let start = sun_primary.saturating_add(1);
    let count = lights.len() as u32;
    let mut closest = 0u8;
    let mut best = DYN_ENT_PRIMARY_LIGHT_LINK_DIST2_INIT;
    let mut light = start;
    while light < count {
        let i = light as usize;
        let region = regions.and_then(|regs| regs.get(i).copied());
        let set = dyn_ent_links_to_primary_light(&lights[i], region, box_mid, box_half);
        visit(light, set);
        if set {
            let d2 = dyn_ent_primary_light_link_dist2(lights[i].origin, box_mid);
            if d2 < best {
                best = d2;
                closest = light as u8;
            }
        }
        light += 1;
    }
    DynEntPrimaryLightLink { closest }
}

pub fn non_sun_primary_light_for_box(
    lights: &[ComPrimaryLightCull],
    sun_primary_index: u32,
    regions: Option<&[LightRegionHulls<'_>]>,
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> u32 {
    non_sun_primary_light_walk_trace(lights, sun_primary_index, regions, box_mid, box_half).walk
}
