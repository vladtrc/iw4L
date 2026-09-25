use crate::ScriptModelId;

#[derive(Clone, Debug, PartialEq)]
pub struct RadiationDigger {
    pub body: ScriptModelId,
    pub arm: ScriptModelId,
    pub blade: ScriptModelId,
    pub pieces: Vec<(ScriptModelId, [f32; 3])>,
    pub body_angles: [f32; 3],
    pub arm_angles: [f32; 3],
    pub blade_angles: [f32; 3],
}
