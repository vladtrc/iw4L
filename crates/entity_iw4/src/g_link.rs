#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvLinkBounds {
    pub mid: [f32; 3],

    pub half: [f32; 3],
}

pub fn sv_link_entity_world_bounds(
    origin: [f32; 3],
    box_mid: [f32; 3],
    box_half: [f32; 3],
) -> SvLinkBounds {
    SvLinkBounds {
        mid: [
            origin[0] + box_mid[0],
            origin[1] + box_mid[1],
            origin[2] + box_mid[2],
        ],
        half: [box_half[0] + 1.0, box_half[1] + 1.0, box_half[2] + 1.0],
    }
}

pub fn sv_link_entity_needs_rotated_radius(snapped_angles: [f32; 3], box_half: [f32; 3]) -> bool {
    box_half != [0.0, 0.0, 0.0] && snapped_angles != [0.0, 0.0, 0.0]
}
