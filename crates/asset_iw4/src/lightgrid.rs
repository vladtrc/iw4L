#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GfxLightGrid {
    pub has_light_regions: u8,
    pub sun_primary_light_index: u32,
    pub mins: [u16; 3],
    pub maxs: [u16; 3],
    pub row_axis: u32,
    pub col_axis: u32,
    pub row_data_start: u32,
    pub raw_row_data_size: u32,
    pub raw_row_data: u32,
    pub entry_count: u32,
    pub entries: u32,
    pub color_count: u32,
    pub colors: u32,
}
