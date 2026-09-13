#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTrailDef {
    pub scroll_time_msec: i32,
    pub repeat_dist: i32,
    pub inv_split_dist: f32,
    pub inv_split_arc_dist: f32,
    pub inv_split_time: f32,
    pub vert_count: i32,
    pub verts: u32,
    pub ind_count: i32,
    pub inds: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxTrailVertex {
    pub pos: [f32; 2],
    pub normal: [f32; 2],
    pub tex_coord: f32,
}
