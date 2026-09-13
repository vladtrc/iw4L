use crate::drawsurf::{GfxDrawSurf, GfxDrawSurfFields, pack, with_primary_light_index};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MaterialDrawSurfBakeInput {
    pub sort_key: u8,

    pub info_game_flags: u8,

    pub material_sorted_index: u16,

    pub technique0_absent: bool,

    pub technique1_present: bool,

    pub material_byte_4b: u8,

    pub technique0_flags: u8,
}

#[inline]
pub fn material_prepass(
    technique0_absent: bool,
    technique1_present: bool,
    material_byte_4b: u8,
    technique0_flags: u8,
) -> u8 {
    if technique0_absent {
        3 - u8::from(technique1_present)
    } else if (material_byte_4b & 4) != 0 {
        3
    } else {
        (!((technique0_flags >> 2) as u32) & 1) as u8
    }
}

#[inline]
pub fn bake_material_draw_surf_key(input: MaterialDrawSurfBakeInput) -> GfxDrawSurf {
    let prepass = material_prepass(
        input.technique0_absent,
        input.technique1_present,
        input.material_byte_4b,
        input.technique0_flags,
    );
    pack(GfxDrawSurfFields {
        custom_index: input.info_game_flags >> 6,
        material_sorted_index: input.material_sorted_index & 0xfff,
        prepass,
        primary_sort_key: input.sort_key & 0x3f,
        ..GfxDrawSurfFields::default()
    })
}

#[inline]
pub const fn material_sort_key_row(sort_key: u8) -> u8 {
    sort_key & 0x3f
}

#[inline]
pub const fn custom_index_from_info_game_flags(info_game_flags: u8) -> u8 {
    info_game_flags >> 6
}

#[inline]
pub const fn surface_casts_sun_shadow_bit(custom_index: u8, flags_bit0: bool) -> bool {
    custom_index != 0 && flags_bit0
}

pub fn world_surface_material(
    material_draw_surf: GfxDrawSurf,
    primary_light_index: u8,
) -> GfxDrawSurf {
    with_primary_light_index(material_draw_surf, primary_light_index)
}

pub fn fill_surface_materials(
    surface_material_slots: &[Option<usize>],
    surface_primary_lights: &[u8],
    baked_by_material_slot: &[Option<GfxDrawSurf>],
    out: &mut [GfxDrawSurf],
) {
    let n = out
        .len()
        .min(surface_material_slots.len())
        .min(surface_primary_lights.len());
    for i in 0..n {
        let packed = surface_material_slots[i]
            .and_then(|slot| baked_by_material_slot.get(slot).copied().flatten())
            .map(|base| world_surface_material(base, surface_primary_lights[i]))
            .unwrap_or(GfxDrawSurf::from_packed(0));
        out[i] = packed;
    }
}
