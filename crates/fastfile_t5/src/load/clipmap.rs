use super::{AssetLinkSink, always_array, asset_ptr_at, follow_name, runtime_array};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{ClipMapGeometry, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_clip_map(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::CLIP_MAP)?;

    let plane_count = s.i32_at(p, 8)?.max(0) as usize;
    let num_static_models = s.u32_at(p, 0x10)? as usize;
    let num_materials = s.u32_at(p, 0x18)? as usize;
    let num_brush_sides = s.u32_at(p, 0x20)? as usize;
    let num_nodes = s.u32_at(p, 0x28)? as usize;
    let num_leafs = s.u32_at(p, 0x30)? as usize;
    let leafbrush_nodes_count = s.u32_at(p, 0x38)? as usize;
    let num_leaf_brushes = s.u32_at(p, 0x40)? as usize;
    let num_leaf_surfaces = s.u32_at(p, 0x48)? as usize;
    let vert_count = s.u32_at(p, 0x50)? as usize;
    let num_brush_verts = s.u32_at(p, 0x58)? as usize;
    let nuinds = s.u32_at(p, 0x60)? as usize;
    let tri_count = s.i32_at(p, 0x68)?.max(0) as usize;
    let border_count = s.i32_at(p, 0x74)?.max(0) as usize;
    let partition_count = s.i32_at(p, 0x7c)?.max(0) as usize;
    let aabb_tree_count = s.i32_at(p, 0x84)?.max(0) as usize;
    let num_sub_models = s.u32_at(p, 0x8c)? as usize;
    let num_brushes = s.u16_at(p, 0x94)? as usize;
    let num_clusters = s.i32_at(p, 0x9c)?.max(0) as usize;
    let cluster_bytes = s.i32_at(p, 0xa0)?.max(0) as usize;
    let dyn0 = s.u16_at(p, 0xfe)? as usize;
    let dyn1 = s.u16_at(p, 0x100)? as usize;
    let dyn2 = s.u16_at(p, 0x102)? as usize;
    let dyn3 = s.u16_at(p, 0x104)? as usize;
    let num_constraints = s.i32_at(p, 0x138)?.max(0) as usize;
    let max_ropes = s.i32_at(p, 0x140)?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let mut planes = None;
    if s.begin_body(p.at(0x0c))? {
        let body = s.alloc_load(4, sz::CPLANE * plane_count)?;
        s.fixup_slot(p.at(0x0c), body)?;
        planes = Some(body);
    } else if let crate::zone::ZonePtr::Offset(q) = s.ptr_at(p, 0x0c)? {
        planes = Some(q);
    }

    let static_models = always_array(s, p.at(0x14), 4, sz::C_STATIC_MODEL * num_static_models)?;
    if let Some(models) = static_models {
        for i in 0..num_static_models {
            asset_ptr_at(
                s,
                links,
                AssetType::XModel,
                models.at(i * sz::C_STATIC_MODEL + sz::C_STATIC_MODEL_XMODEL_OFF),
            )?;
        }
    }

    let materials = always_array(s, p.at(0x1c), 4, sz::DMATERIAL * num_materials)?;

    if let Some(sides) = always_array(s, p.at(0x24), 4, sz::CBRUSH_SIDE * num_brush_sides)? {
        for i in 0..num_brush_sides {
            load_brush_side_plane(s, sides.at(i * sz::CBRUSH_SIDE))?;
        }
    }

    let nodes = always_array(s, p.at(0x2c), 4, sz::C_NODE * num_nodes)?;
    if let Some(nodes) = nodes {
        for i in 0..num_nodes {
            if s.begin_body(nodes.at(i * sz::C_NODE))? {
                let plane = s.alloc_load(4, sz::CPLANE)?;
                s.fixup_slot(nodes.at(i * sz::C_NODE), plane)?;
            }
        }
    }

    let leaves = always_array(s, p.at(0x34), 4, sz::C_LEAF * num_leafs)?;

    always_array(s, p.at(0x44), 2, 2 * num_leaf_brushes)?;

    let leafbrush_nodes = always_array(
        s,
        p.at(0x3c),
        4,
        sz::C_LEAF_BRUSH_NODE * leafbrush_nodes_count,
    )?;
    if let Some(lb_nodes) = leafbrush_nodes {
        for i in 0..leafbrush_nodes_count {
            load_leaf_brush_node(s, lb_nodes.at(i * sz::C_LEAF_BRUSH_NODE))?;
        }
    }

    always_array(s, p.at(0x4c), 4, 4 * num_leaf_surfaces)?;
    let verts = always_array(s, p.at(0x54), 4, 12 * vert_count)?;
    always_array(s, p.at(0x5c), 4, 12 * num_brush_verts)?;
    always_array(s, p.at(0x64), 2, 2 * nuinds)?;
    let tri_indices = always_array(s, p.at(0x6c), 2, 2 * (3 * tri_count))?;
    always_array(s, p.at(0x70), 1, 4 * ((3 * tri_count + 31) >> 5))?;
    always_array(s, p.at(0x78), 4, sz::COLLISION_BORDER * border_count)?;

    let collision_partitions =
        always_array(s, p.at(0x80), 4, sz::COLLISION_PARTITION * partition_count)?;
    if let Some(parts) = collision_partitions {
        for i in 0..partition_count {
            let part = parts.at(i * sz::COLLISION_PARTITION);

            if s.begin_body(part.at(sz::COLLISION_PARTITION_BORDERS_OFF))? {
                s.alloc_load(4, sz::COLLISION_BORDER)?;
            }
        }
    }

    let collision_aabb_trees =
        always_array(s, p.at(0x88), 16, sz::COLLISION_AABB_TREE * aabb_tree_count)?;
    let cmodels = always_array(s, p.at(0x90), 4, sz::C_MODEL * num_sub_models)?;

    let mut brushes = None;
    if let Some(body) = always_array(s, p.at(0x98), 16, sz::C_BRUSH * num_brushes)? {
        brushes = Some(body);
        for i in 0..num_brushes {
            load_cbrush(s, body.at(i * sz::C_BRUSH))?;
        }
    }

    always_array(s, p.at(0xa4), 1, num_clusters * cluster_bytes)?;

    asset_ptr_at(s, links, AssetType::MapEnts, p.at(0xac))?;

    if s.begin_body(p.at(0xb0))? {
        let brush = s.alloc_load(16, sz::C_BRUSH)?;
        s.fixup_slot(p.at(0xb0), brush)?;
        load_cbrush(s, brush)?;
    }

    load_dyn_ent_defs(s, links, p.at(0x108), dyn0)?;
    load_dyn_ent_defs(s, links, p.at(0x10c), dyn1)?;

    runtime_array(s, p.at(0x110), 4, sz::DYN_ENTITY_POSE * dyn0)?;
    runtime_array(s, p.at(0x114), 4, sz::DYN_ENTITY_POSE * dyn1)?;
    runtime_array(s, p.at(0x118), 4, sz::DYN_ENTITY_CLIENT * dyn0)?;
    runtime_array(s, p.at(0x11c), 4, sz::DYN_ENTITY_CLIENT * dyn1)?;
    runtime_array(s, p.at(0x120), 4, sz::DYN_ENTITY_SERVER * dyn2)?;
    runtime_array(s, p.at(0x124), 4, sz::DYN_ENTITY_SERVER * dyn3)?;
    runtime_array(s, p.at(0x128), 4, sz::DYN_ENTITY_COLL * dyn0)?;
    runtime_array(s, p.at(0x12c), 4, sz::DYN_ENTITY_COLL * dyn1)?;
    runtime_array(s, p.at(0x130), 4, sz::DYN_ENTITY_COLL * dyn2)?;
    runtime_array(s, p.at(0x134), 4, sz::DYN_ENTITY_COLL * dyn3)?;

    if let Some(constraints) =
        always_array(s, p.at(0x13c), 4, sz::PHYS_CONSTRAINT * num_constraints)?
    {
        for i in 0..num_constraints {
            let c = constraints.at(i * sz::PHYS_CONSTRAINT);
            follow_name(s, c, sz::PHYS_CONSTRAINT_BONE1_OFF)?;
            follow_name(s, c, sz::PHYS_CONSTRAINT_BONE2_OFF)?;
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                c.at(sz::PHYS_CONSTRAINT_MATERIAL_OFF),
            )?;
        }
    }

    runtime_array(s, p.at(0x144), 4, sz::ROPE * max_ropes)?;

    let name = match s.ptr_at(p, 0)? {
        crate::zone::ZonePtr::Offset(q) => Some(q),
        _ => None,
    };
    s.record_clip_map(ClipMapGeometry {
        name,
        static_models,
        static_model_count: num_static_models,
        plane_count,
        brush_count: num_brushes,
        cmodel_count: num_sub_models,
        leaf_count: num_leafs,
        node_count: num_nodes,
        vert_count,
        tri_count,
        planes,
        brushes,
        verts,
        tri_indices,
        materials,
        material_count: num_materials,
        collision_partitions,
        partition_count,
        collision_aabb_trees,
        aabb_tree_count,
        nodes,
        leaves,
        leafbrush_nodes,
        cmodels,
    });

    s.pop()
}

fn load_brush_side_plane(s: &mut ZoneStream<'_>, side: Ptr) -> Result<()> {
    if s.begin_body(side.at(sz::CBRUSH_SIDE_PLANE_OFF))? {
        let plane = s.alloc_load(4, sz::CPLANE)?;
        s.fixup_slot(side.at(sz::CBRUSH_SIDE_PLANE_OFF), plane)?;
    }
    Ok(())
}

fn load_leaf_brush_node(s: &mut ZoneStream<'_>, node: Ptr) -> Result<()> {
    let leaf_brush_count = s.i16_at(node, sz::C_LEAF_BRUSH_NODE_COUNT_OFF)?;
    if leaf_brush_count > 0 {
        let count = leaf_brush_count as usize;
        let data = node.at(sz::C_LEAF_BRUSH_NODE_DATA_OFF);
        if s.begin_body(data)? {
            let brushes = s.alloc_load(2, 2 * count)?;
            s.fixup_slot(data, brushes)?;
        }
    }
    Ok(())
}

fn load_cbrush(s: &mut ZoneStream<'_>, brush: Ptr) -> Result<()> {
    if s.begin_body(brush.at(sz::C_BRUSH_SIDES_OFF))? {
        let side = s.alloc_load(4, sz::CBRUSH_SIDE)?;
        s.fixup_slot(brush.at(sz::C_BRUSH_SIDES_OFF), side)?;
        load_brush_side_plane(s, side)?;
    }
    if s.begin_body(brush.at(sz::C_BRUSH_VERTS_OFF))? {
        s.alloc_load(4, 12)?;
    }
    Ok(())
}

fn load_dyn_ent_defs(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    count: usize,
) -> Result<()> {
    let Some(defs) = always_array(s, slot, 4, sz::DYN_ENTITY_DEF * count)? else {
        return Ok(());
    };
    for i in 0..count {
        let d = defs.at(i * sz::DYN_ENTITY_DEF);
        asset_ptr_at(
            s,
            links,
            AssetType::XModel,
            d.at(sz::DYN_ENTITY_DEF_XMODEL_OFF),
        )?;
        asset_ptr_at(
            s,
            links,
            AssetType::XModel,
            d.at(sz::DYN_ENTITY_DEF_DESTROYED_XMODEL_OFF),
        )?;
        asset_ptr_at(
            s,
            links,
            AssetType::Fx,
            d.at(sz::DYN_ENTITY_DEF_DESTROY_FX_OFF),
        )?;
        asset_ptr_at(
            s,
            links,
            AssetType::XModelPieces,
            d.at(sz::DYN_ENTITY_DEF_DESTROY_PIECES_OFF),
        )?;
        asset_ptr_at(
            s,
            links,
            AssetType::PhysPreset,
            d.at(sz::DYN_ENTITY_DEF_PHYS_PRESET_OFF),
        )?;
    }
    Ok(())
}
