use crate::scene_light::{GFX_LIGHT_TYPE_SPOT, GfxLightPack, R_DLIGHT_SCENE_CAP};

pub const R_DLIGHT_LIMIT_DEFAULT: u32 = 4;

pub const R_DLIGHT_LIMIT_MAX: u32 = 4;

pub const R_DLIGHT_BACKEND_MAX: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneDlight {
    pub light: GfxLightPack,

    pub used: bool,
}

#[must_use]
pub fn dlight_visible(slot: &SceneDlight) -> bool {
    !slot.used
}

#[must_use]
pub fn dlight_partition_prefers(lhs: &GfxLightPack, rhs: &GfxLightPack, view: [f32; 3]) -> bool {
    if lhs.light_type != rhs.light_type {
        return lhs.light_type == GFX_LIGHT_TYPE_SPOT;
    }
    let d_lhs = dist2(view, lhs.origin) * rhs.radius * rhs.radius;
    let d_rhs = dist2(view, rhs.origin) * lhs.radius * lhs.radius;
    d_lhs < d_rhs
}

fn dist2(view: [f32; 3], origin: [f32; 3]) -> f32 {
    let dx = view[0] - origin[0];
    let dy = view[1] - origin[1];
    let dz = view[2] - origin[2];
    dx * dx + dy * dy + dz * dz
}

#[must_use]
pub fn dlight_hits_aabb(origin: [f32; 3], radius: f32, mid: [f32; 3], half: [f32; 3]) -> bool {
    if radius <= 0.0 {
        return false;
    }
    let mut sum = 0.0f32;
    for i in 0..3 {
        let mut axis = (origin[i] - mid[i]).abs() - half[i];
        if axis < 0.0 {
            axis = 0.0;
        }
        sum += axis * axis;
    }
    sum <= radius * radius
}

pub fn dlight_select_visible(
    slots: &[SceneDlight],
    view: [f32; 3],
    limit: u32,
    dest: &mut [usize],
) -> usize {
    let mut ids = [0usize; R_DLIGHT_SCENE_CAP as usize];
    let mut n = 0usize;
    for (i, slot) in slots.iter().enumerate() {
        if n == ids.len() {
            break;
        }
        if dlight_visible(slot) {
            ids[n] = i;
            n += 1;
        }
    }
    let cap = core::cmp::min(limit, R_DLIGHT_LIMIT_MAX) as usize;
    if n > cap {
        ids[..n].sort_by(|&a, &b| {
            let la = &slots[a].light;
            let lb = &slots[b].light;
            match (
                dlight_partition_prefers(la, lb, view),
                dlight_partition_prefers(lb, la, view),
            ) {
                (true, false) => core::cmp::Ordering::Less,
                (false, true) => core::cmp::Ordering::Greater,
                _ => a.cmp(&b),
            }
        });
        n = cap;
    }
    let out = core::cmp::min(n, dest.len());
    dest[..out].copy_from_slice(&ids[..out]);
    out
}

#[must_use]
pub fn dlight_copies_to_backend(light: &GfxLightPack) -> bool {
    light.light_type != GFX_LIGHT_TYPE_SPOT
}

#[must_use]
pub fn cull_point_and_radius_from_planes(
    planes: &[[f32; 4]],
    origin: [f32; 3],
    radius: f32,
) -> bool {
    let neg_r = -radius;
    planes.iter().any(|plane| {
        plane[0] * origin[0] + plane[1] * origin[1] + plane[2] * origin[2] + plane[3] < neg_r
    })
}

#[must_use]
pub fn spot_dlight0_frustum_culls(
    slots: &[SceneDlight],
    planes: Option<&[[f32; 4]]>,
) -> Option<bool> {
    let Some(first) = slots.first() else {
        return Some(true);
    };
    if first.light.light_type != GFX_LIGHT_TYPE_SPOT {
        return Some(true);
    }
    let planes = planes.filter(|p| !p.is_empty())?;
    Some(cull_point_and_radius_from_planes(
        planes,
        first.light.origin,
        first.light.radius,
    ))
}

#[must_use]
pub fn spot_dlight0_special_copy_allows(
    limit: u32,
    sm3: bool,
    slots: &[SceneDlight],
    frustum_culls: Option<bool>,
) -> bool {
    if limit == 0 || !sm3 {
        return false;
    }
    let Some(first) = slots.first() else {
        return false;
    };
    if first.light.light_type != GFX_LIGHT_TYPE_SPOT {
        return false;
    }
    matches!(frustum_culls, Some(false))
}

pub fn append_scene_dlights_to_backend(
    slots: &[SceneDlight],
    view: [f32; 3],
    limit: u32,
    sm3: bool,
    frustum_culls_dlight0: Option<bool>,
    dest: &mut [GfxLightPack],
) -> usize {
    let mut w = 0usize;
    if spot_dlight0_special_copy_allows(limit, sm3, slots, frustum_culls_dlight0) && w < dest.len()
    {
        dest[w] = slots[0].light;
        w += 1;
    }
    let mut ids = [0usize; R_DLIGHT_LIMIT_MAX as usize];
    let n = dlight_select_visible(slots, view, limit, &mut ids);
    for &i in &ids[..n] {
        let light = slots[i].light;
        if dlight_copies_to_backend(&light) && w < dest.len() {
            dest[w] = light;
            w += 1;
        }
    }
    w
}
