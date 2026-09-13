#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XBoneInfo {
    pub bounds: [f32; 6],
    pub radius_sq: f32,
}
