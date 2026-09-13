use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{
    Ptr, Result, XFILE_BLOCK_INDEX, XFILE_BLOCK_VERTEX, XFILE_BLOCK_VIRTUAL, XModelGeometry,
    ZonePtr, ZoneStream,
};

pub(super) fn load_xmodel(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    s.walk_stage = "xmodel";

    let p = s.alloc_load(4, s.layout(sz::XMODEL, 416))?;
    let width = s.pointer_bytes();
    let num_bones = s.u8_at(p, s.layout(4, 8))? as usize;
    let num_root_bones = s.u8_at(p, s.layout(5, 9))? as usize;
    let num_surfs = s.u8_at(p, s.layout(6, 10))? as usize;
    let num_coll_surfs = s
        .i32_at(p, s.layout(sz::XMODEL_NUM_COLL_SURFS_OFF, 336))?
        .max(0) as usize;
    let num_child = num_bones.saturating_sub(num_root_bones);

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let bone_names_off = s.layout(sz::XMODEL_BONE_NAMES_OFF, 40);
    let parent_list_off = s.layout(40, 48);
    let quats_off = s.layout(44, 56);
    let trans_off = s.layout(48, 64);
    let base_mat_off = s.layout(56, 80);
    s.plain_array(p, bone_names_off, 2, 2, num_bones)?;
    s.plain_array(p, parent_list_off, 1, 1, num_child)?;
    s.plain_array(p, quats_off, 2, sz::XMODEL_QUAT, num_child)?;
    s.plain_array(p, trans_off, 4, 4, num_child * 3)?;
    s.plain_array(p, s.layout(52, 72), 1, 1, num_bones)?;
    s.plain_array(p, base_mat_off, 4, sz::DOBJ_ANIM_MAT, num_bones)?;

    let handles_off = s.layout(sz::XMODEL_MATERIAL_HANDLES_OFF, 88);
    let handles = match s.ptr_at(p, handles_off)? {
        ZonePtr::Following | ZonePtr::Insert => {
            let arr = s.alloc_load(4, width * num_surfs)?;
            s.fixup_slot(p.at(handles_off), arr)?;
            Some(arr)
        }
        ZonePtr::Offset(arr) => {
            s.note_offset(arr);
            Some(s.resolve_alias(arr))
        }
        ZonePtr::Null => None,
    };
    if let Some(arr) = handles {
        for i in 0..num_surfs {
            asset_ptr_at(s, links, AssetType::Material, arr.at(i * width))?;
        }
    }

    let lod_info_off = s.layout(sz::XMODEL_LOD_INFO_OFF, 96);
    let lod_info = s.layout(sz::XMODEL_LOD_INFO, 56);
    let mut lod0_surfaces = None;
    for i in 0..4 {
        let lp = p.at(lod_info_off + i * lod_info);
        s.walk_stage = "xmodel.lod";

        asset_ptr_at(s, links, AssetType::XModelSurfs, lp.at(8))?;
        if i == 0 {
            lod0_surfaces = match s.ptr_at(lp, 8)? {
                ZonePtr::Offset(surfs) => s.xmodel_surfs_array_at_alias(surfs),
                _ => s.take_latest_xmodel_surfs_array(),
            };
        }
    }

    s.walk_stage = "xmodel.coll_surfs";
    let coll_surfs_off = s.layout(sz::XMODEL_COLL_SURFS_OFF, 328);
    let coll_surf = s.layout(sz::XMODEL_COLL_SURF, 48);
    let coll_surfs = if s.begin_body(p.at(coll_surfs_off))? {
        let arr = s.alloc_load(4, coll_surf * num_coll_surfs)?;
        for i in 0..num_coll_surfs {
            let cp = arr.at(i * coll_surf);
            let tri_count = s.i32_at(cp, s.layout(4, 8))?.max(0) as usize;
            s.plain_array(cp, 0, 4, sz::XMODEL_COLL_TRI, tri_count)?;
        }
        Some(arr)
    } else {
        match s.ptr_at(p, coll_surfs_off)? {
            ZonePtr::Offset(arr) => {
                s.note_offset(arr);
                Some(s.resolve_alias(arr))
            }
            _ => None,
        }
    };

    s.plain_array(
        p,
        s.layout(sz::XMODEL_BONE_INFO_OFF, 344),
        4,
        sz::XBONE_INFO,
        num_bones,
    )?;

    asset_ptr_at(
        s,
        links,
        AssetType::PhysPreset,
        p.at(s.layout(sz::XMODEL_PHYS_PRESET_OFF, 392)),
    )?;
    asset_ptr_at(
        s,
        links,
        AssetType::PhysCollMap,
        p.at(s.layout(sz::XMODEL_PHYS_COLLMAP_OFF, 400)),
    )?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    let bone_names = match s.ptr_at(p, bone_names_off)? {
        ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
        _ => None,
    };
    let parent_list = match s.ptr_at(p, parent_list_off)? {
        ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
        _ => None,
    };
    let quats = match s.ptr_at(p, quats_off)? {
        ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
        _ => None,
    };
    let trans = match s.ptr_at(p, trans_off)? {
        ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
        _ => None,
    };
    let base_mat = match s.ptr_at(p, base_mat_off)? {
        ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
        _ => None,
    };
    let mut no_scale_part_bits = [0u32; 6];
    for (i, slot) in no_scale_part_bits.iter_mut().enumerate() {
        *slot = s.u32_at(p, s.layout(12, 16) + i * 4)?;
    }
    s.record_xmodel(XModelGeometry {
        name,
        material_handles: handles,
        surfaces: lod0_surfaces,
        surface_count: s.u16_at(p.at(lod_info_off), 4)? as usize,
        num_bones,
        num_root_bones,
        scale: s.f32_at(p, s.layout(8, 12))?,
        no_scale_part_bits,
        bone_names,
        parent_list,
        quats,
        trans,
        base_mat,
        coll_surfs,
        num_coll_surfs: num_coll_surfs as i32,
        coll_lod: s.u8_at(p, s.layout(0xf2, 322))? as i8 as i16,
        contents: s.u32_at(p, s.layout(0xfc, 340))?,
        radius: Some(s.f32_at(p, s.layout(sz::XMODEL_RADIUS_OFF, 352))?),
    });

    s.pop()
}

pub(super) fn load_xmodel_surfs(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let _ = links;
    s.walk_stage = "xmodel_surfs";
    let p = s.alloc_load(4, s.layout(sz::XMODEL_SURFS, 48))?;
    let insert_slot = s.insert_slot_bound_to(p);
    let num_surfs = s.u16_at(p, s.layout(8, 16))? as usize;
    let surfs_off = s.layout(4, 8);
    let xsurface = s.layout(sz::XSURFACE, 88);
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let surfaces = if s.begin_body(p.at(surfs_off))? {
        let arr = s.alloc_load(4, xsurface * num_surfs)?;
        s.fixup_slot(p.at(surfs_off), arr)?;
        for i in 0..num_surfs {
            load_xsurface(s, arr.at(i * xsurface))?;
        }
        Some(arr)
    } else {
        match s.ptr_at(p, surfs_off)? {
            ZonePtr::Offset(arr) => Some(s.resolve_alias(arr)),
            _ => None,
        }
    };
    s.record_xmodel_surfs_array(surfaces);
    if let Some(slot) = insert_slot {
        s.publish_xmodel_surfs_insert(slot, surfaces)?;
    }
    s.pop()
}

fn load_xsurface(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    s.walk_stage = "xsurface";
    let vert_count = s.u16_at(p, 2)? as usize;
    let tri_count = s.u16_at(p, 4)? as usize;
    let vert_list_count = s.u32_at(p, s.layout(sz::XSURFACE_VERT_LIST_COUNT_OFF, 48))? as usize;

    let vi = p.at(s.layout(sz::XSURFACE_VERT_INFO_OFF, 24));
    let blend_len = s.i16_at(vi, 0)?.max(0) as usize
        + 3 * s.i16_at(vi, 2)?.max(0) as usize
        + 5 * s.i16_at(vi, 4)?.max(0) as usize
        + 7 * s.i16_at(vi, 6)?.max(0) as usize;
    s.plain_array(vi, 8, 2, 2, blend_len)?;

    s.push(XFILE_BLOCK_VERTEX)?;
    s.plain_array(
        p,
        s.layout(sz::XSURFACE_VERTS0_OFF, 40),
        16,
        sz::GFX_PACKED_VERTEX,
        vert_count,
    )?;
    s.pop()?;

    let vert_list_off = s.layout(sz::XSURFACE_VERT_LIST_OFF, 56);
    if s.begin_body(p.at(vert_list_off))? {
        let vert_list = s.layout(sz::XRIGID_VERT_LIST, 16);
        let arr = s.alloc_load(4, vert_list * vert_list_count)?;
        s.fixup_slot(p.at(vert_list_off), arr)?;
        for i in 0..vert_list_count {
            let b = arr.at(i * vert_list);
            if s.begin_body(b.at(8))? {
                load_collision_tree(s)?;
            }
        }
    }

    s.push(XFILE_BLOCK_INDEX)?;
    s.plain_array(
        p,
        sz::XSURFACE_TRI_INDICES_OFF,
        16,
        sz::XSURFACE_TRI16,
        tri_count,
    )?;
    s.pop()
}

fn load_collision_tree(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::XSURFACE_COLLISION_TREE, 56))?;
    let node_count = s.u32_at(p, 24)? as usize;
    let leaf_count = s.u32_at(p, s.layout(32, 40))? as usize;
    s.plain_array(
        p,
        s.layout(28, 32),
        16,
        sz::XSURFACE_COLLISION_NODE,
        node_count,
    )?;
    s.plain_array(
        p,
        s.layout(36, 48),
        2,
        sz::XSURFACE_COLLISION_LEAF,
        leaf_count,
    )?;
    Ok(())
}

pub(super) fn load_phys_collmap(
    s: &mut ZoneStream<'_>,
    _links: &mut dyn AssetLinkSink,
) -> Result<()> {
    s.walk_stage = "phys_collmap";
    let p = s.alloc_load(4, s.layout(sz::PHYS_COLLMAP, 88))?;
    let count = s.u32_at(p, s.layout(4, 8))? as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if s.begin_body(p.at(s.layout(8, 16)))? {
        let geom = s.layout(sz::PHYS_GEOM_INFO, 72);
        let arr = s.alloc_load(4, geom * count)?;
        for i in 0..count {
            let g = arr.at(i * geom);
            if s.begin_body(g.at(0))? {
                load_brush_wrapper(s)?;
            }
        }
    }
    s.pop()
}

fn load_brush_wrapper(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::BRUSH_WRAPPER, 88))?;
    let numsides = s.u16_at(p, 24)? as usize;
    let total_edge_count = s.i32_at(p, s.layout(60, 72))?.max(0) as usize;
    if s.begin_body(p.at(s.layout(28, 32)))? {
        let side = s.layout(sz::CBRUSH_SIDE, 16);
        let sides = s.alloc_load(4, side * numsides)?;
        for i in 0..numsides {
            let sp = sides.at(i * side);
            if s.begin_body(sp.at(0))? {
                s.alloc_load(4, sz::CPLANE)?;
            }
        }
    }
    s.plain_array(p, s.layout(32, 40), 1, 1, total_edge_count)?;
    s.plain_array(p, s.layout(64, 80), 4, sz::CPLANE, numsides)?;
    Ok(())
}
