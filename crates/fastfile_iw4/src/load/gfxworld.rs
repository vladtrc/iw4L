use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{
    GfxLightGridGeometry, GfxLightmapPair, GfxSunEffectsGeometry, GfxWorldGeometry,
    MAX_LIGHTMAP_PAGES, Ptr, Result, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_gfxworld(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.tag_alloc(0xffff, 4, s.layout(sz::GFX_WORLD, 944));
    let p = s.alloc_load(4, s.layout(sz::GFX_WORLD, 944))?;

    let plane_count = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;
    let node_count = s.i32_at(p, s.layout(12, 20))?.max(0) as usize;
    let surface_count = s.u32_at(p, s.layout(16, 24))? as usize;
    let sky_count = s.i32_at(p, s.layout(20, 28))?.max(0) as usize;
    let sun_primary_light_count = s.u32_at(p, s.layout(28, 40))? as usize;
    let primary_light_count = s.u32_at(p, s.layout(32, 44))? as usize;
    let sort_key_distortion = s.u32_at(p, s.layout(48, 60))?;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    s.follow_string(p, s.layout(4, 8))?;

    let mut first_sky_image = None;
    let skies = s.plain_array(p, s.layout(24, 32), 4, s.layout(sz::GFX_SKY, 32), sky_count)?;
    if let Some(skies_ptr) = skies {
        for i in 0..sky_count {
            let sky = skies_ptr.at(i * s.layout(sz::GFX_SKY, 32));
            let start_surfs = s.i32_at(sky, 0)?.max(0) as usize;
            s.plain_array(sky, s.layout(4, 8), 4, 4, start_surfs)?;
            asset_ptr_at(s, links, AssetType::Image, sky.at(s.layout(8, 16)))?;
            if first_sky_image.is_none() {
                first_sky_image = Some(sky.at(s.layout(8, 16)));
            }
        }
    }

    let dp = p.at(s.layout(52, 64));
    let cell_count = s.i32_at(dp, 0)?.max(0) as usize;
    let planes = s.plain_array(dp, s.layout(4, 8), 4, sz::CPLANE, plane_count)?;

    let nodes = s.plain_array(dp, s.layout(8, 16), 2, 2, node_count)?;

    runtime_array(s, dp, s.layout(12, 24), 4, 4, cell_count * 512)?;

    let aabb_counts = s.plain_array(p, s.layout(68, 96), 4, 4, cell_count)?;
    let mut aabb_node_count = 0usize;
    let aabb_trees = s.plain_array(p, s.layout(72, 104), 128, s.pointer_bytes(), cell_count)?;
    if let Some(trees) = aabb_trees {
        for i in 0..cell_count {
            if s.begin_body(trees.at(i * s.pointer_bytes()))? {
                let count = match aabb_counts {
                    Some(counts) => s.i32_at(counts, i * 4)?.max(0) as usize,
                    None => 0,
                };
                aabb_node_count += count;
                let aabbs = s.alloc_load(4, s.layout(sz::GFX_AABB_TREE, 56) * count)?;

                s.fixup_slot(trees.at(i * s.pointer_bytes()), aabbs)?;
                for j in 0..count {
                    let aabb = aabbs.at(j * s.layout(sz::GFX_AABB_TREE, 56));
                    let smodel_index_count = s.u16_at(aabb, 34)? as usize;
                    s.plain_array(aabb, s.layout(36, 40), 2, 2, smodel_index_count)?;
                }
            }
        }
    }

    let mut portal_count = 0usize;
    let cells = s.plain_array(
        p,
        s.layout(76, 112),
        4,
        s.layout(sz::GFX_CELL, 56),
        cell_count,
    )?;
    if let Some(cells_ptr) = cells {
        for i in 0..cell_count {
            let cell = cells_ptr.at(i * s.layout(sz::GFX_CELL, 56));
            let cell_portals = s.i32_at(cell, 24)?.max(0) as usize;
            portal_count += cell_portals;
            if let Some(portals) = s.plain_array(
                cell,
                s.layout(28, 32),
                4,
                s.layout(sz::GFX_PORTAL, 80),
                cell_portals,
            )? {
                for j in 0..cell_portals {
                    let portal = portals.at(j * s.layout(sz::GFX_PORTAL, 80));
                    let vertex_count = s.u8_at(portal, s.layout(34, 50))? as usize;
                    s.plain_array(portal, s.layout(28, 40), 4, 12, vertex_count)?;
                }
            }
            let reflection_probe_count = s.u8_at(cell, s.layout(32, 40))? as usize;
            s.plain_array(cell, s.layout(36, 48), 1, 1, reflection_probe_count)?;
        }
    }

    let geometry = load_gfxworld_draw(s, links, p.at(s.layout(80, 120)))?;
    let light_grid = load_gfx_light_grid(s, p.at(s.layout(152, 264)))?;

    let brush_model_count = s.i32_at(p, s.layout(208, 352))?.max(0) as usize;
    let brush_models = s.plain_array(
        p,
        s.layout(212, 360),
        4,
        sz::GFX_BRUSH_MODEL,
        brush_model_count,
    )?;

    let mut bounds = [0u32; 6];
    for (i, word) in bounds.iter_mut().enumerate() {
        *word = s.f32_at(p, s.layout(216, 368) + i * 4)?.to_bits();
    }

    let material_memory_count = s.i32_at(p, s.layout(244, 396))?.max(0) as usize;
    if let Some(memories) = s.plain_array(
        p,
        s.layout(248, 400),
        4,
        s.layout(sz::MATERIAL_MEMORY, 16),
        material_memory_count,
    )? {
        for i in 0..material_memory_count {
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                memories.at(i * s.layout(sz::MATERIAL_MEMORY, 16)),
            )?;
        }
    }

    let sun = p.at(s.layout(252, 408));
    let sprite = sun.at(s.layout(4, 8));
    let flare = sun.at(s.layout(8, 16));
    asset_ptr_at(s, links, AssetType::Material, sprite)?;
    let (sprite_header, sprite_name, sprite_name_len) = sun_material_ref(s);
    asset_ptr_at(s, links, AssetType::Material, flare)?;
    let (flare_header, flare_name, flare_name_len) = sun_material_ref(s);
    let mut raw = [0u8; 112];
    if let Ok(bytes) = s.slice_at(sun, 0, 112) {
        raw[..bytes.len()].copy_from_slice(bytes);
    }
    let sun_effects = Some(GfxSunEffectsGeometry {
        sprite,
        flare,
        sprite_header,
        flare_header,
        sprite_name,
        sprite_name_len,
        flare_name,
        flare_name_len,
        raw,
    });

    let mut outdoor_lookup = [0u32; 16];
    for (i, word) in outdoor_lookup.iter_mut().enumerate() {
        *word = s.f32_at(p, s.layout(348, 520) + i * 4)?.to_bits();
    }

    let outdoor_slot = p.at(s.layout(412, 584));
    asset_ptr_at(s, links, AssetType::Image, outdoor_slot)?;
    let outdoor_image = match s.ptr_at(outdoor_slot, 0)? {
        ZonePtr::Null => None,
        _ => Some(outdoor_slot),
    };

    let dyn_dpvs = p.at(s.layout(564, 840));
    let dyn_model_count = s.u32_at(dyn_dpvs, 8)? as usize;
    let dyn_brush_count = s.u32_at(dyn_dpvs, 12)? as usize;

    runtime_array(
        s,
        p,
        s.layout(416, 592),
        4,
        4,
        cell_count * cell_count.div_ceil(32),
    )?;
    runtime_array(s, p, s.layout(420, 600), 4, 4, cell_count.div_ceil(32))?;
    runtime_array(s, p, s.layout(424, 608), 4, 6, dyn_model_count)?;
    runtime_array(s, p, s.layout(428, 616), 4, 4, dyn_brush_count)?;

    let non_sun_lights =
        primary_light_count.saturating_sub(sun_primary_light_count.saturating_add(1));
    runtime_array(s, p, s.layout(432, 624), 4, 4, non_sun_lights * 8192)?;
    runtime_array(
        s,
        p,
        s.layout(436, 632),
        4,
        4,
        dyn_model_count * non_sun_lights,
    )?;
    runtime_array(
        s,
        p,
        s.layout(440, 640),
        4,
        4,
        dyn_brush_count * non_sun_lights,
    )?;
    runtime_array(s, p, s.layout(444, 648), 1, 1, dyn_model_count)?;

    let shadow_geometry = s.plain_array(
        p,
        s.layout(448, 656),
        4,
        s.layout(sz::GFX_SHADOW_GEOMETRY, 24),
        primary_light_count,
    )?;
    if let Some(shadows) = shadow_geometry {
        for i in 0..primary_light_count {
            let shadow = shadows.at(i * s.layout(sz::GFX_SHADOW_GEOMETRY, 24));
            let surface_count = s.u16_at(shadow, 0)? as usize;
            let smodel_count = s.u16_at(shadow, 2)? as usize;
            s.plain_array(shadow, s.layout(4, 8), 2, 2, surface_count)?;
            s.plain_array(shadow, s.layout(8, 16), 2, 2, smodel_count)?;
        }
    }

    let light_regions = s.plain_array(
        p,
        s.layout(452, 664),
        4,
        s.layout(sz::GFX_LIGHT_REGION, 16),
        primary_light_count,
    )?;
    if let Some(regions) = light_regions {
        for i in 0..primary_light_count {
            let region = regions.at(i * s.layout(sz::GFX_LIGHT_REGION, 16));
            let count = s.u32_at(region, 0)? as usize;
            if let Some(hulls) = s.plain_array(
                region,
                s.layout(4, 8),
                4,
                s.layout(sz::GFX_LIGHT_REGION_HULL, 88),
                count,
            )? {
                for j in 0..count {
                    let hull = hulls.at(j * s.layout(sz::GFX_LIGHT_REGION_HULL, 88));
                    let axis_count = s.u32_at(hull, 72)? as usize;
                    s.plain_array(hull, s.layout(76, 80), 4, 20, axis_count)?;
                }
            }
        }
    }

    let dpvs = load_gfx_dpvs_static(s, links, p.at(s.layout(456, 672)), surface_count)?;
    load_gfx_dpvs_dynamic(s, dyn_dpvs, cell_count)?;

    let hero_count = s.u32_at(p, s.layout(616, 924))? as usize;
    s.plain_array(
        p,
        s.layout(620, 928),
        4,
        sz::GFX_HERO_ONLY_LIGHT,
        hero_count,
    )?;

    s.pop()?;

    s.record_gfx_world(GfxWorldGeometry {
        sun_effects,
        sort_key_distortion: Some(sort_key_distortion),
        surfaces: dpvs.surfaces,
        surface_count,
        cell_count,
        plane_count,
        node_count,
        portal_count,
        aabb_node_count,
        first_sky_image,
        skies,
        sky_count,
        outdoor_image,
        outdoor_lookup,
        light_grid,
        sun_primary_light_count,
        primary_light_count,
        planes,
        nodes,
        cells,
        aabb_tree_counts: aabb_counts,
        aabb_trees,
        dpvs_static: Some(p.at(s.layout(456, 672))),
        smodel_count: dpvs.smodel_count,
        static_surface_count: dpvs.static_surface_count,
        static_surface_count_no_decal: dpvs.static_surface_count_no_decal,
        lit_opaque_surfs_begin: dpvs.lit_opaque_surfs_begin,
        lit_opaque_surfs_end: dpvs.lit_opaque_surfs_end,
        lit_trans_surfs_begin: dpvs.lit_trans_surfs_begin,
        lit_trans_surfs_end: dpvs.lit_trans_surfs_end,
        shadow_caster_surfs_begin: dpvs.shadow_caster_surfs_begin,
        shadow_caster_surfs_end: dpvs.shadow_caster_surfs_end,
        emissive_surfs_begin: dpvs.emissive_surfs_begin,
        emissive_surfs_end: dpvs.emissive_surfs_end,
        sorted_surf_index: dpvs.sorted_surf_index,
        smodel_insts: dpvs.smodel_insts,
        surfaces_bounds: dpvs.surfaces_bounds,
        smodel_draw_insts: dpvs.smodel_draw_insts,
        dyn_model_count,
        dyn_brush_count,
        light_regions,
        shadow_geometry,
        model_count: brush_model_count,
        models: brush_models,
        bounds: Some(bounds),
        ..geometry
    });
    Ok(())
}

fn load_gfxworld_draw(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<GfxWorldGeometry> {
    let probe_count = s.i32_at(p, 0)?.max(0) as usize;
    let lightmap_count = s.i32_at(p, s.layout(16, 32))?.max(0) as usize;

    let mut reflection_probes = None;
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let a = s.alloc_load(4, s.pointer_bytes() * probe_count)?;
        reflection_probes = Some(a);
        for i in 0..probe_count {
            asset_ptr_at(s, links, AssetType::Image, a.at(i * s.pointer_bytes()))?;
        }
    }
    let reflection_probe_origins = s.plain_array(p, s.layout(8, 16), 4, 12, probe_count)?;
    runtime_array(s, p, s.layout(12, 24), 4, s.pointer_bytes(), probe_count)?;

    let mut lightmaps = [GfxLightmapPair::default(); MAX_LIGHTMAP_PAGES];
    if let Some(a) = s.plain_array(
        p,
        s.layout(20, 40),
        4,
        s.layout(sz::GFX_LIGHTMAP_ARRAY, 16),
        lightmap_count,
    )? {
        let retain = lightmap_count.min(MAX_LIGHTMAP_PAGES);
        for i in 0..lightmap_count {
            let lightmap = a.at(i * s.layout(sz::GFX_LIGHTMAP_ARRAY, 16));
            let serial = s.image_serial();
            asset_ptr_at(s, links, AssetType::Image, lightmap.at(0))?;
            let primary = if s.image_serial() != serial {
                s.latest_image()
            } else {
                None
            };
            let serial = s.image_serial();
            asset_ptr_at(s, links, AssetType::Image, lightmap.at(s.layout(4, 8)))?;
            let secondary = if s.image_serial() != serial {
                s.latest_image()
            } else {
                None
            };
            if i < retain {
                lightmaps[i] = GfxLightmapPair { primary, secondary };
            }
        }
    }
    runtime_array(s, p, s.layout(24, 48), 4, s.pointer_bytes(), lightmap_count)?;
    runtime_array(s, p, s.layout(28, 56), 4, s.pointer_bytes(), lightmap_count)?;

    asset_ptr_at(s, links, AssetType::Image, p.at(s.layout(32, 64)))?;
    asset_ptr_at(s, links, AssetType::Image, p.at(s.layout(36, 72)))?;

    let vertex_count = s.i32_at(p, s.layout(40, 80))?.max(0) as usize;
    let vertices = s.plain_array(p, s.layout(44, 88), 4, sz::GFX_WORLD_VERTEX, vertex_count)?;

    let vertex_data_size = s.i32_at(p, s.layout(52, 104))?.max(0) as usize;
    s.plain_array(p, s.layout(56, 112), 1, 1, vertex_data_size)?;

    let index_count = s.i32_at(p, s.layout(64, 128))?.max(0) as usize;
    let indices = s.plain_array(p, s.layout(68, 136), 2, 2, index_count)?;

    Ok(GfxWorldGeometry {
        vertices,
        vertex_count,
        indices,
        index_count,
        lightmap_count,
        lightmaps,
        first_lightmap_primary: lightmaps[0].primary,
        first_lightmap_secondary: lightmaps[0].secondary,
        reflection_probes,
        reflection_probe_origins,
        reflection_probe_count: probe_count,
        ..Default::default()
    })
}

fn load_gfx_light_grid(s: &mut ZoneStream<'_>, p: Ptr) -> Result<GfxLightGridGeometry> {
    let has_light_regions = s.u8_at(p, 0)? != 0;
    let sun_primary_light_index = s.u32_at(p, 4)?;
    let mins = [s.u16_at(p, 8)?, s.u16_at(p, 10)?, s.u16_at(p, 12)?];
    let maxs = [s.u16_at(p, 14)?, s.u16_at(p, 16)?, s.u16_at(p, 18)?];
    let row_axis = s.u32_at(p, 20)?;
    let col_axis = s.u32_at(p, 24)?;

    let rows = match (mins.get(row_axis as usize), maxs.get(row_axis as usize)) {
        (Some(&lo), Some(&hi)) => usize::from(hi).saturating_sub(usize::from(lo)) + 1,
        _ => 0,
    };
    let row_data_start = s.plain_array(p, s.layout(28, 32), 2, 2, rows)?;

    let raw_size = s.i32_at(p, s.layout(32, 40))?.max(0) as usize;
    let raw_row_data = s.plain_array(p, s.layout(36, 48), 1, 1, raw_size)?;

    let entry_count = s.i32_at(p, s.layout(40, 56))?.max(0) as usize;
    let entries = s.plain_array(p, s.layout(44, 64), 4, 4, entry_count)?;

    let color_count = s.i32_at(p, s.layout(48, 72))?.max(0) as usize;
    let colors = s.plain_array(
        p,
        s.layout(52, 80),
        4,
        sz::GFX_LIGHT_GRID_COLORS,
        color_count,
    )?;
    Ok(GfxLightGridGeometry {
        has_light_regions,
        sun_primary_light_index,
        mins,
        maxs,
        row_axis,
        col_axis,
        row_data_start,
        row_count: rows,
        raw_row_data,
        raw_row_data_size: raw_size,
        entries,
        entry_count,
        colors,
        color_count,
    })
}

struct DpvsStaticLoad {
    surfaces: Option<Ptr>,
    smodel_count: usize,
    static_surface_count: usize,
    static_surface_count_no_decal: usize,
    lit_opaque_surfs_begin: u32,
    lit_opaque_surfs_end: u32,
    lit_trans_surfs_begin: u32,
    lit_trans_surfs_end: u32,
    shadow_caster_surfs_begin: u32,
    shadow_caster_surfs_end: u32,
    emissive_surfs_begin: u32,
    emissive_surfs_end: u32,
    sorted_surf_index: Option<Ptr>,
    smodel_insts: Option<Ptr>,
    surfaces_bounds: Option<Ptr>,
    smodel_draw_insts: Option<Ptr>,
}

fn load_gfx_dpvs_static(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    surface_count: usize,
) -> Result<DpvsStaticLoad> {
    let smodel_count = s.u32_at(p, 0)? as usize;
    let static_surface_count = s.u32_at(p, 4)? as usize;
    let static_surface_count_no_decal = s.u32_at(p, 8)? as usize;
    let lit_opaque_surfs_begin = s.u32_at(p, 12)?;
    let lit_opaque_surfs_end = s.u32_at(p, 16)?;
    let lit_trans_surfs_begin = s.u32_at(p, 20)?;
    let lit_trans_surfs_end = s.u32_at(p, 24)?;
    let shadow_caster_surfs_begin = s.u32_at(p, 28)?;
    let shadow_caster_surfs_end = s.u32_at(p, 32)?;
    let emissive_surfs_begin = s.u32_at(p, 36)?;
    let emissive_surfs_end = s.u32_at(p, 40)?;

    for field in [s.layout(52, 56), s.layout(56, 64), s.layout(60, 72)] {
        runtime_array(s, p, field, 1, 1, smodel_count)?;
    }
    for field in [s.layout(64, 80), s.layout(68, 88), s.layout(72, 96)] {
        runtime_array(s, p, field, 1, 1, static_surface_count)?;
    }

    let sorted_surf_index = s.plain_array(
        p,
        s.layout(76, 104),
        2,
        2,
        static_surface_count + static_surface_count_no_decal,
    )?;
    let smodel_insts = s.plain_array(
        p,
        s.layout(80, 112),
        4,
        sz::GFX_STATIC_MODEL_INST,
        smodel_count,
    )?;

    let surfaces = s.plain_array(
        p,
        s.layout(84, 120),
        4,
        s.layout(sz::GFX_SURFACE, 32),
        surface_count,
    )?;
    if let Some(arr) = surfaces {
        for i in 0..surface_count {
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                arr.at(i * s.layout(sz::GFX_SURFACE, 32) + 16),
            )?;
        }
    }

    let surfaces_bounds = s.plain_array(
        p,
        s.layout(88, 128),
        4,
        sz::GFX_SURFACE_BOUNDS,
        surface_count,
    )?;

    let smodel_draw_insts = s.plain_array(
        p,
        s.layout(92, 136),
        4,
        s.layout(sz::GFX_STATIC_MODEL_DRAW_INST, 88),
        smodel_count,
    )?;
    if let Some(insts) = smodel_draw_insts {
        for i in 0..smodel_count {
            let inst = insts.at(i * s.layout(sz::GFX_STATIC_MODEL_DRAW_INST, 88));
            asset_ptr_at(s, links, AssetType::XModel, inst.at(s.layout(0x34, 56)))?;
        }
    }

    runtime_array(s, p, s.layout(96, 144), 8, 8, surface_count)?;
    let sun_shadow_count = s.u32_at(p, 48)? as usize;
    runtime_array(s, p, s.layout(100, 152), 128, 4, sun_shadow_count)?;

    Ok(DpvsStaticLoad {
        surfaces,
        smodel_count,
        static_surface_count,
        static_surface_count_no_decal,
        lit_opaque_surfs_begin,
        lit_opaque_surfs_end,
        lit_trans_surfs_begin,
        lit_trans_surfs_end,
        shadow_caster_surfs_begin,
        shadow_caster_surfs_end,
        emissive_surfs_begin,
        emissive_surfs_end,
        sorted_surf_index,
        smodel_insts,
        surfaces_bounds,
        smodel_draw_insts,
    })
}

fn load_gfx_dpvs_dynamic(s: &mut ZoneStream<'_>, p: Ptr, cell_count: usize) -> Result<()> {
    let word_count = [s.u32_at(p, 0)? as usize, s.u32_at(p, 4)? as usize];
    for ty in 0..2 {
        runtime_array(
            s,
            p,
            16 + ty * s.pointer_bytes(),
            4,
            4,
            word_count[ty] * cell_count,
        )?;
    }
    for ty in 0..2 {
        for vis in 0..3 {
            runtime_array(
                s,
                p,
                s.layout(24, 32) + (ty * 3 + vis) * s.pointer_bytes(),
                16,
                1,
                32 * word_count[ty],
            )?;
        }
    }
    Ok(())
}

fn sun_material_ref(s: &ZoneStream<'_>) -> (Option<Ptr>, [u8; 32], u8) {
    let Some(material) = s.latest_material() else {
        return (None, [0; 32], 0);
    };
    let mut name = [0u8; 32];
    let len = material
        .name
        .and_then(|ptr| s.cstr(ptr).ok())
        .map(|text| {
            let bytes = text.as_bytes();
            let len = bytes.len().min(32);
            name[..len].copy_from_slice(&bytes[..len]);
            len as u8
        })
        .unwrap_or(0);
    (material.header, name, len)
}

fn runtime_array(
    s: &mut ZoneStream<'_>,
    p: Ptr,
    field: usize,
    align: usize,
    elem: usize,
    count: usize,
) -> Result<()> {
    if s.begin_body(p.at(field))? {
        s.push(XFILE_BLOCK_RUNTIME)?;
        s.tag_alloc(field as u32, align, elem * count);
        s.alloc_load(align, elem * count)?;
        s.pop()?;
    }
    Ok(())
}
