#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CorpseInfo {
    pub anim_tree: i32,
    pub entnum: i32,
    pub time: i32,
    pub legs_anim: i32,
    pub torso_anim: i32,
    pub torso_pitch: u32,
    pub waist_pitch: u32,
    pub falling: u8,
}

pub const CORPSE_INFO_LIVE_INFO_AT: usize = 0x0c;

pub const CORPSE_INFO_TREE_REWRITE_AT: usize = 0x508;
