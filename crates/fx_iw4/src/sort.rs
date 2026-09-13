#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxInsertSortElem {
    pub def_sort_order: u8,
    pub elem_type: u8,
    pub dist_to_cam_sq: f32,
}

#[inline]
pub fn fx_sort_dist_to_cam_sq(camera_origin: [f32; 3], pos_world: [f32; 3]) -> f32 {
    let dx = camera_origin[0] - pos_world[0];
    let dy = camera_origin[1] - pos_world[1];
    let dz = camera_origin[2] - pos_world[2];
    dx * dx + dy * dy + dz * dz
}

#[inline]
pub fn fx_existing_elem_sorts_before_new(
    existing_elem_type: u8,
    existing_visual_count: u8,
    existing_sort_order: u8,
    existing_dist_to_cam_sq: f32,
    new_sort_order: u8,
    new_dist_to_cam_sq: f32,
) -> bool {
    if existing_elem_type > 3 {
        return true;
    }
    if existing_visual_count == 0 {
        return false;
    }
    if existing_sort_order < new_sort_order {
        return true;
    }
    if existing_sort_order > new_sort_order {
        return false;
    }
    new_dist_to_cam_sq < existing_dist_to_cam_sq
}
