use dpvs_iw4::{GfxDrawSurf, add_bsp_draw_surfs_range_sun_shadow_fast};
use render_frame::{SunShadowCasterLists, SunShadowPartitionLists};

pub fn add_bsp_sun_shadow_partition(
    begin: u32,
    end: u32,
    surface_vis: &[u8],
    caster_words: &[u32],
    surface_materials: &[GfxDrawSurf],
    out: &mut [u16],
) -> usize {
    add_bsp_draw_surfs_range_sun_shadow_fast(
        begin,
        end,
        surface_vis,
        caster_words,
        surface_materials,
        out,
    )
}

pub fn pack_sun_shadow_caster_lists(
    vis: [Vec<u8>; 2],
    smodel_vis: [Vec<u8>; 2],
    begin: u32,
    end: u32,
    caster_words: &[u32],
    surface_materials: &[GfxDrawSurf],
) -> SunShadowCasterLists {
    let mut partitions = [
        SunShadowPartitionLists::default(),
        SunShadowPartitionLists::default(),
    ];
    for i in 0..2 {
        let cap = end.saturating_sub(begin) as usize;
        let mut ids = vec![0u16; cap];
        let n = add_bsp_sun_shadow_partition(
            begin,
            end,
            &vis[i],
            caster_words,
            surface_materials,
            &mut ids,
        );
        ids.truncate(n);
        partitions[i] = SunShadowPartitionLists {
            surface_vis: vis[i].clone(),
            smodel_vis: smodel_vis[i].clone(),
            world_surfs: ids,
        };
    }
    SunShadowCasterLists { partitions }
}
