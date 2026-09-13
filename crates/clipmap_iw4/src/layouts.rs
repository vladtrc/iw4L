#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cbrush {
    pub numsides: u16,
    pub glass_piece_index: u16,
    pub sides: u32,
    pub base_adjacent_side: u32,
    pub axial_material_num: [i16; 6],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cbrushside {
    pub plane: u32,
    pub material_num: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cnode {
    pub plane: u32,
    pub children: [i16; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cleaf {
    pub first_coll_aabb_index: u16,
    pub coll_aabb_count: u16,
    pub brush_contents: i32,
    pub terrain_contents: i32,
    pub leaf_brush_node: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CleafBrushNode {
    pub axis: i8,
    pub leaf_brush_count: i16,
    pub contents: i32,
    pub child_offset_0: u16,
    pub child_offset_1: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cmodel {
    pub bounds_mid_point: [f32; 3],
    pub bounds_half_size: [f32; 3],
    pub radius: f32,
    pub leaf_brush_node: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionAabbTree {
    pub origin: [f32; 3],
    pub material_index: u16,
    pub child_count: u16,
    pub half_size: [f32; 3],
    pub u: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CollisionPartition {
    pub tri_count: u8,
    pub border_count: u8,
    pub first_vert_segment: u8,
    pub first_tri: i32,
    pub borders: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipMaterial {
    pub name: u32,
    pub surface_flags: u32,
    pub content_flags: u32,
}
