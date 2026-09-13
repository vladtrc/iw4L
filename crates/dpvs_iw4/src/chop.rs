const EQUAL_EPSILON: f32 = 0.001;

#[inline]
fn plane_dot(plane: [f32; 4], v: [f32; 3]) -> f32 {
    plane[0] * v[0] + plane[1] * v[1] + plane[2] * v[2] + plane[3]
}

pub fn chop_portal_winding(verts_in: &[[f32; 3]], plane: [f32; 4], out: &mut [[f32; 3]]) -> usize {
    let n = verts_in.len();
    if !(3..=128).contains(&n) || out.len() < 128 {
        return 0;
    }
    let mut dist = [0.0f32; 131];
    let mut side = [2u8; 136];
    let mut front = 0i32;
    let mut back = 0i32;
    for i in 0..n {
        let d = plane_dot(plane, verts_in[i]) - EQUAL_EPSILON;
        dist[i] = d;
        side[i] = 2;
        if d >= -EQUAL_EPSILON {
            if d > EQUAL_EPSILON {
                side[i] = 0;
                front += 1;
            }
        } else {
            side[i] = 1;
            back += 1;
        }
    }
    if front == 0 {
        return 0;
    }
    if back == 0 {
        let copy = n.min(out.len());
        out[..copy].copy_from_slice(&verts_in[..copy]);
        return copy;
    }
    side[n] = side[0];
    dist[n] = dist[0];
    let mut new_count = 0usize;
    for i in 0..n {
        if new_count >= 128 {
            break;
        }
        if side[i] == 2 {
            out[new_count] = verts_in[i];
            new_count += 1;
        } else {
            if side[i] == 0 {
                out[new_count] = verts_in[i];
                new_count += 1;
                if new_count >= 128 {
                    break;
                }
            }
            let next = if i + 1 < n { i + 1 } else { 0 };
            if side[next] != 2 && side[next] != side[i] {
                let denom = dist[i] - dist[next];
                let t = if denom.abs() < f32::EPSILON {
                    0.0
                } else {
                    dist[i] / denom
                };
                let a = verts_in[i];
                let b = verts_in[next];
                out[new_count] = [
                    a[0] + (b[0] - a[0]) * t,
                    a[1] + (b[1] - a[1]) * t,
                    a[2] + (b[2] - a[2]) * t,
                ];
                new_count += 1;
            }
        }
    }
    if new_count < 3 { 0 } else { new_count }
}

pub fn chop_portal(
    verts: &[[f32; 3]],
    parent: Option<[f32; 4]>,
    clip_planes: &[[f32; 4]],
    buf_a: &mut [[f32; 3]; 128],
    buf_b: &mut [[f32; 3]; 128],
) -> usize {
    let mut count = verts.len().min(128);
    if count < 3 {
        return 0;
    }
    buf_a[..count].copy_from_slice(&verts[..count]);
    let mut src_is_a = true;

    let mut apply = |plane: [f32; 4]| -> bool {
        let n = if src_is_a {
            let input = *buf_a;
            chop_portal_winding(&input[..count], plane, buf_b)
        } else {
            let input = *buf_b;
            chop_portal_winding(&input[..count], plane, buf_a)
        };
        if n < 3 {
            count = 0;
            return false;
        }
        count = n;
        src_is_a = !src_is_a;
        true
    };

    if let Some(p) = parent
        && !apply(p)
    {
        return 0;
    }
    for &plane in clip_planes {
        if !apply(plane) {
            return 0;
        }
    }

    if !src_is_a {
        buf_a[..count].copy_from_slice(&buf_b[..count]);
    }
    count
}
