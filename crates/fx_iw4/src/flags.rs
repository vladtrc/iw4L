pub const FX_ELEM_USE_COLLISION: i32 = 0x100;

pub const FX_ELEM_DIE_ON_TOUCH: i32 = 0x200;

pub const FX_ELEM_SPAWN_FRUSTUM_CULL: i32 = 0x4;

pub const FX_ELEM_RUNNER_USES_RAND_ROT: i32 = 0x8;

pub const FX_ELEM_VEL_LOCAL: i32 = 0x0100_0000;

pub const FX_ELEM_VEL_WORLD: i32 = 0x0200_0000;

pub const FX_ELEM_UPDATE_HAS_VEL_GRAPH: i32 = 0x0600_0000;

pub const FX_ELEM_USE_MODEL_PHYSICS: i32 = 0x0800_0000;

pub const FX_ELEM_RUN_MASK: i32 = 0xc0;
pub const FX_ELEM_RUN_RELATIVE_TO_SPAWN: i32 = 0x40;
pub const FX_ELEM_RUN_RELATIVE_TO_EFFECT: i32 = 0x80;
pub const FX_ELEM_RUN_RELATIVE_TO_OFFSET: i32 = 0xc0;

#[inline]
pub const fn fx_elem_run_mode(flags: i32) -> i32 {
    flags & FX_ELEM_RUN_MASK
}

#[inline]
pub const fn fx_elem_uses_collision(flags: i32) -> bool {
    (flags & FX_ELEM_USE_COLLISION) != 0
}

#[inline]
pub const fn fx_elem_dies_on_touch(flags: i32) -> bool {
    (flags & FX_ELEM_DIE_ON_TOUCH) != 0
}

#[inline]
pub const fn fx_elem_spawn_frustum_cull(flags: i32) -> bool {
    (flags & FX_ELEM_SPAWN_FRUSTUM_CULL) != 0
}

#[inline]
pub const fn fx_elem_uses_vel_local(flags: i32) -> bool {
    (flags & FX_ELEM_VEL_LOCAL) != 0
}

#[inline]
pub const fn fx_elem_uses_vel_world(flags: i32) -> bool {
    (flags & FX_ELEM_VEL_WORLD) != 0
}

#[inline]
pub const fn fx_elem_update_has_velocity_graph(flags: i32) -> bool {
    (flags & FX_ELEM_UPDATE_HAS_VEL_GRAPH) != 0
}

#[inline]
pub const fn fx_elem_skips_position_update(elem_type: u8, flags: i32) -> bool {
    elem_type == 7 && (flags & FX_ELEM_USE_MODEL_PHYSICS) != 0
}
