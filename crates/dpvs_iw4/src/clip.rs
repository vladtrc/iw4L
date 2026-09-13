use crate::aabb::MAX_CLIP_PLANES;

pub const PORTAL_CLIP_PLANE_EPS: f32 = 0.001;

pub const PORTAL_CLIP_SIDE_LIMIT: usize = 10;

pub const PORTAL_PROJECT_W_MIN: f32 = 0.125;

pub type GfxMatrix = [[f32; 4]; 4];

#[derive(Clone, Copy, Debug)]
pub struct PortalBevels {
    pub view_proj: GfxMatrix,
    pub inv_view_proj: GfxMatrix,
}

#[must_use]
pub fn gfx_matrix_from_d3d_row_major(m: &[f32; 16]) -> GfxMatrix {
    let mut g = [[0.0f32; 4]; 4];
    for row in 0..4 {
        for col in 0..4 {
            g[col][row] = m[row * 4 + col];
        }
    }
    g
}

#[must_use]
pub fn portal_bevels_from_d3d_row_major(
    view_proj: &[f32; 16],
    inv_view_proj: &[f32; 16],
) -> PortalBevels {
    PortalBevels {
        view_proj: gfx_matrix_from_d3d_row_major(view_proj),
        inv_view_proj: gfx_matrix_from_d3d_row_major(inv_view_proj),
    }
}

fn vec3_normalize(v: [f32; 3]) -> [f32; 3] {
    let len_sq = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if len_sq == 0.0 {
        return [0.0; 3];
    }
    let inv = 1.0 / libm::sqrtf(len_sq);
    [v[0] * inv, v[1] * inv, v[2] * inv]
}

pub fn side_plane_normals(
    winding: &[[f32; 3]],
    eye: [f32; 3],
    view_org_is_dir: bool,
    out: &mut [[f32; 3]],
) -> usize {
    let n = winding.len().min(out.len());
    if n == 0 {
        return 0;
    }
    if view_org_is_dir {
        for i in 0..n {
            let prev = if i == 0 { n - 1 } else { i - 1 };
            let edge = [
                winding[i][0] - winding[prev][0],
                winding[i][1] - winding[prev][1],
                winding[i][2] - winding[prev][2],
            ];
            out[i] = vec3_normalize([
                edge[2] * eye[1] - edge[1] * eye[2],
                edge[0] * eye[2] - eye[0] * edge[2],
                eye[0] * edge[1] - edge[0] * eye[1],
            ]);
        }
    } else {
        for i in 0..n {
            let j = if i + 1 == n { 0 } else { i + 1 };
            let a = [
                winding[j][0] - eye[0],
                winding[j][1] - eye[1],
                winding[j][2] - eye[2],
            ];
            let b = [
                winding[i][0] - eye[0],
                winding[i][1] - eye[1],
                winding[i][2] - eye[2],
            ];
            out[i] = vec3_normalize([
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]);
        }
    }
    n
}

#[must_use]
pub fn unproject_clip_xy(inv_view_proj: &GfxMatrix, px: f32, py: f32) -> [f32; 3] {
    let x = inv_view_proj[0][0] * px + inv_view_proj[1][0] * py + inv_view_proj[3][0];
    let y = inv_view_proj[0][1] * px + inv_view_proj[1][1] * py + inv_view_proj[3][1];
    let z = inv_view_proj[0][2] * px + inv_view_proj[1][2] * py + inv_view_proj[3][2];
    let w = inv_view_proj[0][3] * px + inv_view_proj[1][3] * py + inv_view_proj[3][3];
    if w == 0.0 {
        return [0.0, 0.0, 0.0];
    }
    let inv_w = 1.0 / w;
    [x * inv_w, y * inv_w, z * inv_w]
}

#[must_use]
pub fn clip_space_winding_aabb(
    view_proj: &GfxMatrix,
    winding: &[[f32; 3]],
) -> ([f32; 2], [f32; 2]) {
    let mut mins = [1.0f32, 1.0];
    let mut maxs = [-1.0f32, -1.0];
    for p in winding {
        let w = view_proj[0][3] * p[0]
            + view_proj[1][3] * p[1]
            + view_proj[2][3] * p[2]
            + view_proj[3][3];
        if w < PORTAL_PROJECT_W_MIN {
            return ([-1.0, -1.0], [1.0, 1.0]);
        }
        let inv_w = 1.0 / w;
        let x = (view_proj[0][0] * p[0]
            + view_proj[1][0] * p[1]
            + view_proj[2][0] * p[2]
            + view_proj[3][0])
            * inv_w;
        let y = (view_proj[0][1] * p[0]
            + view_proj[1][1] * p[1]
            + view_proj[2][1] * p[2]
            + view_proj[3][1])
            * inv_w;
        mins[0] = mins[0].min(x);
        mins[1] = mins[1].min(y);
        maxs[0] = maxs[0].max(x);
        maxs[1] = maxs[1].max(y);
    }
    (mins, maxs)
}

pub fn add_bevel_planes(
    dest: &mut [[f32; 4]],
    mins: [f32; 2],
    maxs: [f32; 2],
    eye: [f32; 3],
    inv_view_proj: &GfxMatrix,
) -> usize {
    let mut verts = [[0.0f32; 3]; 4];
    let corners = [
        [mins[0], maxs[1]],
        [mins[0], mins[1]],
        [maxs[0], mins[1]],
        [maxs[0], maxs[1]],
    ];
    for (i, xy) in corners.iter().enumerate() {
        verts[i] = unproject_clip_xy(inv_view_proj, xy[0], xy[1]);
    }
    let mut normals = [[0.0f32; 3]; 4];
    side_plane_normals(&verts, eye, false, &mut normals);
    let mut written = 0usize;
    for i in 0..4 {
        if written >= dest.len() {
            break;
        }
        let n = normals[i];
        dest[written] = [
            n[0],
            n[1],
            n[2],
            PORTAL_CLIP_PLANE_EPS - (n[0] * verts[i][0] + n[1] * verts[i][1] + n[2] * verts[i][2]),
        ];
        written += 1;
    }
    written
}

fn plane_point_dist(plane: [f32; 4], p: [f32; 3]) -> f32 {
    plane[0] * p[0] + plane[1] * p[1] + plane[2] * p[2] + plane[3]
}

#[must_use]
pub fn nearest_point_on_winding(plane: [f32; 4], points: &[[f32; 3]]) -> f32 {
    let n = points.len();
    if n == 0 {
        return 0.0;
    }
    let d_first = plane_point_dist(plane, points[0]);
    let d_last = plane_point_dist(plane, points[n - 1]);
    if d_last <= d_first {
        let mut dist_min = d_last;
        let mut i = n.saturating_sub(2);
        while i > 0 {
            let d = plane_point_dist(plane, points[i]);
            if d > dist_min {
                break;
            }
            dist_min = d;
            i -= 1;
        }
        dist_min
    } else {
        let mut dist_min = d_first;
        for p in points.iter().take(n.saturating_sub(1)).skip(1) {
            let d = plane_point_dist(plane, *p);
            if d > dist_min {
                break;
            }
            dist_min = d;
        }
        dist_min
    }
}

pub fn portal_clip_planes(
    winding: &[[f32; 3]],
    eye: [f32; 3],
    view_org_is_dir: bool,
    near: Option<[f32; 4]>,
    far: Option<[f32; 4]>,
    out: &mut [[f32; 4]],
    bevels: Option<&PortalBevels>,
) -> usize {
    let mut written = 0usize;
    let nvert = winding.len();
    if nvert >= 11 {
        if let Some(b) = bevels {
            let (mins, maxs) = clip_space_winding_aabb(&b.view_proj, winding);
            written = add_bevel_planes(out, mins, maxs, eye, &b.inv_view_proj).min(out.len());
        }
    } else {
        let mut normals = [[0.0f32; 3]; MAX_CLIP_PLANES];
        let nside = side_plane_normals(winding, eye, view_org_is_dir, &mut normals);
        for i in 0..nside {
            if written >= out.len() {
                break;
            }
            let n = normals[i];
            if n[0] * n[0] + n[1] * n[1] + n[2] * n[2] == 0.0 {
                continue;
            }
            let v = winding[i];
            out[written] = [
                n[0],
                n[1],
                n[2],
                PORTAL_CLIP_PLANE_EPS - (n[0] * v[0] + n[1] * v[1] + n[2] * v[2]),
            ];
            written += 1;
        }
    }
    if let Some(near) = near {
        if written < out.len() {
            let mut plane = near;
            let dist = nearest_point_on_winding(plane, winding);
            if dist > 0.0 {
                plane[3] -= dist;
            }
            out[written] = plane;
            written += 1;
        }
    }
    if let Some(far) = far {
        if written < out.len() {
            out[written] = far;
            written += 1;
        }
    }
    written
}

pub fn portal_clip_planes_no_frustum(
    winding: &[[f32; 3]],
    eye: [f32; 3],
    view_org_is_dir: bool,
    out: &mut [[f32; 4]],
) -> usize {
    portal_clip_planes(winding, eye, view_org_is_dir, None, None, out, None)
}

#[must_use]
pub fn portal_hull_origin(plane: [f32; 4]) -> [f32; 3] {
    [
        -plane[3] * plane[0],
        -plane[3] * plane[1],
        -plane[3] * plane[2],
    ]
}

#[must_use]
pub fn portal_hull_point(origin: [f32; 3], hull_axis: [[f32; 3]; 2], u: f32, v: f32) -> [f32; 3] {
    [
        origin[0] + u * hull_axis[0][0] + v * hull_axis[1][0],
        origin[1] + u * hull_axis[0][1] + v * hull_axis[1][1],
        origin[2] + u * hull_axis[0][2] + v * hull_axis[1][2],
    ]
}

#[must_use]
pub fn portal_vert_hull_uv(vert: [f32; 3], hull_axis: [[f32; 3]; 2]) -> [f32; 2] {
    [
        vert[0] * hull_axis[0][0] + vert[1] * hull_axis[0][1] + vert[2] * hull_axis[0][2],
        vert[0] * hull_axis[1][0] + vert[1] * hull_axis[1][1] + vert[2] * hull_axis[1][2],
    ]
}

pub fn rebuild_portal_hull_winding(
    verts: &[[f32; 3]],
    plane: [f32; 4],
    hull_axis: [[f32; 3]; 2],
    out: &mut [[f32; 3]],
) -> usize {
    let n = verts.len();
    if n < 3 || n > crate::convex_hull::COM_CONVEX_HULL_MAX || out.is_empty() {
        return 0;
    }
    let mut uv = [[0.0f32; 2]; crate::convex_hull::COM_CONVEX_HULL_MAX];
    for i in 0..n {
        uv[i] = portal_vert_hull_uv(verts[i], hull_axis);
    }
    let mut hull2 = [[0.0f32; 2]; crate::convex_hull::COM_CONVEX_HULL_MAX];
    let h = crate::com_convex_hull(&uv[..n], &mut hull2);
    if h == 0 {
        return 0;
    }
    let origin = portal_hull_origin(plane);
    let w = h.min(out.len());
    for i in 0..w {
        out[i] = portal_hull_point(origin, hull_axis, hull2[i][0], hull2[i][1]);
    }
    w
}
