#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RadiationConveyer {
    pub origin: [f32; 3],
    pub half: [f32; 3],
    pub vector: [f32; 3],
    pub cmodel: u32,
}
