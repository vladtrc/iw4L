use crate::adjust_mover::ET_SCRIPTMOVER;

pub const CG_SCRIPT_MOVER_NODRAW: u32 = 0x20;

pub const SCRIPT_MOVER_BMODEL_SOLID: u32 = 0xffffff;

#[must_use]
pub fn cg_script_mover_add_bmodel(
    e_type: i32,
    e_flags: u32,
    solid: u32,
    dobj_present: bool,
) -> bool {
    e_type == ET_SCRIPTMOVER
        && (e_flags & CG_SCRIPT_MOVER_NODRAW) == 0
        && !dobj_present
        && solid == SCRIPT_MOVER_BMODEL_SOLID
}
