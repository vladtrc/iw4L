use crate::FX_MARK_STRIDE;

const _: () = assert!(FX_MARK_STRIDE == 0x40);
const _: () = assert!(FX_MARK_STAGING_TRI_STRIDE == 0x0e);
const _: () = assert!(FX_MARK_STAGING_POINT_STRIDE == 0x20);
const _: () = assert!(FX_MARK_STAGING_TRI_STRIDE != crate::FX_TRI_GROUP_STRIDE);
const _: () = assert!(FX_MARK_STAGING_POINT_STRIDE != crate::FX_POINT_GROUP_STRIDE);

pub const FX_MARK_STAGING_TRI_STRIDE: usize = 0x0e;

pub const FX_MARK_STAGING_POINT_STRIDE: usize = 0x20;

pub const FX_MARK_OFF_FRAME_COUNT_DRAWN: usize = 0x04;
pub const FX_MARK_OFF_FRAME_COUNT_ALLOCED: usize = 0x08;
pub const FX_MARK_OFF_ORIGIN: usize = 0x0c;
pub const FX_MARK_OFF_RADIUS: usize = 0x18;
pub const FX_MARK_OFF_TEX_COORD_AXIS: usize = 0x1c;
pub const FX_MARK_OFF_NATIVE_COLOR: usize = 0x28;
pub const FX_MARK_OFF_MATERIAL: usize = 0x2c;
pub const FX_MARK_OFF_CONTEXT: usize = 0x30;
pub const FX_MARK_OFF_TRI_COUNT: usize = 0x38;
pub const FX_MARK_OFF_POINT_COUNT: usize = 0x3a;
pub const FX_MARK_OFF_TRIS: usize = 0x3c;
pub const FX_MARK_OFF_POINTS: usize = 0x3e;

pub const FX_MARKS_INIT_FRAME_COUNT: i32 = 1;

pub const FX_MARKS_INIT_ALLOCED_COUNT: u32 = 0;

#[inline]
pub const fn fx_mark_world_brushes_invokes_callback(any_marks: bool) -> bool {
    any_marks
}

#[inline]
pub const fn fx_mark_point_groups_for_count(point_count: u32) -> u32 {
    point_count.saturating_add(1) / 2
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxAllocMarkRefuse {
    CallbackNotInvoked,

    TriCountZero,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxMarkConstructed {
    pub frame_count_drawn: i32,
    pub frame_count_alloced: i32,
    pub origin: [f32; 3],
    pub radius: f32,
    pub tex_coord_axis: [f32; 3],
    pub native_color: u32,
    pub material: u32,
    pub context: u32,
    pub tri_count: u8,
    pub point_count: i16,
    pub tris: u16,
    pub points: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct FxAllocMarkRequest {
    pub any_marks: bool,
    pub tri_count: u32,
    pub point_count: u32,
    pub origin: [f32; 3],
    pub radius: f32,
    pub tex_coord_axis: [f32; 3],
    pub native_color: u32,
    pub material: u32,
    pub frame_count: i32,

    pub first_tri_context: u32,
}

pub fn fx_alloc_and_construct_mark(
    req: &FxAllocMarkRequest,
    tri_handle: u16,
    point_handle: u16,
) -> Result<FxMarkConstructed, FxAllocMarkRefuse> {
    if !fx_mark_world_brushes_invokes_callback(req.any_marks) {
        return Err(FxAllocMarkRefuse::CallbackNotInvoked);
    }
    if req.tri_count == 0 {
        return Err(FxAllocMarkRefuse::TriCountZero);
    }
    Ok(FxMarkConstructed {
        frame_count_drawn: req.frame_count.wrapping_sub(1),
        frame_count_alloced: req.frame_count,
        origin: req.origin,
        radius: req.radius,
        tex_coord_axis: req.tex_coord_axis,
        native_color: req.native_color,
        material: req.material,
        context: req.first_tri_context,
        tri_count: req.tri_count as u8,
        point_count: req.point_count as i16,
        tris: tri_handle,
        points: point_handle,
    })
}
