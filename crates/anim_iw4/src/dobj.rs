#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DObj {
    pub tree: u32,
    pub num_models: u8,
    pub hide_bits: u32,
    pub models: u32,
}
