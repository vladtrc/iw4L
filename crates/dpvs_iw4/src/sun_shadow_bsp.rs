use crate::drawsurf::GfxDrawSurf;

#[inline]
fn caster_bit(words: &[u32], index: usize) -> bool {
    let word = index / 32;
    let bit = index % 32;
    words.get(word).is_some_and(|w| (*w & (1u32 << bit)) != 0)
}

pub fn add_bsp_draw_surfs_range_sun_shadow_fast(
    begin: u32,
    end: u32,
    surface_vis: &[u8],
    caster_words: &[u32],
    surface_materials: &[GfxDrawSurf],
    out: &mut [u16],
) -> usize {
    let mut n = 0usize;
    let begin = begin as usize;
    let end = end as usize;
    for surf in begin..end {
        if n >= out.len() {
            break;
        }
        if surface_vis.get(surf).copied().unwrap_or(0) == 0 {
            continue;
        }
        if !caster_bit(caster_words, surf) {
            continue;
        }
        if surface_materials
            .get(surf)
            .is_none_or(|key| key.packed == 0)
        {
            continue;
        }
        let Ok(id) = u16::try_from(surf) else {
            continue;
        };
        out[n] = id;
        n += 1;
    }
    n
}

#[inline]
pub fn surface_material_at(surface_materials: &[GfxDrawSurf], surf: usize) -> GfxDrawSurf {
    surface_materials
        .get(surf)
        .copied()
        .unwrap_or(GfxDrawSurf::from_packed(0))
}
