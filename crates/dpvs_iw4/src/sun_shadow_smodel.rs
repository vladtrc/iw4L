pub const STATIC_MODEL_FLAG_NO_CAST_SHADOW: u8 = 0x10;

#[derive(Clone, Copy, Debug)]
pub struct GfxStaticModelDrawInstShadow {
    pub flags: u8,

    pub cull_dist: u16,

    pub origin: [f32; 3],
}

#[inline]
fn origin_dist_sq(origin: [f32; 3], lod_origin: [f32; 3]) -> f32 {
    let dx = origin[0] - lod_origin[0];
    let dy = origin[1] - lod_origin[1];
    let dz = origin[2] - lod_origin[2];
    dx * dx + dy * dy + dz * dz
}

#[must_use]
pub fn smodel_cull_dist_hides(cull_dist: u16, dist_sq: f32, lod_scale: Option<f32>) -> bool {
    if cull_dist == 0 {
        return false;
    }
    let cull = f32::from(cull_dist);
    let scaled_sq = match lod_scale {
        Some(s) => dist_sq * s * s,
        None => dist_sq,
    };
    cull * cull <= scaled_sq
}

pub fn cull_smodel_sun_shadow_vis(
    vis: &mut [u8],
    insts: &[GfxStaticModelDrawInstShadow],
    lod_origin: [f32; 3],
    lod_scale: Option<f32>,
) {
    let n = vis.len().min(insts.len());
    for i in 0..n {
        if vis[i] == 0 {
            continue;
        }
        let inst = insts[i];
        let dist_sq = origin_dist_sq(inst.origin, lod_origin);
        if smodel_cull_dist_hides(inst.cull_dist, dist_sq, lod_scale) {
            vis[i] = 0;
        }
    }
}

pub fn add_smodel_range_sun_shadow(
    vis: &[u8],
    insts: &[GfxStaticModelDrawInstShadow],
    out: &mut [u16],
) -> usize {
    let mut n = 0usize;
    let last = vis.len().min(insts.len());
    for i in 0..last {
        if n >= out.len() {
            break;
        }
        if vis.get(i).copied().unwrap_or(0) == 0 {
            continue;
        }
        if insts[i].flags & STATIC_MODEL_FLAG_NO_CAST_SHADOW != 0 {
            continue;
        }
        let Ok(id) = u16::try_from(i) else {
            continue;
        };
        out[n] = id;
        n += 1;
    }
    n
}
