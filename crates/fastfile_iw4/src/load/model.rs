use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::{
    asset_type::AssetType,
    zone::{
        Ptr, Result, XFILE_BLOCK_INDEX, XFILE_BLOCK_VERTEX, XFILE_BLOCK_VIRTUAL, XModelGeometry,
        ZonePtr, ZoneStream,
    },
};

pub(super) fn load_xmodel(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::XMODEL, 408))?;

    let num_bones = s.u8_at(p, s.layout(4, 8))? as usize;
    let num_root_bones = s.u8_at(p, s.layout(5, 9))? as usize;
    let num_surfs = s.u8_at(p, s.layout(6, 10))? as usize;
    let num_coll_surfs = s.i32_at(p, s.layout(248, 336))?.max(0) as usize;

    let num_child = num_bones.saturating_sub(num_root_bones);

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    s.plain_array(p, s.layout(36, 40), 2, 2, num_bones)?;
    s.plain_array(p, s.layout(40, 48), 1, 1, num_child)?;
    s.plain_array(p, s.layout(44, 56), 2, sz::XMODEL_QUAT, num_child)?;
    s.plain_array(p, s.layout(48, 64), 4, 4, num_child * 3)?;
    s.plain_array(p, s.layout(52, 72), 1, 1, num_bones)?;
    let part_classification = match s.ptr_at(p, s.layout(52, 72))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    s.plain_array(p, s.layout(56, 80), 4, sz::DOBJ_ANIM_MAT, num_bones)?;

    let handles = match s.ptr_at(p, s.layout(60, 88))? {
        ptr if ptr.is_following() => Some(s.alloc_load(4, s.pointer_bytes() * num_surfs)?),
        ZonePtr::Offset(arr) => {
            s.note_offset(arr);
            Some(s.resolve_alias(arr))
        }
        _ => None,
    };
    if let Some(arr) = handles {
        for i in 0..num_surfs {
            asset_ptr_at(s, links, AssetType::Material, arr.at(i * s.pointer_bytes()))?;
        }
    }

    let mut lod_xsurfaces = [None; 4];
    let mut lod_surface_names = [None; 4];
    for i in 0..4 {
        (lod_xsurfaces[i], lod_surface_names[i]) = load_lod_info(
            s,
            links,
            p.at(s.layout(64, 96) + i * s.layout(sz::XMODEL_LOD_INFO, 56)),
        )?;
    }
    let mut lod_numsurfs = [0u16; 4];
    let mut lod_surf_index = [0u16; 4];
    for lod in 0..4 {
        let lp = p.at(s.layout(64, 96) + lod * s.layout(sz::XMODEL_LOD_INFO, 56));
        lod_numsurfs[lod] = s.u16_at(lp, 4)?;
        lod_surf_index[lod] = s.u16_at(lp, 6)?;
    }

    let mut coll_surfs = None;
    if s.begin_body(p.at(s.layout(244, 328)))? {
        let arr = s.alloc_load(4, s.layout(sz::XMODEL_COLL_SURF, 48) * num_coll_surfs)?;
        coll_surfs = Some(arr);
        for i in 0..num_coll_surfs {
            let cp = arr.at(i * s.layout(sz::XMODEL_COLL_SURF, 48));
            let tri_count = s.i32_at(cp, s.layout(4, 8))?.max(0) as usize;
            s.plain_array(cp, 0, 4, sz::XMODEL_COLL_TRI, tri_count)?;
        }
    }

    s.plain_array(p, s.layout(256, 344), 4, sz::XBONE_INFO, num_bones)?;
    let bone_info = match s.ptr_at(p, s.layout(256, 344))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };

    asset_ptr_at(s, links, AssetType::PhysPreset, p.at(s.layout(296, 392)))?;
    asset_ptr_at(s, links, AssetType::PhysCollmap, p.at(s.layout(300, 400)))?;

    let phys_preset = match s.ptr_at(p, s.layout(296, 392))? {
        ZonePtr::Null => None,
        _ => Some(p.at(s.layout(296, 392))),
    };

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    let bone_names = match s.ptr_at(p, s.layout(36, 40))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let parent_list = match s.ptr_at(p, s.layout(40, 48))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let quats = match s.ptr_at(p, s.layout(44, 56))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let trans = match s.ptr_at(p, s.layout(48, 64))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let base_mat = match s.ptr_at(p, s.layout(56, 80))? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    };
    let mut no_scale_part_bits = [0u32; 6];
    for (i, word) in no_scale_part_bits.iter_mut().enumerate() {
        *word = s.u32_at(p, s.layout(12, 16) + i * 4)?;
    }
    s.record_xmodel(XModelGeometry {
        name,
        material_handles: handles,
        material_handle_count: num_surfs,
        surfaces: lod_xsurfaces[0],
        surface_count: lod_numsurfs[0] as usize,
        lod_xsurfaces,
        lod_surface_names,
        lod_numsurfs,
        lod_surf_index,
        num_bones,
        num_root_bones,
        scale: s.f32_at(p, s.layout(8, 12))?,
        no_scale_part_bits,
        bone_names,
        parent_list,
        quats,
        trans,
        base_mat,
        part_classification,
        bone_info,
        coll_surfs,
        num_coll_surfs: num_coll_surfs as i32,

        coll_lod: i16::from(s.u8_at(p, s.layout(0xf2, 322))? as i8),
        lod_dist: [
            s.f32_at(p, s.layout(0x40, 96))?,
            s.f32_at(p, s.layout(0x40, 96) + s.layout(sz::XMODEL_LOD_INFO, 56))?,
            s.f32_at(
                p,
                s.layout(0x40, 96) + 2 * s.layout(sz::XMODEL_LOD_INFO, 56),
            )?,
            s.f32_at(
                p,
                s.layout(0x40, 96) + 3 * s.layout(sz::XMODEL_LOD_INFO, 56),
            )?,
        ],
        lod_part_bits: {
            let mut rows = [[0u32; 6]; 4];
            for (lod, row) in rows.iter_mut().enumerate() {
                let base = s.layout(0x40, 96)
                    + lod * s.layout(sz::XMODEL_LOD_INFO, 56)
                    + s.layout(0x0c, 16);
                for (i, word) in row.iter_mut().enumerate() {
                    *word = s.u32_at(p, base + i * 4)?;
                }
            }
            rows
        },
        lod_smc: {
            let mut rows = [[0u8; 4]; 4];
            for (lod, row) in rows.iter_mut().enumerate() {
                let base = s.layout(0x40, 96)
                    + lod * s.layout(sz::XMODEL_LOD_INFO, 56)
                    + s.layout(0x28, 48);
                *row = [
                    s.u8_at(p, base)?,
                    s.u8_at(p, base + 1)?,
                    s.u8_at(p, base + 2)?,
                    s.u8_at(p, base + 3)?,
                ];
            }
            rows
        },
        num_lods: s.u8_at(p, s.layout(0xf1, 321))?,
        lod_start: s.u8_at(p, s.layout(0xf0, 320))?,
        contents: s.u32_at(p, s.layout(0xfc, 340))?,
        radius: Some(s.f32_at(p, s.layout(0x104, 352))?),
        bounds_mid: Some([
            s.f32_at(p, s.layout(0x108, 356))?,
            s.f32_at(p, s.layout(0x10c, 360))?,
            s.f32_at(p, s.layout(0x110, 364))?,
        ]),
        bounds_half: Some([
            s.f32_at(p, s.layout(0x114, 368))?,
            s.f32_at(p, s.layout(0x118, 372))?,
            s.f32_at(p, s.layout(0x11c, 376))?,
        ]),
        phys_preset,
    });

    s.pop()
}

fn load_lod_info(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    lp: Ptr,
) -> Result<(Option<Ptr>, Option<Ptr>)> {
    let num_surfs = s.u16_at(lp, 4)? as usize;

    s.push(crate::zone::XFILE_BLOCK_TEMP)?;
    let (load, insert_slot) = s.begin_body_with_insert(lp.at(8))?;
    let surfaces = if load {
        let body = s.next_ptr(4)?;
        let surfaces = load_xmodel_surfs(s, num_surfs)?;
        s.fixup_slot(lp.at(8), body)?;
        if let (Some(slot), Some(array)) = (insert_slot, surfaces) {
            links.remember_xmodel_surfaces(slot, array);
        }
        let name = match s.ptr_at(body, 0)? {
            ZonePtr::Offset(name) => Some(name),
            _ => None,
        };
        if let (Some(slot), Some(name)) = (insert_slot, name) {
            links.remember_xmodel_surface_name(slot, name);
        }
        (surfaces, name)
    } else {
        match s.ptr_at(lp, 8)? {
            ZonePtr::Offset(model_surfs) => (
                links.xmodel_surfaces(model_surfs),
                links.xmodel_surface_name(model_surfs),
            ),
            _ => (None, None),
        }
    };
    s.pop()?;
    Ok(surfaces)
}

fn load_xmodel_surfs(s: &mut ZoneStream<'_>, num_surfs: usize) -> Result<Option<Ptr>> {
    let p = s.alloc_load(4, s.layout(sz::XMODEL_SURFS, 48))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let surfaces = if s.begin_body(p.at(s.layout(4, 8)))? {
        let arr = s.alloc_load(4, s.layout(sz::XSURFACE, 88) * num_surfs)?;
        s.fixup_slot(p.at(s.layout(4, 8)), arr)?;
        for i in 0..num_surfs {
            load_xsurface(s, arr.at(i * s.layout(sz::XSURFACE, 88)))?;
        }
        Some(arr)
    } else {
        None
    };

    s.pop()?;
    Ok(surfaces)
}

fn load_xsurface(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let vert_count = s.u16_at(p, 2)? as usize;
    let tri_count = s.u16_at(p, 4)? as usize;
    let vert_list_count = s.u32_at(p, s.layout(32, 48))? as usize;

    let vi = p.at(s.layout(16, 24));
    let blend_len = s.i16_at(vi, 0)?.max(0) as usize
        + 3 * s.i16_at(vi, 2)?.max(0) as usize
        + 5 * s.i16_at(vi, 4)?.max(0) as usize
        + 7 * s.i16_at(vi, 6)?.max(0) as usize;
    s.plain_array(vi, 8, 2, 2, blend_len)?;

    s.push(XFILE_BLOCK_VERTEX)?;
    s.plain_array(p, s.layout(28, 40), 16, sz::GFX_PACKED_VERTEX, vert_count)?;
    s.pop()?;

    if s.begin_body(p.at(s.layout(36, 56)))? {
        let arr = s.alloc_load(4, s.layout(sz::XRIGID_VERT_LIST, 16) * vert_list_count)?;
        s.fixup_slot(p.at(s.layout(36, 56)), arr)?;
        for i in 0..vert_list_count {
            let b = arr.at(i * s.layout(sz::XRIGID_VERT_LIST, 16));
            if s.begin_body(b.at(8))? {
                let tree = load_collision_tree(s)?;
                s.fixup_slot(b.at(8), tree)?;
            }
        }
    }

    s.push(XFILE_BLOCK_INDEX)?;
    s.plain_array(p, s.layout(12, 16), 16, sz::XSURFACE_TRI16, tri_count)?;
    s.pop()
}

fn load_collision_tree(s: &mut ZoneStream<'_>) -> Result<Ptr> {
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
    Ok(p)
}
