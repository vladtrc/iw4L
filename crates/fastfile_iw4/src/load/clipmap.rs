use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at_linked, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{
    ClipMapGeometry, Ptr, Result, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_VIRTUAL, ZoneStream,
};

pub(super) fn load_clipmap(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::CLIP_MAP, 512))?;
    let plane_count = s.i32_at(p, s.layout(8, 12))?.max(0) as usize;
    let static_model_count = s.u32_at(p, s.layout(16, 24))? as usize;
    let material_count = s.u32_at(p, s.layout(24, 40))? as usize;
    let brush_side_count = s.u32_at(p, s.layout(32, 56))? as usize;
    let node_count = s.u32_at(p, s.layout(48, 88))? as usize;
    let leaf_count = s.u32_at(p, s.layout(56, 104))? as usize;
    let leafbrush_node_count = s.u32_at(p, s.layout(64, 120))? as usize;
    let vert_count = s.u32_at(p, s.layout(88, 168))? as usize;
    let tri_count = s.i32_at(p, s.layout(96, 184))?.max(0) as usize;
    let cmodel_count = s.u32_at(p, s.layout(132, 256))? as usize;
    let brush_count = s.u16_at(p, s.layout(140, 272))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    let name = s.follow_string(p, 0)?;

    let planes = s.plain_array(p, s.layout(12, 16), 4, sz::CPLANE, plane_count)?;

    let static_models = s.plain_array(
        p,
        s.layout(20, 32),
        4,
        s.layout(sz::C_STATIC_MODEL, 80),
        static_model_count,
    )?;
    if let Some(models) = static_models {
        for i in 0..static_model_count {
            asset_ptr_at_linked(
                s,
                links,
                AssetType::XModel,
                models.at(i * s.layout(sz::C_STATIC_MODEL, 80)),
            )?;
        }
    }

    let materials = s.plain_array(
        p,
        s.layout(28, 48),
        4,
        s.layout(sz::CLIP_MATERIAL, 16),
        material_count,
    )?;
    if let Some(materials) = materials {
        for i in 0..material_count {
            follow_name(s, materials.at(i * s.layout(sz::CLIP_MATERIAL, 16)), 0)?;
        }
    }

    let brush_sides = s.plain_array(
        p,
        s.layout(36, 64),
        4,
        s.layout(sz::CBRUSH_SIDE, 16),
        brush_side_count,
    )?;
    if let Some(sides) = brush_sides {
        for i in 0..brush_side_count {
            s.plain_array(
                sides.at(i * s.layout(sz::CBRUSH_SIDE, 16)),
                0,
                4,
                sz::CPLANE,
                1,
            )?;
        }
    }

    s.plain_array(
        p,
        s.layout(44, 80),
        1,
        1,
        s.u32_at(p, s.layout(40, 72))? as usize,
    )?;

    let nodes = if let Some(nodes) =
        s.plain_array(p, s.layout(52, 96), 4, s.layout(sz::C_NODE, 16), node_count)?
    {
        for i in 0..node_count {
            s.plain_array(nodes.at(i * s.layout(sz::C_NODE, 16)), 0, 4, sz::CPLANE, 1)?;
        }
        Some(nodes)
    } else {
        None
    };

    let leaves = s.plain_array(p, s.layout(60, 112), 4, sz::C_LEAF, leaf_count)?;

    let leafbrush_count = s.u32_at(p, s.layout(72, 136))? as usize;
    let leafbrushes = s.plain_array(p, s.layout(76, 144), 2, 2, leafbrush_count)?;

    let leafbrush_nodes = if let Some(lb_nodes) = s.plain_array(
        p,
        s.layout(68, 128),
        4,
        s.layout(sz::C_LEAF_BRUSH_NODE, 24),
        leafbrush_node_count,
    )? {
        for i in 0..leafbrush_node_count {
            let node = lb_nodes.at(i * s.layout(sz::C_LEAF_BRUSH_NODE, 24));
            let leaf_brush_count = s.i16_at(node, 2)?;
            if leaf_brush_count > 0 {
                s.plain_array(node, 8, 2, 2, leaf_brush_count as usize)?;
            }
        }
        Some(lb_nodes)
    } else {
        None
    };

    s.plain_array(
        p,
        s.layout(84, 160),
        4,
        4,
        s.u32_at(p, s.layout(80, 152))? as usize,
    )?;
    let verts = s.plain_array(p, s.layout(92, 176), 4, 12, vert_count)?;
    let index_count = 3 * tri_count;
    let tri_indices = s.plain_array(p, s.layout(100, 192), 2, 2, index_count)?;
    let tri_edge_is_walkable =
        s.plain_array(p, s.layout(104, 200), 1, 1, index_count.div_ceil(32) * 4)?;
    s.plain_array(
        p,
        s.layout(112, 216),
        4,
        sz::COLLISION_BORDER,
        s.i32_at(p, s.layout(108, 208))?.max(0) as usize,
    )?;

    let partition_count = s.i32_at(p, s.layout(116, 224))?.max(0) as usize;
    let collision_partitions = if let Some(parts) = s.plain_array(
        p,
        s.layout(120, 232),
        4,
        s.layout(sz::COLLISION_PARTITION, 16),
        partition_count,
    )? {
        for i in 0..partition_count {
            s.plain_array(
                parts.at(i * s.layout(sz::COLLISION_PARTITION, 16)),
                8,
                4,
                sz::COLLISION_BORDER,
                1,
            )?;
        }
        Some(parts)
    } else {
        None
    };

    let aabb_tree_count = s.i32_at(p, s.layout(124, 240))?.max(0) as usize;
    let collision_aabb_trees = s.plain_array(
        p,
        s.layout(128, 248),
        16,
        sz::COLLISION_AABB_TREE,
        aabb_tree_count,
    )?;
    let cmodels = s.plain_array(p, s.layout(136, 264), 4, sz::CMODEL, cmodel_count)?;

    let brushes = s.plain_array(
        p,
        s.layout(144, 280),
        128,
        s.layout(sz::CBRUSH, 48),
        brush_count,
    )?;
    if let Some(brushes) = brushes {
        for i in 0..brush_count {
            let brush = brushes.at(i * s.layout(sz::CBRUSH, 48));
            s.plain_array(brush, s.layout(4, 8), 4, s.layout(sz::CBRUSH_SIDE, 16), 1)?;
            s.plain_array(brush, s.layout(8, 16), 1, 1, 1)?;
        }
    }
    let brush_bounds = s.plain_array(p, s.layout(148, 288), 128, 24, brush_count)?;
    let brush_contents = s.plain_array(p, s.layout(152, 296), 4, 4, brush_count)?;

    s.plain_array(
        p,
        s.layout(164, 320),
        4,
        sz::SMODEL_AABB_NODE,
        s.u16_at(p, s.layout(160, 312))? as usize,
    )?;
    asset_ptr_at_linked(s, links, AssetType::MapEnts, p.at(s.layout(156, 304)))?;

    let mut dyn_ent_count = [0usize; 2];
    let mut dyn_ent_defs = [None; 2];
    for ty in 0..2 {
        dyn_ent_count[ty] = s.u16_at(p, s.layout(168, 328) + ty * 2)? as usize;
        dyn_ent_defs[ty] = load_dyn_entity_defs(s, links, p, ty)?;
    }
    for ty in 0..2 {
        runtime_array(
            s,
            p,
            s.layout(180, 352) + ty * s.pointer_bytes(),
            4,
            sz::DYN_ENTITY_POSE,
            dyn_ent_count[ty],
        )?;
    }
    for ty in 0..2 {
        runtime_array(
            s,
            p,
            s.layout(188, 368) + ty * s.pointer_bytes(),
            4,
            s.layout(sz::DYN_ENTITY_CLIENT, 16),
            dyn_ent_count[ty],
        )?;
    }
    for ty in 0..2 {
        runtime_array(
            s,
            p,
            s.layout(196, 384) + ty * s.pointer_bytes(),
            4,
            sz::DYN_ENTITY_COLL,
            dyn_ent_count[ty],
        )?;
    }

    s.record_clip_map(ClipMapGeometry {
        name,
        plane_count,
        static_model_count,
        static_models,
        material_count,
        brush_side_count,
        node_count,
        leaf_count,
        leafbrush_node_count,
        brush_count,
        cmodel_count,
        vert_count,
        tri_count,
        planes,
        materials,
        brush_sides,
        brushes,
        brush_bounds,
        brush_contents,
        nodes,
        leaves,
        leafbrushes,
        leafbrush_count,
        leafbrush_nodes,
        verts,
        tri_indices,
        tri_edge_is_walkable,
        collision_partitions,
        partition_count,
        collision_aabb_trees,
        aabb_tree_count,
        cmodels,
        dyn_ent_count,
        dyn_ent_defs,
    });

    s.pop()
}

fn load_dyn_entity_defs(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    ty: usize,
) -> Result<Option<Ptr>> {
    let count = s.u16_at(p, s.layout(168, 328) + ty * 2)? as usize;
    let Some(defs) = s.plain_array(
        p,
        s.layout(172, 336) + ty * s.pointer_bytes(),
        4,
        s.layout(sz::DYN_ENTITY_DEF, 112),
        count,
    )?
    else {
        return Ok(None);
    };
    for i in 0..count {
        let def = defs.at(i * s.layout(sz::DYN_ENTITY_DEF, 112));
        asset_ptr_at_linked(s, links, AssetType::XModel, def.at(32))?;
        asset_ptr_at_linked(s, links, AssetType::Fx, def.at(s.layout(40, 48)))?;
        asset_ptr_at_linked(s, links, AssetType::PhysPreset, def.at(s.layout(44, 56)))?;
    }
    Ok(Some(defs))
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
        s.alloc_load(align, elem * count)?;
        s.pop()?;
    }
    Ok(())
}
