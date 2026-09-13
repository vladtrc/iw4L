use super::{AssetLinkSink, always_alloc, always_array, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::xmodel_lod as lod;
use crate::zone::{AlignWasteSite, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_xmodel(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::XMODEL)?;
    let num_bones = s.u8_at(p, 4)? as usize;
    let num_root = s.u8_at(p, 5)? as usize;
    let num_surfs = s.u8_at(p, 6)? as usize;
    let num_child = num_bones.saturating_sub(num_root);
    let num_coll_surfs = s.i32_at(p, sz::XMODEL_NUM_COLL_SURFS_OFF)?.max(0) as usize;
    let num_collmaps = s.u8_at(p, sz::XMODEL_NUM_COLLMAPS_OFF)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    s.align_waste_xmodel_entry_phase(s.watermark(XFILE_BLOCK_VIRTUAL as u8) & 15);
    follow_name(s, p, 0)?;

    s.with_align_site(AlignWasteSite::BoneArrays, |s| {
        s.plain_array(p, 8, 2, 2, num_bones)?;
        s.plain_array(p, 12, 1, 1, num_child)?;
        s.plain_array(p, 16, 2, 2, 4 * num_child)?;

        s.plain_array(p, 20, 4, 4, (sz::XMODEL_TRANS_OCCUPANCY / 4) * num_child)?;
        s.plain_array(p, 24, 1, 1, num_bones)?;
        s.plain_array(p, 28, 4, sz::DOBJ_ANIM_MAT, num_bones)?;
        Ok(())
    })?;
    if always_alloc(s, p.at(32))? {
        let arr = s.with_align_site(AlignWasteSite::XModelMisc, |s| {
            let arr = s.alloc_load(4, sz::XSURFACE * num_surfs)?;
            s.fixup_slot(p.at(32), arr)?;
            Ok(arr)
        })?;
        for i in 0..num_surfs {
            load_xsurface(s, arr.at(i * sz::XSURFACE))?;
        }
    }

    if always_alloc(s, p.at(36))? {
        let arr = s.with_align_site(AlignWasteSite::XModelMisc, |s| {
            let arr = s.alloc_load(4, 4 * num_surfs)?;
            s.fixup_slot(p.at(36), arr)?;
            Ok(arr)
        })?;
        for i in 0..num_surfs {
            asset_ptr_at(s, links, AssetType::Material, arr.at(i * 4))?;
        }
    }

    if always_alloc(s, p.at(sz::XMODEL_COLL_SURFS_OFF))? {
        let arr = s.with_align_site(AlignWasteSite::XModelMisc, |s| {
            let arr = s.alloc_load(4, sz::XMODEL_COLL_SURF * num_coll_surfs)?;
            s.fixup_slot(p.at(sz::XMODEL_COLL_SURFS_OFF), arr)?;
            Ok(arr)
        })?;
        for i in 0..num_coll_surfs {
            let cp = arr.at(i * sz::XMODEL_COLL_SURF);
            let tri_count = s.i32_at(cp, 4)?.max(0) as usize;
            s.with_align_site(AlignWasteSite::XModelMisc, |s| {
                s.plain_array(cp, 0, 4, sz::XMODEL_COLL_TRI, tri_count)?;
                Ok(())
            })?;
        }
    }

    if always_alloc(s, p.at(sz::XMODEL_BONE_INFO_OFF))? {
        s.with_align_site(AlignWasteSite::XModelMisc, |s| {
            let arr = s.alloc_load(4, sz::XBONE_INFO * num_bones)?;
            s.fixup_slot(p.at(sz::XMODEL_BONE_INFO_OFF), arr)?;
            Ok(())
        })?;
    }

    if always_alloc(s, p.at(sz::XMODEL_STREAM_INFO_OFF))? {
        s.with_align_site(AlignWasteSite::XModelMisc, |s| {
            let arr = s.alloc_load(4, sz::XMODEL_HIGH_MIP_BOUNDS * num_surfs)?;
            s.fixup_slot(p.at(sz::XMODEL_STREAM_INFO_OFF), arr)?;
            Ok(())
        })?;
    }

    asset_ptr_at(
        s,
        links,
        AssetType::PhysPreset,
        p.at(sz::XMODEL_PHYS_PRESET_OFF),
    )?;

    if always_alloc(s, p.at(sz::XMODEL_COLLMAPS_OFF))? {
        let arr = s.alloc_load(4, sz::COLLMAP * num_collmaps)?;
        s.fixup_slot(p.at(sz::XMODEL_COLLMAPS_OFF), arr)?;
        for i in 0..num_collmaps {
            load_collmap(s, arr.at(i * sz::COLLMAP))?;
        }
    }

    asset_ptr_at(
        s,
        links,
        AssetType::PhysConstraints,
        p.at(sz::XMODEL_PHYS_CONSTRAINTS_OFF),
    )?;

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    let surfaces = match s.ptr_at(p, 32)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let material_handles = match s.ptr_at(p, 36)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let bone_names = match s.ptr_at(p, 8)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let parent_list = match s.ptr_at(p, 12)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let quats = match s.ptr_at(p, 16)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let trans = match s.ptr_at(p, 20)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let base_mat = match s.ptr_at(p, 28)? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let retained_ptr = |off| -> Result<Option<Ptr>> {
        Ok(match s.ptr_at(p, off)? {
            ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
            ZonePtr::Null => None,
            _ => None,
        })
    };
    let mut lod_surf_span = [(0, 0); 4];
    let mut lod_dist = [0.0f32; 4];
    for (lod, span) in lod_surf_span.iter_mut().enumerate() {
        let record = lod::XMODEL_LOD_INFO_OFF + lod * lod::XMODEL_LOD_INFO_STRIDE;
        *span = (s.u16_at(p, record + 6)?, s.u16_at(p, record + 4)?);
        lod_dist[lod] = s.f32_at(p, record)?;
    }
    s.record_xmodel(crate::zone::XModelGeometry {
        name,
        material_handles,
        surfaces,
        surface_count: num_surfs,
        lod_surf_span,
        lod_dist,
        num_lods: s.i16_at(p, lod::XMODEL_NUM_LODS_OFF)?,
        num_bones,
        num_root_bones: num_root,
        bone_names,
        parent_list,
        quats,
        trans,
        base_mat,
        part_classification: retained_ptr(24)?,
        bone_info: retained_ptr(sz::XMODEL_BONE_INFO_OFF)?,
        coll_surfs: retained_ptr(sz::XMODEL_COLL_SURFS_OFF)?,
        num_coll_surfs,
        coll_lod: s.i16_at(p, 218)?,
        contents: s.u32_at(p, 180)?,
        radius: Some(s.f32_at(p, sz::XMODEL_RADIUS_OFF)?),
    });

    s.pop()
}

fn load_xsurface(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let vert_list_count = s.u8_at(p, 1)? as usize;
    let vert_count = s.u16_at(p, 4)? as usize;
    let tri_count = s.u16_at(p, 6)? as usize;

    let vi = p.at(16);
    let v0 = s.i16_at(vi, 0)?.max(0) as usize;
    let v1 = s.i16_at(vi, 2)?.max(0) as usize;
    let v2 = s.i16_at(vi, 4)?.max(0) as usize;
    let v3 = s.i16_at(vi, 6)?.max(0) as usize;
    s.with_align_site(AlignWasteSite::VertInfo, |s| {
        s.plain_array(vi, 8, 2, 2, v0 + 3 * v1 + 5 * v2 + 7 * v3)?;
        s.plain_array(vi, 12, 4, 4, 12 * (v0 + v1 + v2 + v3))?;
        Ok(())
    })?;

    s.with_align_site(AlignWasteSite::Verts0, |s| {
        match s.ptr_at(p, 32)? {
            ZonePtr::Following | ZonePtr::Insert => {
                let phase = (s.watermark(XFILE_BLOCK_VIRTUAL as u8) & 15) as usize;
                s.align_waste_verts0_phase(phase);
            }
            _ => {}
        }
        s.plain_array(p, 32, 16, sz::GFX_PACKED_VERTEX, vert_count)?;
        Ok(())
    })?;

    if s.begin_body(p.at(40))? {
        let arr = s.with_align_site(AlignWasteSite::VertList, |s| {
            let arr = s.alloc_load(4, sz::XRIGID_VERT_LIST * vert_list_count)?;
            s.fixup_slot(p.at(40), arr)?;
            Ok(arr)
        })?;
        for i in 0..vert_list_count {
            let b = arr.at(i * sz::XRIGID_VERT_LIST);
            if s.begin_body(b.at(8))? {
                let tree = load_collision_tree(s)?;
                s.fixup_slot(b.at(8), tree)?;
            }
        }
    }

    s.with_align_site(AlignWasteSite::TriIndices, |s| {
        s.plain_array(p, 12, 16, 2, 3 * tri_count)?;
        Ok(())
    })?;
    Ok(())
}

fn load_collision_tree(s: &mut ZoneStream<'_>) -> Result<Ptr> {
    let p = s.with_align_site(AlignWasteSite::VertList, |s| {
        s.alloc_load(4, sz::XSURFACE_COLLISION_TREE)
    })?;
    let node_count = s.u32_at(p, 24)? as usize;
    let leaf_count = s.u32_at(p, 32)? as usize;
    if always_alloc(s, p.at(28))? {
        let bytes = sz::XSURFACE_COLLISION_NODE * node_count;
        if s.diag_skip_coll_nodes_virtual() {
            s.diag_skip_stream(bytes)?;
        } else {
            s.with_align_site(AlignWasteSite::CollNodes, |s| {
                let arr = s.alloc_load(16, bytes)?;
                s.fixup_slot(p.at(28), arr)?;
                Ok(())
            })?;
        }
    }
    if always_alloc(s, p.at(36))? {
        s.with_align_site(AlignWasteSite::CollLeafs, |s| {
            let arr = s.alloc_load(2, sz::XSURFACE_COLLISION_LEAF * leaf_count)?;
            s.fixup_slot(p.at(36), arr)?;
            Ok(())
        })?;
    }
    Ok(p)
}

fn load_collmap(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    if always_alloc(s, p.at(0))? {
        let list = s.alloc_load(4, sz::PHYS_GEOM_LIST)?;
        s.fixup_slot(p.at(0), list)?;
        let count = s.u32_at(list, 0)? as usize;
        if always_alloc(s, list.at(4))? {
            s.with_align_site(AlignWasteSite::PhysGeoms, |s| {
                let arr = s.alloc_load(16, sz::PHYS_GEOM_INFO * count)?;
                s.fixup_slot(list.at(4), arr)?;
                for i in 0..count {
                    let g = arr.at(i * sz::PHYS_GEOM_INFO);
                    if s.begin_body(g.at(0))? {
                        load_brush_wrapper(s)?;
                    }
                }
                Ok(())
            })?;
        }
    }
    Ok(())
}

fn load_brush_wrapper(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.with_align_site(AlignWasteSite::BrushWrapper, |s| {
        s.alloc_load(16, sz::BRUSH_WRAPPER)
    })?;
    let numsides = s.u32_at(p, sz::BRUSH_WRAPPER_NUMSIDES_OFF)? as usize;
    let numverts = s.u32_at(p, sz::BRUSH_WRAPPER_NUMVERTS_OFF)? as usize;

    if always_alloc(s, p.at(sz::BRUSH_WRAPPER_SIDES_OFF))? {
        let sides = s.alloc_load(4, sz::CBRUSH_SIDE * numsides)?;
        s.fixup_slot(p.at(sz::BRUSH_WRAPPER_SIDES_OFF), sides)?;
        for i in 0..numsides {
            let sp = sides.at(i * sz::CBRUSH_SIDE);
            if s.begin_body(sp.at(0))? {
                s.alloc_load(4, sz::CPLANE)?;
            }
        }
    }
    s.plain_array(p, sz::BRUSH_WRAPPER_VERTS_OFF, 4, 12, numverts)?;
    s.plain_array(p, sz::BRUSH_WRAPPER_PLANES_OFF, 4, sz::CPLANE, numsides)?;
    Ok(())
}

pub(super) fn load_phys_constraints(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::PHYS_CONSTRAINTS)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let data = p.at(8);
    for i in 0..16 {
        let c = data.at(i * sz::PHYS_CONSTRAINT);
        follow_name(s, c, sz::PHYS_CONSTRAINT_BONE1_OFF)?;
        follow_name(s, c, sz::PHYS_CONSTRAINT_BONE2_OFF)?;
        asset_ptr_at(
            s,
            links,
            AssetType::Material,
            c.at(sz::PHYS_CONSTRAINT_MATERIAL_OFF),
        )?;
    }
    s.pop()
}

pub(super) fn load_xmodel_pieces(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::XMODEL_PIECES)?;
    let num_pieces = s.i32_at(p, 4)?.max(0) as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if let Some(pieces) = always_array(s, p.at(8), 4, sz::XMODEL_PIECE * num_pieces)? {
        for i in 0..num_pieces {
            asset_ptr_at(s, links, AssetType::XModel, pieces.at(i * sz::XMODEL_PIECE))?;
        }
    }
    s.pop()
}
