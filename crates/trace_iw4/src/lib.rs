#![no_std]
#![forbid(unsafe_code)]

mod box_trace;
mod bullet;
mod surface;
mod trace;

pub use box_trace::{BrushRef, trace_box, trace_box_into, trace_capsule, trace_capsule_hit};
pub use bullet::BulletFireParams;
pub use surface::surface_type_from_flags;
pub use trace::{
    ENTITYNUM_NONE, ENTITYNUM_WORLD, HITTYPE_DYNENT_FIRST, HITTYPE_DYNENT_LAST, HITTYPE_ENTITY,
    HITTYPE_GLASS, Trace, cm_brush_sweep_hit_kind, trace_get_entity_hit_id, trace_get_glass_hit_id,
};
