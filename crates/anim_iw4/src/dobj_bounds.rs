pub const DOBJ_RADIUS_PARENT_ROOT: u8 = 0xff;

pub const DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT: usize = 32;

pub fn dobj_compute_bounds_radius(xmodel_radii: &[f32], parents: &[u8]) -> f32 {
    let n = xmodel_radii
        .len()
        .min(parents.len())
        .min(DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT);
    if n == 0 {
        return 0.0;
    }
    let mut acc = [0.0f32; DOBJ_COMPUTE_BOUNDS_MODEL_LIMIT];
    let mut max = 0.0f32;
    for i in 0..n {
        let mut r = xmodel_radii[i];
        let parent = parents[i];
        if parent != DOBJ_RADIUS_PARENT_ROOT {
            let pi = parent as usize;
            if pi < i {
                r += acc[pi];
            }
        }
        acc[i] = r;
        if max < r {
            max = r;
        }
    }
    max
}
