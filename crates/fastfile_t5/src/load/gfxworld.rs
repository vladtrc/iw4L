use super::{AssetLinkSink, always_array, asset_ptr_at, follow_name, runtime_array};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{
    GfxLightGridGeometry, GfxWorldGeometry, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZoneStream,
};

pub(super) fn load_gfxworld(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::GFX_WORLD)?;

    let plane_count = s.i32_at(p, 8)?.max(0) as usize;
    let node_count = s.i32_at(p, 12)?.max(0) as usize;
    let surface_count = s.i32_at(p, 16)?.max(0) as usize;
    let sky_surf_count = s.i32_at(p, sz::GFX_WORLD_SKY_SURF_COUNT_OFF)?.max(0) as usize;
    let sun_primary_light_index = s.u32_at(p, sz::GFX_WORLD_SUN_PRIMARY_LIGHT_INDEX_OFF)? as usize;
    let primary_light_count = s.u32_at(p, sz::GFX_WORLD_PRIMARY_LIGHT_COUNT_OFF)? as usize;
    let cull_group_count = s.i32_at(p, sz::GFX_WORLD_CULL_GROUP_COUNT_OFF)?.max(0) as usize;
    let corona_count = s.u32_at(p, sz::GFX_WORLD_CORONA_COUNT_OFF)? as usize;
    let shadow_map_volume_count = s.u32_at(p, sz::GFX_WORLD_SHADOW_MAP_VOLUME_COUNT_OFF)? as usize;
    let shadow_map_volume_plane_count =
        s.u32_at(p, sz::GFX_WORLD_SHADOW_MAP_VOLUME_PLANE_COUNT_OFF)? as usize;
    let exposure_volume_count = s.u32_at(p, sz::GFX_WORLD_EXPOSURE_VOLUME_COUNT_OFF)? as usize;
    let exposure_volume_plane_count =
        s.u32_at(p, sz::GFX_WORLD_EXPOSURE_VOLUME_PLANE_COUNT_OFF)? as usize;
    let model_count = s.i32_at(p, sz::GFX_WORLD_MODEL_COUNT_OFF)?.max(0) as usize;
    let material_memory_count =
        s.i32_at(p, sz::GFX_WORLD_MATERIAL_MEMORY_COUNT_OFF)?.max(0) as usize;
    let lod_chain_count = s.u32_at(p, sz::GFX_WORLD_LOD_CHAIN_COUNT_OFF)? as usize;
    let lod_info_count = s.u32_at(p, sz::GFX_WORLD_LOD_INFO_COUNT_OFF)? as usize;
    let lod_surface_count = s.u32_at(p, sz::GFX_WORLD_LOD_SURFACE_COUNT_OFF)? as usize;
    let num_occluders = s.u32_at(p, sz::GFX_WORLD_NUM_OCCLUDERS_OFF)? as usize;
    let num_outdoor_bounds = s.u32_at(p, sz::GFX_WORLD_NUM_OUTDOOR_BOUNDS_OFF)? as usize;
    let hero_light_count = s.u32_at(p, sz::GFX_WORLD_HERO_LIGHT_COUNT_OFF)? as usize;
    let hero_light_tree_count = s.u32_at(p, sz::GFX_WORLD_HERO_LIGHT_TREE_COUNT_OFF)? as usize;
    let sun_parse_exposure_bits = s
        .f32_at(p, sz::GFX_WORLD_SUN_PARSE_EXPOSURE_OFF)
        .ok()
        .map(f32::to_bits);
    let sun_parse_tree_scatter_intensity_bits = s
        .f32_at(p, sz::GFX_WORLD_SUN_PARSE_TREE_SCATTER_INTENSITY_OFF)
        .ok()
        .map(f32::to_bits);
    let sun_parse_tree_scatter_amount_bits = s
        .f32_at(p, sz::GFX_WORLD_SUN_PARSE_TREE_SCATTER_AMOUNT_OFF)
        .ok()
        .map(f32::to_bits);

    let dpvs_dyn = p.at(sz::GFX_WORLD_DPVS_DYN_OFF);
    let dyn_model_count = s.u32_at(dpvs_dyn, 8)? as usize;
    let dyn_brush_count = s.u32_at(dpvs_dyn, 12)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    follow_name(s, p, 4)?;

    load_stream_info(s, p.at(sz::GFX_WORLD_STREAM_INFO_OFF))?;

    let sky_start_surfs = always_array(
        s,
        p.at(sz::GFX_WORLD_SKY_START_SURFS_OFF),
        4,
        4 * sky_surf_count,
    )?;

    let first_sky_image = Some(p.at(sz::GFX_WORLD_SKY_IMAGE_OFF));
    asset_ptr_at(
        s,
        links,
        AssetType::Image,
        p.at(sz::GFX_WORLD_SKY_IMAGE_OFF),
    )?;
    follow_name(s, p, sz::GFX_WORLD_SKY_BOX_MODEL_OFF)?;

    let sky_box_model = match s.ptr_at(p, sz::GFX_WORLD_SKY_BOX_MODEL_OFF)? {
        crate::ZonePtr::Offset(ptr) => Some(ptr),
        _ => None,
    };

    if s.begin_body(p.at(sz::GFX_WORLD_SUN_LIGHT_OFF))? {
        let light = s.alloc_load(16, sz::GFX_LIGHT)?;
        s.fixup_slot(p.at(sz::GFX_WORLD_SUN_LIGHT_OFF), light)?;
        asset_ptr_at(
            s,
            links,
            AssetType::LightDef,
            light.at(sz::GFX_LIGHT_DEF_OFF),
        )?;
    }

    let sun_light = match s.ptr_at(p, sz::GFX_WORLD_SUN_LIGHT_OFF)? {
        crate::ZonePtr::Offset(ptr) => Some(ptr),
        _ => None,
    };
    always_array(
        s,
        p.at(sz::GFX_WORLD_CORONAS_OFF),
        4,
        sz::GFX_LIGHT_CORONA * corona_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_SHADOW_MAP_VOLUMES_OFF),
        4,
        sz::GFX_SHADOW_MAP_VOLUME * shadow_map_volume_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_SHADOW_MAP_VOLUME_PLANES_OFF),
        4,
        sz::GFX_VOLUME_PLANE * shadow_map_volume_plane_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_EXPOSURE_VOLUMES_OFF),
        4,
        sz::GFX_EXPOSURE_VOLUME * exposure_volume_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_EXPOSURE_VOLUME_PLANES_OFF),
        4,
        sz::GFX_VOLUME_PLANE * exposure_volume_plane_count,
    )?;

    let dpvs_planes = p.at(sz::GFX_WORLD_DPVS_PLANES_OFF);
    let cell_count = load_dpvs_planes(s, dpvs_planes, plane_count, node_count)?;

    let planes = match s.ptr_at(dpvs_planes, 4)? {
        crate::zone::ZonePtr::Offset(q) => Some(q),
        _ => None,
    };
    let nodes = match s.ptr_at(dpvs_planes, 8)? {
        crate::zone::ZonePtr::Offset(q) => Some(q),
        _ => None,
    };

    let cells = if let Some(cells) = always_array(
        s,
        p.at(sz::GFX_WORLD_CELLS_OFF),
        4,
        sz::GFX_CELL * cell_count,
    )? {
        for i in 0..cell_count {
            load_cell(s, cells.at(i * sz::GFX_CELL))?;
        }
        Some(cells)
    } else {
        None
    };

    let (
        vertices,
        indices,
        vertex_count,
        index_count,
        lightmap_count,
        lightmaps,
        reflection_probes,
        reflection_probe_count,
        vertex_layer,
        vertex_layer_size,
    ) = load_draw(s, links, p.at(sz::GFX_WORLD_DRAW_OFF))?;
    let light_grid = load_light_grid(s, p.at(sz::GFX_WORLD_LIGHT_GRID_OFF))?;

    always_array(
        s,
        p.at(sz::GFX_WORLD_MODELS_OFF),
        4,
        sz::GFX_BRUSH_MODEL * model_count,
    )?;

    if let Some(memories) = always_array(
        s,
        p.at(sz::GFX_WORLD_MATERIAL_MEMORY_OFF),
        4,
        sz::MATERIAL_MEMORY * material_memory_count,
    )? {
        for i in 0..material_memory_count {
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                memories.at(i * sz::MATERIAL_MEMORY),
            )?;
        }
    }

    let sun = p.at(sz::GFX_WORLD_SUN_OFF);
    asset_ptr_at(
        s,
        links,
        AssetType::Material,
        sun.at(sz::SUNFLARE_SPRITE_MATERIAL_OFF),
    )?;
    asset_ptr_at(
        s,
        links,
        AssetType::Material,
        sun.at(sz::SUNFLARE_FLARE_MATERIAL_OFF),
    )?;

    asset_ptr_at(
        s,
        links,
        AssetType::Image,
        p.at(sz::GFX_WORLD_OUTDOOR_IMAGE_OFF),
    )?;

    let cell_bits = cell_count * cell_count.div_ceil(32);
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_CELL_CASTER_BITS_OFF),
        4,
        4 * cell_bits,
    )?;
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_SCENE_DYN_MODEL_OFF),
        4,
        sz::GFX_SCENE_DYN_MODEL * dyn_model_count,
    )?;
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_SCENE_DYN_BRUSH_OFF),
        4,
        sz::GFX_SCENE_DYN_BRUSH * dyn_brush_count,
    )?;

    let non_sun = primary_light_count.saturating_sub(sun_primary_light_index.saturating_add(1));
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_PRIMARY_LIGHT_ENTITY_SHADOW_VIS_OFF),
        4,
        4 * (non_sun << 13),
    )?;
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_PRIMARY_LIGHT_DYN_ENT_SHADOW_VIS_OFF),
        4,
        4 * dyn_model_count * non_sun,
    )?;
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_PRIMARY_LIGHT_DYN_ENT_SHADOW_VIS_OFF + 4),
        4,
        4 * dyn_brush_count * non_sun,
    )?;
    runtime_array(
        s,
        p.at(sz::GFX_WORLD_NON_SUN_PRIMARY_LIGHT_FOR_MODEL_DYN_ENT_OFF),
        1,
        dyn_model_count,
    )?;

    if let Some(shadows) = always_array(
        s,
        p.at(sz::GFX_WORLD_SHADOW_GEOM_OFF),
        4,
        sz::GFX_SHADOW_GEOMETRY * primary_light_count,
    )? {
        for i in 0..primary_light_count {
            let shadow = shadows.at(i * sz::GFX_SHADOW_GEOMETRY);
            let surface_n = s.u16_at(shadow, 0)? as usize;
            let smodel_n = s.u16_at(shadow, 2)? as usize;
            always_array(s, shadow.at(4), 2, 2 * surface_n)?;
            always_array(s, shadow.at(8), 2, 2 * smodel_n)?;
        }
    }

    if let Some(regions) = always_array(
        s,
        p.at(sz::GFX_WORLD_LIGHT_REGION_OFF),
        4,
        sz::GFX_LIGHT_REGION * primary_light_count,
    )? {
        for i in 0..primary_light_count {
            load_light_region(s, regions.at(i * sz::GFX_LIGHT_REGION))?;
        }
    }

    let (
        surfaces,
        sorted_surf_index,
        smodel_insts,
        smodel_draw_insts,
        smodel_count,
        static_surface_count,
        lit_surfs_begin,
        lit_surfs_end,
        decal_surfs_begin,
        decal_surfs_end,
        emissive_surfs_begin,
        emissive_surfs_end,
    ) = load_dpvs_static(
        s,
        links,
        p.at(sz::GFX_WORLD_DPVS_OFF),
        surface_count,
        cull_group_count,
    )?;
    load_dpvs_dynamic(s, dpvs_dyn, cell_count)?;

    always_array(
        s,
        p.at(sz::GFX_WORLD_LOD_CHAINS_OFF),
        4,
        sz::GFX_WORLD_LOD_CHAIN * lod_chain_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_LOD_INFOS_OFF),
        4,
        sz::GFX_WORLD_LOD_INFO * lod_info_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_LOD_SURFACES_OFF),
        4,
        4 * lod_surface_count,
    )?;

    let water = p.at(sz::GFX_WORLD_WATER_BUFFERS_OFF);
    for i in 0..2 {
        let buf = water.at(i * sz::GFX_WATER_BUFFER);
        let buffer_size = s.u32_at(buf, 0)? as usize;
        always_array(s, buf.at(4), 4, buffer_size)?;
    }

    asset_ptr_at(
        s,
        links,
        AssetType::Material,
        p.at(sz::GFX_WORLD_WATER_MATERIAL_OFF),
    )?;
    asset_ptr_at(
        s,
        links,
        AssetType::Material,
        p.at(sz::GFX_WORLD_CORONA_MATERIAL_OFF),
    )?;
    asset_ptr_at(
        s,
        links,
        AssetType::Material,
        p.at(sz::GFX_WORLD_ROPE_MATERIAL_OFF),
    )?;

    always_array(
        s,
        p.at(sz::GFX_WORLD_OCCLUDERS_OFF),
        4,
        sz::OCCLUDER * num_occluders,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_OUTDOOR_BOUNDS_OFF),
        4,
        sz::GFX_OUTDOOR_BOUNDS * num_outdoor_bounds,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_HERO_LIGHTS_OFF),
        4,
        sz::GFX_HERO_LIGHT * hero_light_count,
    )?;
    always_array(
        s,
        p.at(sz::GFX_WORLD_HERO_LIGHT_TREE_OFF),
        4,
        sz::GFX_HERO_LIGHT_TREE * hero_light_tree_count,
    )?;

    s.record_gfx_world(GfxWorldGeometry {
        vertices,
        vertex_count,
        vertex_layer,
        vertex_layer_size,
        indices,
        index_count,
        surfaces,
        surface_count,
        lightmap_count,
        lightmaps,
        first_sky_image,
        sky_surf_count,
        sky_start_surfs,
        sky_box_model,
        sun_light,
        reflection_probes,
        reflection_probe_count,
        sun_primary_light_index,
        sun_parse_exposure_bits,

        sky_dynamic_intensity_bits: Some([
            s.u32_at(p, 0x130)?,
            s.u32_at(p, 0x134)?,
            s.u32_at(p, 0x138)?,
            s.u32_at(p, 0x13c)?,
        ]),
        sun_parse_tree_scatter_intensity_bits,
        sun_parse_tree_scatter_amount_bits,
        exposure_volume_count,
        light_grid,
        cell_count,
        plane_count,
        node_count,
        smodel_count,
        static_surface_count,
        lit_surfs_begin,
        lit_surfs_end,
        decal_surfs_begin,
        decal_surfs_end,
        emissive_surfs_begin,
        emissive_surfs_end,
        planes,
        nodes,
        cells,
        sorted_surf_index,
        smodel_insts,
        smodel_draw_insts,
        dpvs_static: Some(p.at(sz::GFX_WORLD_DPVS_OFF)),
    });

    s.pop()
}

fn load_stream_info(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let aabb_tree_count = s.i32_at(p, 0)?.max(0) as usize;
    let leaf_ref_count = s.i32_at(p, 8)?.max(0) as usize;
    always_array(s, p.at(4), 4, sz::GFX_STREAMING_AABB_TREE * aabb_tree_count)?;
    always_array(s, p.at(12), 4, 4 * leaf_ref_count)?;
    Ok(())
}

fn load_dpvs_planes(
    s: &mut ZoneStream<'_>,
    p: Ptr,
    plane_count: usize,
    node_count: usize,
) -> Result<usize> {
    let cell_count = s.i32_at(p, 0)?.max(0) as usize;

    if s.begin_body(p.at(4))? {
        let planes = s.alloc_load(4, sz::CPLANE * plane_count)?;
        s.fixup_slot(p.at(4), planes)?;
    }
    always_array(s, p.at(8), 2, 2 * node_count)?;

    runtime_array(s, p.at(12), 4, 4 * (cell_count << 9))?;
    Ok(cell_count)
}

fn load_cell(s: &mut ZoneStream<'_>, cell: Ptr) -> Result<()> {
    let aabb_tree_count = s.i32_at(cell, 0x18)?.max(0) as usize;
    let portal_count = s.i32_at(cell, 0x20)?.max(0) as usize;
    let cull_group_count = s.i32_at(cell, 0x28)?.max(0) as usize;
    let reflection_probe_count = s.u8_at(cell, 0x30)? as usize;

    if let Some(trees) = always_array(s, cell.at(0x1c), 4, sz::GFX_AABB_TREE * aabb_tree_count)? {
        for i in 0..aabb_tree_count {
            load_aabb_tree(s, trees.at(i * sz::GFX_AABB_TREE))?;
        }
    }

    if let Some(portals) = always_array(s, cell.at(0x24), 4, sz::GFX_PORTAL * portal_count)? {
        for i in 0..portal_count {
            load_portal(s, portals.at(i * sz::GFX_PORTAL))?;
        }
    }

    always_array(s, cell.at(0x2c), 4, 4 * cull_group_count)?;
    always_array(s, cell.at(0x34), 1, reflection_probe_count)?;
    Ok(())
}

fn load_aabb_tree(s: &mut ZoneStream<'_>, aabb: Ptr) -> Result<()> {
    let smodel_index_count = s.u16_at(aabb, sz::GFX_AABB_TREE_SMODEL_INDEX_COUNT_OFF)? as usize;

    if s.begin_body(aabb.at(sz::GFX_AABB_TREE_SMODEL_INDEXES_OFF))? {
        let idx = s.alloc_load(2, 2 * smodel_index_count)?;
        s.fixup_slot(aabb.at(sz::GFX_AABB_TREE_SMODEL_INDEXES_OFF), idx)?;
    }
    Ok(())
}

fn load_portal(s: &mut ZoneStream<'_>, portal: Ptr) -> Result<()> {
    if s.begin_body(portal.at(sz::GFX_PORTAL_CELL_OFF))? {
        let cell = s.alloc_load(4, sz::GFX_CELL)?;
        s.fixup_slot(portal.at(sz::GFX_PORTAL_CELL_OFF), cell)?;
        load_cell(s, cell)?;
    }
    let vertex_count = s.u8_at(portal, sz::GFX_PORTAL_VERTEX_COUNT_OFF)? as usize;
    always_array(
        s,
        portal.at(sz::GFX_PORTAL_VERTICES_OFF),
        4,
        12 * vertex_count,
    )?;
    Ok(())
}

fn load_draw(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<(
    Option<Ptr>,
    Option<Ptr>,
    usize,
    usize,
    usize,
    [crate::GfxLightmapImages; crate::MAX_LIGHTMAP_PAGES],
    Option<Ptr>,
    usize,
    Option<Ptr>,
    usize,
)> {
    let probe_count = s.u32_at(p, 0)? as usize;
    let lightmap_count = s.i32_at(p, 0x0c)?.max(0) as usize;
    let vertex_count = s.u32_at(p, 0x9c)? as usize;
    let vertex_layer_data_size = s.u32_at(p, 0xa8)? as usize;
    let index_count = s.i32_at(p, 0xb8)?.max(0) as usize;

    let reflection_probes = if let Some(probes) =
        always_array(s, p.at(4), 4, sz::GFX_REFLECTION_PROBE * probe_count)?
    {
        for i in 0..probe_count {
            load_reflection_probe(s, links, probes.at(i * sz::GFX_REFLECTION_PROBE))?;
        }
        Some(probes)
    } else {
        None
    };
    runtime_array(s, p.at(8), 4, 4 * probe_count)?;

    let mut lightmaps = [crate::GfxLightmapImages::default(); crate::MAX_LIGHTMAP_PAGES];
    if let Some(rows) = always_array(s, p.at(0x10), 4, sz::GFX_LIGHTMAP_ARRAY * lightmap_count)? {
        for i in 0..lightmap_count {
            let lm = rows.at(i * sz::GFX_LIGHTMAP_ARRAY);
            let serial = s.image_serial();
            asset_ptr_at(s, links, AssetType::Image, lm.at(0))?;
            if s.image_serial() != serial
                && let Some(page) = lightmaps.get_mut(i)
            {
                page.primary = s.latest_image();
            }
            let serial = s.image_serial();
            asset_ptr_at(s, links, AssetType::Image, lm.at(4))?;
            if s.image_serial() != serial
                && let Some(page) = lightmaps.get_mut(i)
            {
                page.secondary = s.latest_image();
            }
            let serial = s.image_serial();
            asset_ptr_at(s, links, AssetType::Image, lm.at(8))?;
            if s.image_serial() != serial
                && let Some(page) = lightmaps.get_mut(i)
            {
                page.secondary_b = s.latest_image();
            }
        }
    }
    runtime_array(s, p.at(0x14), 4, 4 * lightmap_count)?;
    runtime_array(s, p.at(0x18), 4, 4 * lightmap_count)?;
    runtime_array(s, p.at(0x1c), 4, 4 * lightmap_count)?;

    for i in 0..31 {
        asset_ptr_at(s, links, AssetType::Image, p.at(0x20 + i * 4))?;
    }

    let vd = p.at(0xa0);
    let vertices = always_array(s, vd.at(0), 4, sz::GFX_WORLD_VERTEX * vertex_count)?;
    let vld = p.at(0xac);
    let vertex_layer = always_array(s, vld.at(0), 1, vertex_layer_data_size)?;

    let indices = always_array(s, p.at(0xbc), 2, 2 * index_count)?;
    Ok((
        vertices,
        indices,
        vertex_count,
        index_count,
        lightmap_count,
        lightmaps,
        reflection_probes,
        probe_count,
        vertex_layer,
        vertex_layer_data_size,
    ))
}

fn load_reflection_probe(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
) -> Result<()> {
    asset_ptr_at(
        s,
        links,
        AssetType::Image,
        p.at(sz::GFX_REFLECTION_PROBE_IMAGE_OFF),
    )?;
    let volume_count = s.u32_at(p, sz::GFX_REFLECTION_PROBE_VOLUME_COUNT_OFF)? as usize;
    always_array(
        s,
        p.at(sz::GFX_REFLECTION_PROBE_VOLUMES_OFF),
        4,
        sz::GFX_REFLECTION_PROBE_VOLUME_DATA * volume_count,
    )?;
    Ok(())
}

fn load_light_grid(s: &mut ZoneStream<'_>, p: Ptr) -> Result<GfxLightGridGeometry> {
    let has_light_regions = s.u8_at(p, 0)? != 0;
    let sun_primary_light_index = s.u32_at(p, 4)?;
    let mins = [s.u16_at(p, 8)?, s.u16_at(p, 10)?, s.u16_at(p, 12)?];
    let maxs = [s.u16_at(p, 14)?, s.u16_at(p, 16)?, s.u16_at(p, 18)?];
    let row_axis = s.u32_at(p, 0x14)?;
    let col_axis = s.u32_at(p, 0x18)?;
    let rows = match (mins.get(row_axis as usize), maxs.get(row_axis as usize)) {
        (Some(&lo), Some(&hi)) => usize::from(hi).saturating_sub(usize::from(lo)) + 1,
        _ => 0,
    };
    let row_data_start = always_array(s, p.at(0x1c), 2, 2 * rows)?;

    let raw_size = s.u32_at(p, 0x20)? as usize;
    let raw_row_data = always_array(s, p.at(0x24), 1, raw_size)?;

    let entry_count = s.u32_at(p, 0x28)? as usize;
    let entries = always_array(s, p.at(0x2c), 4, 4 * entry_count)?;

    let color_count = s.u32_at(p, 0x30)? as usize;
    let colors = always_array(s, p.at(0x34), 4, sz::GFX_LIGHT_GRID_COLORS * color_count)?;
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

fn load_light_region(s: &mut ZoneStream<'_>, region: Ptr) -> Result<()> {
    let hull_count = s.u32_at(region, 0)? as usize;
    if let Some(hulls) = always_array(s, region.at(4), 4, sz::GFX_LIGHT_REGION_HULL * hull_count)? {
        for i in 0..hull_count {
            let hull = hulls.at(i * sz::GFX_LIGHT_REGION_HULL);
            let axis_count = s.u32_at(hull, 72)? as usize;
            always_array(s, hull.at(76), 4, sz::GFX_LIGHT_REGION_AXIS * axis_count)?;
        }
    }
    Ok(())
}

fn load_dpvs_static(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    surface_count: usize,
    cull_group_count: usize,
) -> Result<(
    Option<Ptr>,
    Option<Ptr>,
    Option<Ptr>,
    Option<Ptr>,
    usize,
    usize,
    u32,
    u32,
    u32,
    u32,
    u32,
    u32,
)> {
    let smodel_count = s.u32_at(p, 0)? as usize;
    let static_surface_count = s.u32_at(p, 8)? as usize;
    let lit_surfs_begin = s.u32_at(p, 0x0c)?;
    let lit_surfs_end = s.u32_at(p, 0x10)?;
    let decal_surfs_begin = s.u32_at(p, 0x14)?;
    let decal_surfs_end = s.u32_at(p, 0x18)?;
    let emissive_surfs_begin = s.u32_at(p, 0x1c)?;
    let emissive_surfs_end = s.u32_at(p, 0x20)?;
    let smodel_vis_data_count = s.u32_at(p, 0x24)? as usize;
    let surface_vis_data_count = s.u32_at(p, 0x28)? as usize;

    for field in [0x2cusize, 0x30, 0x34] {
        runtime_array(s, p.at(field), 1, smodel_count)?;
    }
    for field in [0x38usize, 0x3c, 0x40] {
        runtime_array(s, p.at(field), 1, static_surface_count)?;
    }
    runtime_array(s, p.at(0x44), 1, smodel_count)?;
    runtime_array(s, p.at(0x48), 1, static_surface_count)?;

    runtime_array(s, p.at(0x4c), 128, 4 * 2 * smodel_vis_data_count)?;

    let sorted = always_array(s, p.at(0x50), 2, 2 * static_surface_count)?;
    let smodel_insts = always_array(s, p.at(0x54), 4, sz::GFX_STATIC_MODEL_INST * smodel_count)?;

    let mut surfaces = None;
    if let Some(surfs) = always_array(s, p.at(0x58), 16, sz::GFX_SURFACE * surface_count)? {
        surfaces = Some(surfs);
        for i in 0..surface_count {
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                surfs.at(i * sz::GFX_SURFACE + sz::GFX_SURFACE_MATERIAL_OFF),
            )?;
        }
    }

    always_array(s, p.at(0x5c), 4, sz::GFX_CULL_GROUP * cull_group_count)?;

    let mut smodel_draw_insts = None;
    if let Some(insts) = always_array(
        s,
        p.at(0x60),
        4,
        sz::GFX_STATIC_MODEL_DRAW_INST * smodel_count,
    )? {
        smodel_draw_insts = Some(insts);
        for i in 0..smodel_count {
            asset_ptr_at(
                s,
                links,
                AssetType::XModel,
                insts
                    .at(i * sz::GFX_STATIC_MODEL_DRAW_INST
                        + sz::GFX_STATIC_MODEL_DRAW_INST_MODEL_OFF),
            )?;
        }
    }

    runtime_array(s, p.at(0x64), 4, sz::GFX_DRAW_SURF * static_surface_count)?;
    runtime_array(s, p.at(0x68), 128, 4 * surface_vis_data_count)?;
    Ok((
        surfaces,
        sorted,
        smodel_insts,
        smodel_draw_insts,
        smodel_count,
        static_surface_count,
        lit_surfs_begin,
        lit_surfs_end,
        decal_surfs_begin,
        decal_surfs_end,
        emissive_surfs_begin,
        emissive_surfs_end,
    ))
}

fn load_dpvs_dynamic(s: &mut ZoneStream<'_>, p: Ptr, cell_count: usize) -> Result<()> {
    for ty in 0..2 {
        let words = s.u32_at(p, ty * 4)? as usize;
        runtime_array(s, p.at(0x10 + ty * 4), 4, 4 * words * cell_count)?;
        for vis in 0..3 {
            runtime_array(s, p.at(0x18 + (ty * 3 + vis) * 4), 16, 32 * words)?;
        }
    }
    Ok(())
}
