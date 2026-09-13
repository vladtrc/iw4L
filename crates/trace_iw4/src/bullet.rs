#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletFireParams {
    pub ignore_hit_ent: i32,
    pub start: [f32; 3],
    pub dir: [f32; 3],
}
