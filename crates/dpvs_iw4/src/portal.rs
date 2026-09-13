#[inline]
pub fn portal_eye_dist(plane: [f32; 4], eye: [f32; 3]) -> f32 {
    plane[0] * eye[0] + plane[1] * eye[1] + plane[2] * eye[2] + plane[3]
}

pub fn portal_behind_plane(plane: [f32; 4], vertices: &[[f32; 3]]) -> bool {
    if vertices.is_empty() {
        return true;
    }
    vertices
        .iter()
        .all(|v| plane[0] * v[0] + plane[1] * v[1] + plane[2] * v[2] + plane[3] <= 0.0)
}

pub fn portal_behind_any_plane(vertices: &[[f32; 3]], planes: &[[f32; 4]]) -> bool {
    planes.iter().any(|p| portal_behind_plane(*p, vertices))
}

pub fn should_skip_portal(
    plane: [f32; 4],
    vertices: &[[f32; 3]],
    eye: [f32; 3],
    clip_planes: &[[f32; 4]],
) -> bool {
    portal_eye_dist(plane, eye) > 0.0 || portal_behind_any_plane(vertices, clip_planes)
}
