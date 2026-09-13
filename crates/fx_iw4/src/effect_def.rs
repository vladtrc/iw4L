#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxEffectDef {
    pub msec_looping_life: i32,
    pub looping_count: i32,
    pub one_shot_count: i32,
    pub emission_count: i32,
    pub elem_defs: u32,
}
