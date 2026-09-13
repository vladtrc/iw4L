#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxElem {
    pub def_index: u8,
    pub sequence: u8,
    pub at_rest_fraction: u8,
    pub emit_residual: u8,
    pub next_elem_handle: u16,
    pub prev_elem_handle: u16,
    pub msec_begin: i32,
    pub spark_cloud_handle: u16,
}

pub const FX_ELEM_AT_REST_NONE: u8 = 0xff;
