use crate::portal::portal_eye_dist;

pub fn portal_admits_eye(plane: [f32; 4], vertices: &[[f32; 3]], eye: [f32; 3]) -> bool {
    let dist = portal_eye_dist(plane, eye);
    if dist > 0.0 {
        return false;
    }
    if dist <= -0.125 {
        return true;
    }
    if vertices.len() < 3 {
        return false;
    }
    let (x, y) = vec3_projection_coords([plane[0], plane[1], plane[2]]);
    projected_winding_contains_coplanar_point(vertices, x, y, eye)
}

pub fn vec3_projection_coords(dir: [f32; 3]) -> (usize, usize) {
    let sq = [dir[0] * dir[0], dir[1] * dir[1], dir[2] * dir[2]];
    if sq[0] > sq[2] || sq[1] > sq[2] {
        if sq[0] > sq[1] || sq[2] > sq[1] {
            if dir[0] <= 0.0 { (2, 1) } else { (1, 2) }
        } else if dir[1] <= 0.0 {
            (0, 2)
        } else {
            (2, 0)
        }
    } else if dir[2] <= 0.0 {
        (1, 0)
    } else {
        (0, 1)
    }
}

pub fn projected_winding_contains_coplanar_point(
    verts: &[[f32; 3]],
    x: usize,
    y: usize,
    point: [f32; 3],
) -> bool {
    if verts.len() < 3 || x > 2 || y > 2 {
        return false;
    }
    let mut prev = verts.len() - 1;
    for cur in 0..verts.len() {
        let edge_x = verts[cur][y] - verts[prev][y];
        let edge_y = verts[prev][x] - verts[cur][x];
        let delta_x = point[x] - verts[prev][x];
        let delta_y = point[y] - verts[prev][y];
        if edge_y * delta_y + edge_x * delta_x < 0.0 {
            return false;
        }
        prev = cur;
    }
    true
}
