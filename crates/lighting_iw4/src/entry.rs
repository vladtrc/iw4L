#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GfxLightGridEntry {
    pub colors_index: u16,
    pub primary_light_index: u8,
    pub needs_trace: u8,
}
