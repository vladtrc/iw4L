use crate::ScriptModelId;

#[derive(Clone, Debug, PartialEq)]
pub struct RadiationMovingDigger {
    pub id: ScriptModelId,
    pub start: [f32; 3],
    pub legs: Vec<([f32; 3], u32)>,
}
