use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{
    GfxLightGridGeometry, GfxLightmapPair, GfxWorldGeometry, MAX_LIGHTMAP_PAGES, Ptr, Result,
    XFILE_BLOCK_RUNTIME, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_gfxworld(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.walk_stage = "gfx_world";
    let width = s.pointer_bytes();
    let p = s.alloc_load(4, s.layout(sz::GFX_WORLD, 968))?;

    let plane_count = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;
    let node_count = s.i32_at(p, s.layout(12, 20))?.max(0) as usize;
    let surface_count = s.u32_at(p, s.layout(16, 24))? as usize;
    let sky_count = s.i32_at(p, s.layout(20, 28))?.max(0) as usize;
    let sun_primary_light_count = s.u32_at(p, s.layout(28, 40))? as usize;
    let primary_light_count = s.u32_at(p, s.layout(32, 44))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    s.follow_string(p, s.layout(4, 8))?;

    s.walk_stage = "gfx_world.skies";
    let mut first_sky_image = None;
    let sky = s.layout(sz::GFX_SKY, 32);
    let skies = s.plain_array(p, s.layout(24, 32), 4, sky, sky_count)?;
    if let Some(skies) = skies {
        for i in 0..sky_count {
            let row = skies.at(i * sky);
            let start_surfs = s.i32_at(row, 0)?.max(0) as usize;
            s.plain_array(row, s.layout(4, 8), 4, 4, start_surfs)?;
            let image = row.at(s.layout(8, 16));
            asset_ptr_at(s, links, AssetType::Image, image)?;
            if first_sky_image.is_none() {
                first_sky_image = Some(image);
            }
        }
    }

    s.walk_stage = "gfx_world.dpvs_planes";

    let dp = p.at(s.layout(0x34, 64));
    let cell_count = s.i32_at(dp, 0)?.max(0) as usize;
    let planes = s.plain_array(dp, s.layout(4, 8), 4, sz::CPLANE, plane_count)?;
    let nodes = s.plain_array(dp, s.layout(8, 16), 2, 2, node_count)?;
    runtime_array(s, dp, s.layout(12, 24), 4, 4, cell_count * 512)?;

    s.walk_stage = "gfx_world.aabb_trees";
    let aabb_counts = s.plain_array(p, s.layout(0x44, 96), 4, 4, cell_count)?;
    let aabb_trees = s.plain_array(p, s.layout(0x48, 104), 128, width, cell_count)?;
    if let Some(trees) = aabb_trees {
        let aabb_tree = s.layout(sz::GFX_AABB_TREE, 56);
        for i in 0..cell_count {
            if s.begin_body(trees.at(i * width))? {
                let count = match aabb_counts {
                    Some(counts) => s.i32_at(counts, i * 4)?.max(0) as usize,
                    None => 0,
                };
                let aabbs = s.alloc_load(4, aabb_tree * count)?;
                s.fixup_slot(trees.at(i * width), aabbs)?;
                for j in 0..count {
                    let aabb = aabbs.at(j * aabb_tree);
                    let smodel_index_count = s.u16_at(aabb, 0x22)? as usize;
                    s.plain_array(aabb, s.layout(0x24, 40), 2, 2, smodel_index_count)?;
                }
            }
        }
    }

    s.walk_stage = "gfx_world.cells";
    let cell = s.layout(sz::GFX_CELL, 72);
    let cells = s.plain_array(p, s.layout(0x4c, 112), 4, cell, cell_count)?;
    if let Some(cells_ptr) = cells {
        let portal = s.layout(sz::GFX_PORTAL, 80);
        for i in 0..cell_count {
            let row = cells_ptr.at(i * cell);
            let cell_portals = s.i32_at(row, 0x18)?.max(0) as usize;
            if let Some(portals) =
                s.plain_array(row, s.layout(0x1c, 32), 4, portal, cell_portals)?
            {
                for j in 0..cell_portals {
                    let portal_row = portals.at(j * portal);
                    let vertex_count = s.u8_at(portal_row, s.layout(0x22, 50))? as usize;
                    s.plain_array(portal_row, s.layout(0x1c, 40), 4, 12, vertex_count)?;
                }
            }
            let reflection_probe_count = s.u8_at(row, s.layout(0x20, 40))? as usize;
            s.plain_array(row, s.layout(0x24, 48), 1, 1, reflection_probe_count)?;
            let reflection_probe_ref_count = s.u8_at(row, s.layout(0x28, 56))? as usize;
            s.plain_array(row, s.layout(0x2c, 64), 1, 1, reflection_probe_ref_count)?;
        }
    }

    s.walk_stage = "gfx_world.draw";
    let draw = load_gfxworld_draw(s, links, p.at(s.layout(0x50, 120)))?;
    s.walk_stage = "gfx_world.light_grid";
    let light_grid = load_gfx_light_grid(s, p.at(s.layout(sz::GFX_WORLD_LIGHT_GRID_OFF, 288)))?;

    s.walk_stage = "gfx_world.brush_models";
    let brush_model_count = s.i32_at(p, s.layout(0xdc, 376))?.max(0) as usize;
    s.plain_array(
        p,
        s.layout(0xe0, 384),
        4,
        sz::GFX_BRUSH_MODEL,
        brush_model_count,
    )?;

    let material_memory = s.layout(sz::MATERIAL_MEMORY, 16);
    let material_memory_count = s.i32_at(p, s.layout(0x100, 420))?.max(0) as usize;
    if let Some(memories) = s.plain_array(
        p,
        s.layout(0x104, 424),
        4,
        material_memory,
        material_memory_count,
    )? {
        for i in 0..material_memory_count {
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                memories.at(i * material_memory),
            )?;
        }
    }

    s.walk_stage = "gfx_world.sun";
    let sun = p.at(s.layout(0x108, 432));
    asset_ptr_at(s, links, AssetType::Material, sun.at(s.layout(4, 8)))?;
    asset_ptr_at(s, links, AssetType::Material, sun.at(s.layout(8, 16)))?;

    let mut outdoor_lookup = [0u32; 16];
    let lookup_off = s.layout(0x168, 544);
    for (i, word) in outdoor_lookup.iter_mut().enumerate() {
        *word = s.f32_at(p, lookup_off + i * 4)?.to_bits();
    }
    let outdoor_slot = p.at(s.layout(0x1a8, 608));
    asset_ptr_at(s, links, AssetType::Image, outdoor_slot)?;
    let outdoor_image = match s.ptr_at(outdoor_slot, 0)? {
        ZonePtr::Null => None,
        _ => Some(outdoor_slot),
    };

    s.walk_stage = "gfx_world.runtime";
    let dyn_dpvs = p.at(s.layout(0x240, 864));
    let dyn_model_count = s.u32_at(dyn_dpvs, 8)? as usize;
    let dyn_brush_count = s.u32_at(dyn_dpvs, 12)? as usize;

    runtime_array(
        s,
        p,
        s.layout(0x1ac, 616),
        4,
        4,
        cell_count * cell_count.div_ceil(32),
    )?;
    runtime_array(s, p, s.layout(0x1b0, 624), 4, 4, cell_count.div_ceil(32))?;
    runtime_array(s, p, s.layout(0x1b4, 632), 4, 6, dyn_model_count)?;
    runtime_array(s, p, s.layout(0x1b8, 640), 4, 4, dyn_brush_count)?;

    let non_sun_lights =
        primary_light_count.saturating_sub(sun_primary_light_count.saturating_add(1));
    runtime_array(s, p, s.layout(0x1bc, 648), 4, 4, non_sun_lights * 8192)?;
    runtime_array(
        s,
        p,
        s.layout(0x1c0, 656),
        4,
        4,
        dyn_model_count * non_sun_lights,
    )?;
    runtime_array(
        s,
        p,
        s.layout(0x1c4, 664),
        4,
        4,
        dyn_brush_count * non_sun_lights,
    )?;
    runtime_array(s, p, s.layout(0x1c8, 672), 1, 1, dyn_model_count)?;

    let shadow_geometry = s.layout(sz::GFX_SHADOW_GEOMETRY, 24);
    if let Some(shadows) = s.plain_array(
        p,
        s.layout(0x1cc, 680),
        4,
        shadow_geometry,
        primary_light_count,
    )? {
        for i in 0..primary_light_count {
            let shadow = shadows.at(i * shadow_geometry);
            let surface_n = s.u16_at(shadow, 0)? as usize;
            let smodel_n = s.u16_at(shadow, 2)? as usize;
            s.plain_array(shadow, s.layout(4, 8), 2, 2, surface_n)?;
            s.plain_array(shadow, s.layout(8, 16), 2, 2, smodel_n)?;
        }
    }

    s.walk_stage = "gfx_world.light_regions";
    let light_region = s.layout(sz::GFX_LIGHT_REGION, 16);
    let light_regions = s.plain_array(
        p,
        s.layout(0x1d0, 688),
        4,
        light_region,
        primary_light_count,
    )?;
    if let Some(regions) = light_regions {
        let hull_size = s.layout(sz::GFX_LIGHT_REGION_HULL, 88);
        for i in 0..primary_light_count {
            let region = regions.at(i * light_region);
            let count = s.u32_at(region, 0)? as usize;
            if let Some(hulls) = s.plain_array(region, s.layout(4, 8), 4, hull_size, count)? {
                for j in 0..count {
                    let hull = hulls.at(j * hull_size);
                    let axis_count = s.u32_at(hull, 0x48)? as usize;
                    s.plain_array(hull, s.layout(0x4c, 80), 4, 20, axis_count)?;
                }
            }
        }
    }

    s.walk_stage = "gfx_world.dpvs_static";
    let dpvs_static_off = s.layout(0x1d4, 696);
    let dpvs = load_gfx_dpvs_static(s, links, p.at(dpvs_static_off), surface_count)?;
    load_gfx_dpvs_dynamic(s, dyn_dpvs, cell_count)?;

    let hero_count = s.u32_at(p, s.layout(0x274, 948))? as usize;
    s.plain_array(
        p,
        s.layout(0x278, 952),
        4,
        sz::GFX_HERO_ONLY_LIGHT,
        hero_count,
    )?;

    s.pop()?;

    s.record_gfx_world(GfxWorldGeometry {
        vertices: draw.vertices,
        vertex_count: draw.vertex_count,
        vertex_layer: draw.vertex_layer,
        vertex_layer_size: draw.vertex_layer_size,
        indices: draw.indices,
        index_count: draw.index_count,
        surfaces: dpvs.surfaces,
        surface_count,
        lightmap_count: draw.lightmap_count,
        lightmaps: draw.lightmaps,
        first_lightmap_primary: draw.lightmaps[0].primary,
        first_lightmap_secondary: draw.lightmaps[0].secondary,
        first_sky_image,
        skies,
        sky_count,
        outdoor_image,
        outdoor_lookup,
        reflection_probes: draw.reflection_probes,
        reflection_probe_origins: draw.reflection_probe_origins,
        reflection_probe_count: draw.reflection_probe_count,
        light_grid,
        sun_primary_light_count,
        primary_light_count,
        light_regions,
        cell_count,
        plane_count,
        node_count,
        planes,
        nodes,
        cells,
        aabb_tree_counts: aabb_counts,
        aabb_trees,
        smodel_count: dpvs.smodel_count,
        static_surface_count: dpvs.static_surface_count,
        static_surface_count_no_decal: dpvs.static_surface_count_no_decal,
        lit_surfs_begin: dpvs.lit_opaque_surfs_begin,
        lit_surfs_end: dpvs.lit_opaque_surfs_end,
        lit_trans_surfs_begin: dpvs.lit_trans_surfs_begin,
        lit_trans_surfs_end: dpvs.lit_trans_surfs_end,
        emissive_surfs_begin: dpvs.emissive_surfs_begin,
        emissive_surfs_end: dpvs.emissive_surfs_end,
        sorted_surf_index: dpvs.sorted_surf_index,
        smodel_insts: dpvs.smodel_insts,
        surfaces_bounds: dpvs.surfaces_bounds,
        smodel_draw_insts: dpvs.smodel_draw_insts,
        dpvs_static: Some(p.at(dpvs_static_off)),
    });
    Ok(())
}

struct DrawLoad {
    vertices: Option<Ptr>,
    vertex_count: usize,
    vertex_layer: Option<Ptr>,
    vertex_layer_size: usize,
    indices: Option<Ptr>,
    index_count: usize,
    lightmap_count: usize,
    lightmaps: [GfxLightmapPair; MAX_LIGHTMAP_PAGES],
    reflection_probes: Option<Ptr>,
    reflection_probe_origins: Option<Ptr>,
    reflection_probe_count: usize,
}

fn load_gfxworld_draw(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<DrawLoad> {
    let width = s.pointer_bytes();
    let probe_count = s.i32_at(p, 0)?.max(0) as usize;
    let probe_ref_count = s.u32_at(p, s.layout(0x10, 32))? as usize;
    let lightmap_count = s.i32_at(p, s.layout(0x1c, 56))?.max(0) as usize;

    let mut reflection_probes = None;
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let a = s.alloc_load(4, width * probe_count)?;
        reflection_probes = Some(a);
        for i in 0..probe_count {
            asset_ptr_at(s, links, AssetType::Image, a.at(i * width))?;
        }
    }
    let reflection_probe_origins = s.plain_array(p, s.layout(8, 16), 4, 12, probe_count)?;
    runtime_array(s, p, s.layout(0xc, 24), 4, 4, probe_count)?;

    s.plain_array(p, s.layout(0x14, 40), 4, 12, probe_ref_count)?;
    s.plain_array(p, s.layout(0x18, 48), 1, 1, probe_ref_count)?;

    let mut lightmaps = [GfxLightmapPair::default(); MAX_LIGHTMAP_PAGES];
    let lightmap_array = s.layout(sz::GFX_LIGHTMAP_ARRAY, 16);
    if let Some(a) = s.plain_array(p, s.layout(0x20, 64), 4, lightmap_array, lightmap_count)? {
        let retain = lightmap_count.min(MAX_LIGHTMAP_PAGES);
        for i in 0..lightmap_count {
            let lightmap = a.at(i * lightmap_array);
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
    runtime_array(s, p, s.layout(0x24, 72), 4, 4, lightmap_count)?;
    runtime_array(s, p, s.layout(0x28, 80), 4, 4, lightmap_count)?;

    asset_ptr_at(s, links, AssetType::Image, p.at(s.layout(0x2c, 88)))?;
    asset_ptr_at(s, links, AssetType::Image, p.at(s.layout(0x30, 96)))?;

    let vertex_count = s.i32_at(p, s.layout(0x34, 104))?.max(0) as usize;

    let vertices = s.plain_array(
        p,
        s.layout(0x38, 112),
        4,
        sz::GFX_WORLD_VERTEX,
        vertex_count,
    )?;

    let vertex_layer_size = s.i32_at(p, s.layout(0x40, 128))?.max(0) as usize;
    let vertex_layer = s.plain_array(p, s.layout(0x44, 136), 1, 1, vertex_layer_size)?;

    let index_count = s.i32_at(p, s.layout(0x4c, 152))?.max(0) as usize;
    let indices = s.plain_array(p, s.layout(0x50, 160), 2, 2, index_count)?;

    Ok(DrawLoad {
        vertices,
        vertex_count,
        vertex_layer,
        vertex_layer_size,
        indices,
        index_count,
        lightmap_count,
        lightmaps,
        reflection_probes,
        reflection_probe_origins,
        reflection_probe_count: probe_count,
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
    let emissive_surfs_begin = s.u32_at(p, 36)?;
    let emissive_surfs_end = s.u32_at(p, 40)?;

    let width = s.pointer_bytes();
    let smodel_vis = s.layout(0x34, 56);
    for i in 0..3 {
        runtime_array(s, p, smodel_vis + i * width, 1, 1, smodel_count)?;
    }
    let surface_vis = s.layout(0x40, 80);
    for i in 0..3 {
        runtime_array(s, p, surface_vis + i * width, 1, 1, static_surface_count)?;
    }

    let sorted_surf_index = s.plain_array(
        p,
        s.layout(0x4c, 104),
        2,
        2,
        static_surface_count + static_surface_count_no_decal,
    )?;
    let smodel_insts = s.plain_array(
        p,
        s.layout(0x50, 112),
        4,
        sz::GFX_STATIC_MODEL_INST,
        smodel_count,
    )?;

    let surface = s.layout(sz::GFX_SURFACE, 32);
    let surfaces = s.plain_array(p, s.layout(0x54, 120), 4, surface, surface_count)?;
    if let Some(arr) = surfaces {
        for i in 0..surface_count {
            asset_ptr_at(s, links, AssetType::Material, arr.at(i * surface + 0x10))?;
        }
    }
    let surfaces_bounds = s.plain_array(
        p,
        s.layout(0x58, 128),
        4,
        sz::GFX_SURFACE_BOUNDS,
        surface_count,
    )?;

    let draw_inst = s.layout(sz::GFX_STATIC_MODEL_DRAW_INST, 88);
    let smodel_draw_insts = s.plain_array(p, s.layout(0x5c, 136), 4, draw_inst, smodel_count)?;
    if let Some(insts) = smodel_draw_insts {
        for i in 0..smodel_count {
            let inst = insts.at(i * draw_inst);
            asset_ptr_at(s, links, AssetType::XModel, inst.at(s.layout(0x34, 56)))?;
        }
    }

    runtime_array(s, p, s.layout(0x60, 144), 8, 8, surface_count)?;
    let sun_shadow_count = s.u32_at(p, 0x30)? as usize;
    runtime_array(s, p, s.layout(0x64, 152), 128, 4, sun_shadow_count)?;

    Ok(DpvsStaticLoad {
        surfaces,
        smodel_count,
        static_surface_count,
        static_surface_count_no_decal,
        lit_opaque_surfs_begin,
        lit_opaque_surfs_end,
        lit_trans_surfs_begin,
        lit_trans_surfs_end,
        emissive_surfs_begin,
        emissive_surfs_end,
        sorted_surf_index,
        smodel_insts,
        surfaces_bounds,
        smodel_draw_insts,
    })
}

fn load_gfx_dpvs_dynamic(s: &mut ZoneStream<'_>, p: Ptr, cell_count: usize) -> Result<()> {
    let width = s.pointer_bytes();
    let words0 = s.u32_at(p, 0)? as usize;
    let words1 = s.u32_at(p, 4)? as usize;
    let cell_bits = s.layout(0x10, 16);
    runtime_array(s, p, cell_bits, 4, 4, words0 * cell_count)?;
    runtime_array(s, p, cell_bits + width, 4, 4, words1 * cell_count)?;

    let vis = s.layout(0x18, 32);
    for (index, words) in [
        (0, words0),
        (3, words1),
        (1, words0),
        (4, words1),
        (2, words0),
        (5, words1),
    ] {
        runtime_array(s, p, vis + index * width, 16, 1, 32 * words)?;
    }
    Ok(())
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
        s.alloc_load(align, elem.saturating_mul(count))?;
        s.pop()?;
    }
    Ok(())
}
