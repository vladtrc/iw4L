#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxSystem {
    pub effects: u32,
    pub elems: u32,
    pub trails: u32,
    pub trail_elems: u32,
    pub spark_cloud_history: u32,
    pub first_active_effect: i32,
    pub first_new_effect: i32,
    pub first_free_effect: i32,
    pub all_effect_handles: u32,
    pub spotlight_count: i32,
    pub iterator_count: i32,
    pub msec_now: i32,
    pub needs_garbage_collection: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FxEffect {
    pub def: u32,
    pub status: i32,
    pub first_trail_handle: u16,
    pub own_effect_handle: u16,
    pub bolt: u8,
    pub msec_begin: i32,
    pub msec_last_update: i32,
}
