use fx_iw4::{
    FX_ELEM_AT_REST_NONE, FX_ELEM_POOL_CAPACITY, FX_ELEM_RUNTIME_STRIDE,
    FX_SPARK_CLOUD_HANDLE_NONE, elem_handle_from_ptr_delta,
};

pub const FX_ELEM_HANDLE_NONE: u16 = 0xffff;

#[derive(Clone, Debug)]
pub struct FxElemSlot {
    pub occupied: bool,
    pub def_index: u8,
    pub elem_type: u8,

    pub flags: i32,
    pub visual_count: u8,
    pub sequence: u8,
    pub at_rest_fraction: u8,
    pub supported_since_ms: Option<i32>,

    pub emit_residual: u8,
    pub next_elem_handle: u16,
    pub prev_elem_handle: u16,
    pub msec_begin: i32,

    pub life_span_msec: i32,
    /// `elem_random_seed` of the owning effect, sequence and `msec_begin`,
    /// none of which change while the element lives.
    pub random_seed: u64,
    pub motion_random: fx_iw4::FxMotionRandom,
    pub base_vel: [f32; 3],

    pub origin: [f32; 3],

    pub spawn_orientation: fx_iw4::FxOrientation,

    pub owner_effect_slot: u16,

    pub class_index: u8,

    pub sort_order: u8,

    pub spark_cloud_handle: u16,
}

impl Default for FxElemSlot {
    fn default() -> Self {
        Self {
            occupied: false,
            def_index: 0,
            elem_type: 0,
            flags: 0,
            visual_count: 0,
            sequence: 0,
            at_rest_fraction: FX_ELEM_AT_REST_NONE,
            supported_since_ms: None,
            emit_residual: 0,
            next_elem_handle: FX_ELEM_HANDLE_NONE,
            prev_elem_handle: FX_ELEM_HANDLE_NONE,
            msec_begin: 0,
            life_span_msec: 0,
            random_seed: 0,
            motion_random: fx_iw4::FxMotionRandom::default(),
            base_vel: [0.0; 3],
            origin: [0.0; 3],
            spawn_orientation: fx_iw4::FxOrientation {
                origin: [0.0; 3],
                axis: fx_iw4::FxOrientFrame::IDENTITY.axis,
            },
            owner_effect_slot: 0,
            class_index: 0,
            sort_order: 0,
            spark_cloud_handle: FX_SPARK_CLOUD_HANDLE_NONE,
        }
    }
}

impl FxElemSlot {
    #[inline]
    pub fn orientation(
        &self,
        effect_now: &fx_iw4::FxOrientFrame,
        effect_at_spawn: &fx_iw4::FxOrientFrame,
    ) -> fx_iw4::FxOrientation {
        if fx_iw4::elem_run_mode(self.flags) == fx_iw4::FX_ELEM_RUN_RELATIVE_TO_OFFSET {
            self.spawn_orientation
        } else {
            fx_iw4::get_orientation(self.flags, effect_now, effect_at_spawn, None)
        }
    }

    #[inline]
    pub fn world_origin(
        &self,
        effect_now: &fx_iw4::FxOrientFrame,
        effect_at_spawn: &fx_iw4::FxOrientFrame,
    ) -> [f32; 3] {
        let orient = self.orientation(effect_now, effect_at_spawn);
        fx_iw4::orientation_pos_to_world(orient.origin, orient.axis, self.origin)
    }
}

#[inline]
pub fn elem_handle_for_slot(slot: u32) -> u16 {
    elem_handle_from_ptr_delta(slot.wrapping_mul(FX_ELEM_RUNTIME_STRIDE as u32))
}

#[inline]
pub fn elem_slot_for_handle(handle: u16) -> Option<usize> {
    if handle == FX_ELEM_HANDLE_NONE {
        return None;
    }
    let byte = (handle as u32) << 2;
    if !byte.is_multiple_of(FX_ELEM_RUNTIME_STRIDE as u32) {
        return None;
    }
    let slot = (byte / FX_ELEM_RUNTIME_STRIDE as u32) as usize;
    (slot < FX_ELEM_POOL_CAPACITY as usize).then_some(slot)
}

pub fn elem_class_for_type(elem_type: u8) -> Option<usize> {
    if elem_type == 9 {
        return None;
    }
    if elem_type < 4 {
        Some(0)
    } else if elem_type == 4 || elem_type == 5 {
        Some(2)
    } else {
        Some(1)
    }
}
