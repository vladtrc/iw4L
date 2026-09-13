use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{
    ClipMapGeometry, Ptr, Result, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_TEMP, XFILE_BLOCK_VIRTUAL,
    ZonePtr, ZoneStream,
};

pub(super) fn load_clipmap(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.walk_stage = "clip_map";
    let width = s.pointer_bytes();

    let p = s.alloc_load(4, s.layout(sz::CLIP_MAP, 512))?;

    let info_off = s.layout(8, 16);
    let plane_count = s.i32_at(p, info_off)?.max(0) as usize;
    let material_count = s.u32_at(p, info_off + s.layout(8, 16))? as usize;
    let brush_side_count = s.u32_at(p, info_off + s.layout(16, 32))? as usize;
    let leafbrush_node_count = s.u32_at(p, info_off + s.layout(32, 64))? as usize;
    let brush_count = s.u16_at(p, info_off + s.layout(48, 96))? as usize;
    let static_model_count = s.u32_at(p, s.layout(0x4c, 152))? as usize;
    let node_count = s.u32_at(p, s.layout(0x54, 168))? as usize;
    let leaf_count = s.u32_at(p, s.layout(0x5c, 184))? as usize;
    let vert_count = s.u32_at(p, s.layout(0x64, 200))? as usize;
    let tri_count = s.i32_at(p, s.layout(0x6c, 216))?.max(0) as usize;
    let cmodel_count = s.u32_at(p, s.layout(0x90, 288))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    let name = s.follow_string(p, 0)?;

    let info = load_clip_info(s, p.at(info_off))?;
    load_clip_info_ptr(s, p.at(s.layout(0x48, 144)))?;

    s.walk_stage = "clip_map.static_models";
    let static_model = s.layout(sz::C_STATIC_MODEL, 80);
    let static_models = if let Some(models) =
        s.plain_array(p, s.layout(0x50, 160), 4, static_model, static_model_count)?
    {
        for i in 0..static_model_count {
            asset_ptr_at(s, links, AssetType::XModel, models.at(i * static_model))?;
        }
        Some(models)
    } else {
        None
    };

    s.walk_stage = "clip_map.nodes";
    let c_node = s.layout(sz::C_NODE, 16);
    let nodes = if let Some(nodes) = s.plain_array(p, s.layout(0x58, 176), 4, c_node, node_count)? {
        for i in 0..node_count {
            s.plain_array(nodes.at(i * c_node), 0, 4, sz::CPLANE, 1)?;
        }
        Some(nodes)
    } else {
        None
    };

    s.walk_stage = "clip_map.geometry";
    let leaves = s.plain_array(p, s.layout(0x60, 192), 4, sz::C_LEAF, leaf_count)?;
    let verts = s.plain_array(p, s.layout(0x68, 208), 4, 12, vert_count)?;
    let index_count = 3 * tri_count;
    let tri_indices = s.plain_array(p, s.layout(0x70, 224), 2, 2, index_count)?;
    s.plain_array(p, s.layout(0x74, 232), 1, 1, index_count.div_ceil(32) * 4)?;
    let border_count = s.i32_at(p, s.layout(0x78, 240))?.max(0) as usize;
    s.plain_array(
        p,
        s.layout(0x7c, 248),
        4,
        sz::COLLISION_BORDER,
        border_count,
    )?;

    let partition = s.layout(sz::COLLISION_PARTITION, 16);
    let partition_count = s.i32_at(p, s.layout(0x80, 256))?.max(0) as usize;
    let collision_partitions = if let Some(parts) =
        s.plain_array(p, s.layout(0x84, 264), 4, partition, partition_count)?
    {
        for i in 0..partition_count {
            s.plain_array(parts.at(i * partition), 8, 4, sz::COLLISION_BORDER, 1)?;
        }
        Some(parts)
    } else {
        None
    };

    let aabb_tree_count = s.i32_at(p, s.layout(0x88, 272))?.max(0) as usize;
    let collision_aabb_trees = s.plain_array(
        p,
        s.layout(0x8c, 280),
        16,
        sz::COLLISION_AABB_TREE,
        aabb_tree_count,
    )?;

    s.walk_stage = "clip_map.cmodels";
    let cmodel = s.layout(sz::CMODEL, 80);
    let cmodels =
        if let Some(models) = s.plain_array(p, s.layout(0x94, 296), 4, cmodel, cmodel_count)? {
            for i in 0..cmodel_count {
                load_clip_info_ptr(s, models.at(i * cmodel + s.layout(0x1c, 32)))?;
            }
            Some(models)
        } else {
            None
        };

    s.walk_stage = "clip_map.smodel_nodes";
    let smodel_node_count = s.u16_at(p, s.layout(0xbc, 376))? as usize;
    s.plain_array(
        p,
        s.layout(0xc0, 384),
        4,
        sz::SMODEL_AABB_NODE,
        smodel_node_count,
    )?;
    asset_ptr_at(s, links, AssetType::MapEnts, p.at(s.layout(0x98, 304)))?;

    s.walk_stage = "clip_map.stages";
    let stage = s.layout(sz::STAGE, 24);
    let stage_count = s.u8_at(p, s.layout(0xa0, 320))? as usize;
    if let Some(stages) = s.plain_array(p, s.layout(0x9c, 312), 4, stage, stage_count)? {
        for i in 0..stage_count {
            follow_name(s, stages.at(i * stage), 0)?;
        }
    }
    load_map_triggers(s, p.at(s.layout(0xa4, 328)))?;

    s.walk_stage = "clip_map.dyn_entities";
    let dyn_count = s.layout(0xc4, 392);
    for ty in 0..2 {
        load_dyn_entity_defs(s, links, p, ty)?;
    }
    for (base, elem) in [
        (s.layout(0xd0, 416), sz::DYN_ENTITY_POSE),
        (s.layout(0xd8, 432), s.layout(sz::DYN_ENTITY_CLIENT, 24)),
        (s.layout(0xe0, 448), sz::DYN_ENTITY_COLL),
    ] {
        for ty in 0..2 {
            let count = s.u16_at(p, dyn_count + ty * 2)? as usize;
            runtime_array(s, p, base + ty * width, 4, elem, count)?;
        }
    }

    s.record_clip_map(ClipMapGeometry {
        name,
        plane_count,
        material_count: info.material_count,
        brush_count,
        cmodel_count,
        static_model_count,
        leaf_count,
        node_count,
        vert_count,
        tri_count,
        leafbrush_count: info.leafbrush_count,
        partition_count,
        aabb_tree_count,
        planes: info.planes,
        materials: info.materials,
        brush_sides: info.brush_sides,
        brushes: info.brushes,
        brush_bounds: info.brush_bounds,
        brush_contents: info.brush_contents,
        nodes,
        leaves,
        leafbrushes: info.leafbrushes,
        leafbrush_nodes: info.leafbrush_nodes,
        verts,
        tri_indices,
        collision_partitions,
        collision_aabb_trees,
        cmodels,
        static_models,
    });
    let _ = (material_count, brush_side_count, leafbrush_node_count);

    s.pop()
}

struct ClipInfoGeom {
    planes: Option<Ptr>,
    materials: Option<Ptr>,
    material_count: usize,
    brush_sides: Option<Ptr>,
    brushes: Option<Ptr>,
    brush_bounds: Option<Ptr>,
    brush_contents: Option<Ptr>,
    leafbrush_nodes: Option<Ptr>,
    leafbrushes: Option<Ptr>,
    leafbrush_count: usize,
}

fn load_clip_info(s: &mut ZoneStream<'_>, info: Ptr) -> Result<ClipInfoGeom> {
    let plane_count = s.i32_at(info, 0)?.max(0) as usize;
    let material_count = s.u32_at(info, s.layout(8, 16))? as usize;
    let brush_side_count = s.u32_at(info, s.layout(0x10, 32))? as usize;
    let leafbrush_node_count = s.u32_at(info, s.layout(0x20, 64))? as usize;
    let brush_count = s.u16_at(info, s.layout(0x30, 96))? as usize;

    s.walk_stage = "clip_info.planes";
    let planes = s.plain_array(info, s.layout(4, 8), 4, sz::CPLANE, plane_count)?;
    s.walk_stage = "clip_info.materials";

    let clip_material = s.layout(sz::CLIP_MATERIAL, 16);
    let materials = s.plain_array(info, s.layout(0xc, 24), 4, clip_material, material_count)?;
    if let Some(materials) = materials {
        for i in 0..material_count {
            follow_name(s, materials.at(i * clip_material), 0)?;
        }
    }

    s.walk_stage = "clip_info.brush_sides";
    let brush_side = s.layout(sz::CBRUSH_SIDE, 16);
    let brush_sides = s.plain_array(info, s.layout(0x14, 40), 4, brush_side, brush_side_count)?;
    if let Some(sides) = brush_sides {
        for i in 0..brush_side_count {
            s.plain_array(sides.at(i * brush_side), 0, 4, sz::CPLANE, 1)?;
        }
    }

    s.walk_stage = "clip_info.brush_edges";
    let brush_edge_count = s.u32_at(info, s.layout(0x18, 48))? as usize;
    s.plain_array(info, s.layout(0x1c, 56), 1, 1, brush_edge_count)?;

    s.walk_stage = "clip_info.leafbrush_nodes";
    let leaf_brush_node = s.layout(sz::C_LEAF_BRUSH_NODE, 24);
    let leafbrush_nodes = if let Some(nodes) = s.plain_array(
        info,
        s.layout(0x24, 72),
        4,
        leaf_brush_node,
        leafbrush_node_count,
    )? {
        for i in 0..leafbrush_node_count {
            let node = nodes.at(i * leaf_brush_node);
            let leaf_brush_count = s.i16_at(node, 2)?;
            if leaf_brush_count > 0 {
                s.plain_array(node, 8, 2, 2, leaf_brush_count as usize)?;
            }
        }
        Some(nodes)
    } else {
        None
    };

    let leafbrush_count = s.u32_at(info, s.layout(0x28, 80))? as usize;
    let leafbrushes = s.plain_array(info, s.layout(0x2c, 88), 2, 2, leafbrush_count)?;

    s.walk_stage = "clip_info.brushes";
    let cbrush = s.layout(sz::CBRUSH, 48);
    let brushes = s.plain_array(info, s.layout(0x34, 104), 128, cbrush, brush_count)?;
    if let Some(brushes) = brushes {
        for i in 0..brush_count {
            let brush = brushes.at(i * cbrush);
            s.plain_array(brush, s.layout(4, 8), 4, brush_side, 1)?;
            s.plain_array(brush, s.layout(8, 16), 1, 1, 1)?;
        }
    }
    let brush_bounds = s.plain_array(info, s.layout(0x38, 112), 128, sz::BOUNDS, brush_count)?;
    let brush_contents = s.plain_array(info, s.layout(0x3c, 120), 4, 4, brush_count)?;

    Ok(ClipInfoGeom {
        planes,
        materials,
        material_count,
        brush_sides,
        brushes,
        brush_bounds,
        brush_contents,
        leafbrush_nodes,
        leafbrushes,
        leafbrush_count,
    })
}

fn load_clip_info_ptr(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    s.push(XFILE_BLOCK_TEMP)?;
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => {}
        ZonePtr::Offset(target) => {
            s.note_offset(target);
        }
        ZonePtr::Following | ZonePtr::Insert => {
            let (load, _) = s.begin_body_with_insert(slot)?;
            if load {
                let info = s.alloc_load(4, s.layout(sz::CLIP_INFO, 128))?;
                s.fixup_slot(slot, info)?;
                load_clip_info(s, info)?;
            }
        }
    }
    s.pop()
}

fn load_dyn_entity_defs(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    ty: usize,
) -> Result<()> {
    let width = s.pointer_bytes();
    let entity_def = s.layout(sz::DYN_ENTITY_DEF, 120);
    let count = s.u16_at(p, s.layout(0xc4, 392) + ty * 2)? as usize;
    let Some(defs) = s.plain_array(p, s.layout(0xc8, 400) + ty * width, 4, entity_def, count)?
    else {
        return Ok(());
    };
    for i in 0..count {
        let def = defs.at(i * entity_def);
        asset_ptr_at(s, links, AssetType::XModel, def.at(0x20))?;
        asset_ptr_at(s, links, AssetType::Fx, def.at(s.layout(0x28, 48)))?;
        asset_ptr_at(s, links, AssetType::PhysPreset, def.at(s.layout(0x2c, 56)))?;
        s.plain_array(def, s.layout(0x34, 72), 4, sz::DYN_ENTITY_HINGE, 1)?;
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

fn load_map_triggers(s: &mut ZoneStream<'_>, triggers: Ptr) -> Result<()> {
    let model_count = s.i32_at(triggers, 0)?.max(0) as usize;
    let hull_count = s.i32_at(triggers, s.layout(8, 16))?.max(0) as usize;
    let slab_count = s.i32_at(triggers, s.layout(16, 32))?.max(0) as usize;
    s.plain_array(triggers, s.layout(4, 8), 4, sz::TRIGGER_MODEL, model_count)?;
    s.plain_array(triggers, s.layout(12, 24), 4, sz::TRIGGER_HULL, hull_count)?;
    s.plain_array(triggers, s.layout(20, 40), 4, sz::TRIGGER_SLAB, slab_count)?;
    Ok(())
}

pub(super) fn load_mapents(s: &mut ZoneStream<'_>) -> Result<()> {
    s.walk_stage = "map_ents";
    let p = s.alloc_load(4, s.layout(sz::MAP_ENTS, 192))?;
    let entity_chars = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let entity_string = s.plain_array(p, s.layout(4, 8), 1, 1, entity_chars)?;
    s.record_map_ents(crate::zone::MapEntsGeometry {
        entity_string,
        entity_chars,
    });

    load_map_triggers(s, p.at(s.layout(0xc, 24)))?;
    load_client_triggers(s, p.at(s.layout(0x24, 72)))?;

    s.pop()
}

fn load_client_triggers(s: &mut ZoneStream<'_>, ct: Ptr) -> Result<()> {
    load_map_triggers(s, ct)?;
    let trigger_count = s.i32_at(ct, 0)?.max(0) as usize;
    let node_count = s.u16_at(ct, s.layout(0x18, 48))? as usize;
    s.plain_array(
        ct,
        s.layout(0x1c, 56),
        4,
        sz::CLIENT_TRIGGER_AABB_NODE,
        node_count,
    )?;
    let string_len = s.u32_at(ct, s.layout(0x20, 64))? as usize;
    s.plain_array(ct, s.layout(0x24, 72), 1, 1, string_len)?;
    s.plain_array(ct, s.layout(0x28, 80), 2, 2, trigger_count)?;
    s.plain_array(ct, s.layout(0x2c, 88), 1, 1, trigger_count)?;
    s.plain_array(ct, s.layout(0x30, 96), 4, 12, trigger_count)?;
    s.plain_array(ct, s.layout(0x34, 104), 4, 4, trigger_count)?;
    s.plain_array(ct, s.layout(0x38, 112), 2, 2, trigger_count)?;
    Ok(())
}
