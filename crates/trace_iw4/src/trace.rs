pub const HITTYPE_ENTITY: i32 = 1;

pub const HITTYPE_GLASS: i32 = 4;

#[must_use]
pub const fn trace_get_glass_hit_id(hit_type: i32, hit_id: u16) -> u16 {
    if hit_type == HITTYPE_GLASS { hit_id } else { 0 }
}

#[must_use]
pub const fn cm_brush_sweep_hit_kind(glass_encoded: u16) -> (i32, u16) {
    if glass_encoded != 0 {
        (HITTYPE_GLASS, glass_encoded)
    } else {
        (HITTYPE_ENTITY, ENTITYNUM_WORLD)
    }
}

pub const ENTITYNUM_WORLD: u16 = 0x7fe;

pub const ENTITYNUM_NONE: u16 = 0x7ff;

pub const HITTYPE_DYNENT_FIRST: i32 = 2;

pub const HITTYPE_DYNENT_LAST: i32 = 4;

#[must_use]
pub const fn trace_get_entity_hit_id(hit_type: i32, hit_id: u16) -> u16 {
    if hit_type >= HITTYPE_DYNENT_FIRST && hit_type <= HITTYPE_DYNENT_LAST {
        return ENTITYNUM_WORLD;
    }
    if hit_type == HITTYPE_ENTITY {
        return hit_id;
    }
    ENTITYNUM_NONE
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Trace {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub surface_flags: u32,
    pub contents: u32,
    pub material: u32,
    pub hit_type: i32,
    pub hit_id: u16,
    pub model_index: u16,
    pub part_name: u16,
    pub part_group: u16,
    pub allsolid: u8,
    pub startsolid: u8,
    pub walkable: u8,
    pub endpos: [f32; 3],
}
