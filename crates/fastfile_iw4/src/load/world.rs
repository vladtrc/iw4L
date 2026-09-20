use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at, asset_ptr_at_linked, copy_linked_material, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{
    ComWorldGeometry, FxWorldGeometry, GGlassDataGeometry, GfxLightDefGeometry, MapEntsGeometry,
    Ptr, Result, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream,
};

pub(super) fn load_comworld(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::COM_WORLD, 24))?;
    let count = s.i32_at(p, s.layout(8, 12))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let primary_lights = s.follow_array(
        p,
        s.layout(12, 16),
        4,
        s.layout(sz::COM_PRIMARY_LIGHT, 72),
        count,
    )?;
    if let Some(arr) = primary_lights {
        for i in 0..count {
            follow_name(s, arr.at(i * s.layout(sz::COM_PRIMARY_LIGHT, 72)), 64)?;
        }
    }

    s.record_com_world(ComWorldGeometry {
        primary_lights,
        primary_light_count: count,
    });

    s.pop()
}

pub(super) fn load_light_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::GFX_LIGHT_DEF, 32))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    asset_ptr_at(s, links, AssetType::Image, p.at(s.layout(4, 8)))?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(n) => Some(n),
        _ => None,
    };

    let image_slot = s.ptr_at(p, s.layout(4, 8))?;
    let image_ptr = match image_slot {
        ZonePtr::Offset(img) => Some(s.resolve_alias(img)),
        _ => None,
    };
    let latest = matches!(image_slot, ZonePtr::Following | ZonePtr::Insert)
        .then(|| s.latest_image())
        .flatten();
    let attenuation_width = match image_ptr {
        Some(img) => Some(s.u16_at(img, s.layout(20, 24))?),
        None => latest.map(|g| g.width),
    };
    let attenuation_image_name = match image_ptr {
        Some(img) => match s.ptr_at(img, s.layout(28, 32))? {
            ZonePtr::Offset(n) => Some(s.resolve_alias(n)),
            _ => latest.and_then(|g| g.name),
        },
        None => latest.and_then(|g| g.name),
    };
    s.record_light_def(GfxLightDefGeometry {
        name,
        lmap_lookup_start: s.i32_at(p, s.layout(12, 24))?,
        attenuation_width,
        attenuation_image: image_ptr,
        attenuation_image_name,
        attenuation_sampler: s.u8_at(p, s.layout(8, 16))?,
    });
    s.pop()
}

pub(super) fn load_fxworld(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::FX_WORLD, 176))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let glass = p.at(s.layout(4, 8));
    let def_count = s.i32_at(glass, 8)?.max(0) as usize;
    let piece_limit = s.i32_at(glass, 12)?.max(0) as usize;
    let word_count = s.i32_at(glass, 16)?.max(0) as usize;
    let init_pieces = s.i32_at(glass, 20)?.max(0) as usize;
    let cell_count = s.i32_at(glass, 24)?.max(0) as usize;
    let geo_limit = s.i32_at(glass, 36)?.max(0) as usize;
    let init_geo = s.i32_at(glass, 44)?.max(0) as usize;

    let defs = if s.begin_body(glass.at(48))? {
        let arr = s.alloc_load(4, s.layout(sz::FX_GLASS_DEF, 48) * def_count)?;
        for i in 0..def_count {
            let d = arr.at(i * s.layout(sz::FX_GLASS_DEF, 48));

            asset_ptr_at(s, links, AssetType::PhysPreset, d.at(s.layout(32, 40)))?;
            let fresh_intact = asset_ptr_at_linked(s, links, AssetType::Material, d.at(24))?;
            let mut intact_buf = [0u8; 128];
            let intact = copy_linked_material(s, links, d.at(24), fresh_intact, &mut intact_buf);
            let fresh_shatter =
                asset_ptr_at_linked(s, links, AssetType::Material, d.at(s.layout(28, 32)))?;
            let mut shatter_buf = [0u8; 128];
            let shattered = copy_linked_material(
                s,
                links,
                d.at(s.layout(28, 32)),
                fresh_shatter,
                &mut shatter_buf,
            );
            links.capture_fx_glass_def(i, intact, shattered)?;
        }
        Some(arr)
    } else {
        match s.ptr_at(glass, 48)? {
            ZonePtr::Offset(p) => Some(s.resolve_alias(p)),
            _ => None,
        }
    };

    for (field, align, elem, count) in [
        (s.layout(52, 56), 4, sz::FX_GLASS_PIECE_PLACE, piece_limit),
        (s.layout(56, 64), 4, sz::FX_GLASS_PIECE_STATE, piece_limit),
        (
            s.layout(60, 72),
            4,
            s.layout(sz::FX_GLASS_PIECE_DYNAMICS, 48),
            piece_limit,
        ),
        (s.layout(64, 80), 4, sz::FX_GLASS_GEOMETRY_DATA, geo_limit),
        (s.layout(68, 88), 4, 4, word_count),
        (s.layout(72, 96), 4, 4, word_count * cell_count),
        (s.layout(76, 104), 16, 1, piece_limit.div_ceil(16) * 16),
        (s.layout(80, 112), 4, 12, piece_limit),
        (s.layout(84, 120), 16, 4, piece_limit.div_ceil(4) * 4),
    ] {
        runtime_array(s, glass, field, align, elem, count)?;
    }

    let init_piece_indices = s.plain_array(glass, s.layout(88, 128), 2, 2, init_pieces)?;
    let init_piece_states = s.plain_array(
        glass,
        s.layout(92, 136),
        4,
        sz::FX_GLASS_INIT_PIECE_STATE,
        init_pieces,
    )?;
    let init_geo_data = s.plain_array(
        glass,
        s.layout(96, 144),
        4,
        sz::FX_GLASS_GEOMETRY_DATA,
        init_geo,
    )?;

    s.record_fx_world(FxWorldGeometry {
        glass_sys: Some(glass),
        def_count,
        piece_limit,
        init_piece_count: init_pieces,
        init_geo_count: init_geo,
        geo_data_limit: geo_limit,
        piece_word_count: word_count,
        cell_count,
        defs,
        init_piece_indices,
        init_piece_states,
        init_geo_data,
    });

    s.pop()
}

pub(super) fn load_gameworld_mp(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::GAME_WORLD_MP, 16))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let mut geometry = GGlassDataGeometry::default();
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let glass = s.alloc_load(4, s.layout(sz::G_GLASS_DATA, 144))?;
        let piece_count = s.u32_at(glass, s.layout(4, 8))? as usize;
        let name_count = s.u32_at(glass, s.layout(12, 16))? as usize;
        let pieces = s.plain_array(glass, 0, 4, sz::G_GLASS_PIECE, piece_count)?;
        let names = if s.begin_body(glass.at(s.layout(16, 24)))? {
            let arr = s.alloc_load(4, s.layout(sz::G_GLASS_NAME, 24) * name_count)?;
            for i in 0..name_count {
                let g = arr.at(i * s.layout(sz::G_GLASS_NAME, 24));
                follow_name(s, g, 0)?;
                let count = s.u16_at(g, s.layout(6, 10))? as usize;
                s.plain_array(g, s.layout(8, 16), 2, 2, count)?;
            }
            Some(arr)
        } else {
            match s.ptr_at(glass, s.layout(16, 24))? {
                ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
                _ => None,
            }
        };
        geometry = GGlassDataGeometry {
            data: Some(glass),
            piece_count,
            name_count,
            pieces,
            names,
        };
    } else if let ZonePtr::Offset(q) = s.ptr_at(p, s.layout(4, 8))? {
        let glass = s.resolve_alias(q);
        geometry = GGlassDataGeometry {
            data: Some(glass),
            piece_count: s.u32_at(glass, s.layout(4, 8))? as usize,
            name_count: s.u32_at(glass, s.layout(12, 16))? as usize,
            pieces: match s.ptr_at(glass, 0)? {
                ZonePtr::Offset(pp) => Some(s.resolve_alias(pp)),
                _ => None,
            },
            names: match s.ptr_at(glass, s.layout(16, 24))? {
                ZonePtr::Offset(nn) => Some(s.resolve_alias(nn)),
                _ => None,
            },
        };
    }
    s.record_g_glass_data(geometry);

    s.pop()
}

/// `GameWorldSp`: name + inline `PathData` + inline `VehicleTrack` + glass ptr.
///
/// Only the stream advance matters here (single-player pathfinding and
/// vehicle data the runtime never consumes): node `Links`, tree children and
/// track branches are walked so the cursor lands past the asset. Script
/// strings inside `pathnode_constant_t` are u16 indices, never followed.
pub(super) fn load_gameworld_sp(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::GAME_WORLD_SP, 112))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    load_path_data(s, p.at(s.layout(4, 8)))?;
    load_vehicle_track(s, p.at(s.layout(44, 88)))?;
    load_glass_ptr(s, p, s.layout(48, 104))?;

    s.pop()
}

fn load_path_data(s: &mut ZoneStream<'_>, path: Ptr) -> Result<()> {
    let node_count = s.u32_at(path, 0)? as usize;
    let vis_bytes = s.i32_at(path, s.layout(24, 48))?.max(0) as usize;
    let tree_count = s.i32_at(path, s.layout(32, 64))?.max(0) as usize;
    let node_stride = s.layout(sz::PATH_NODE, 168);

    if let Some(nodes) = s.follow_array(path, s.layout(4, 8), 4, node_stride, node_count)? {
        let link_field = s.layout(60, 64);
        for i in 0..node_count {
            let node = nodes.at(i * node_stride);
            let link_count = s.u16_at(node, 56)? as usize;
            s.follow_array(node, link_field, 4, sz::PATH_LINK, link_count)?;
        }
    }

    // basenodes live in the runtime block: zero-filled, no stream bytes.
    runtime_array(s, path, s.layout(8, 16), 16, sz::PATH_BASENODE, node_count)?;

    s.plain_array(path, s.layout(12, 32), 2, 2, node_count)?;
    s.plain_array(path, s.layout(16, 40), 2, 2, node_count)?;
    s.plain_array(path, s.layout(20, 56), 1, 1, vis_bytes)?;

    if let Some(tree) = s.follow_array(
        path,
        s.layout(28, 72),
        4,
        s.layout(sz::PATHNODE_TREE, 24),
        tree_count,
    )? {
        for i in 0..tree_count {
            load_pathnode_tree(s, tree.at(i * s.layout(sz::PATHNODE_TREE, 24)))?;
        }
    }
    Ok(())
}

fn load_pathnode_tree(s: &mut ZoneStream<'_>, t: Ptr) -> Result<()> {
    let axis = s.i32_at(t, 0)?;
    let union_off = s.layout(8, 8);
    if axis >= 0 {
        let width = s.pointer_bytes();
        for k in 0..2 {
            if s.begin_body(t.at(union_off + k * width))? {
                let child = s.alloc_load(4, s.layout(sz::PATHNODE_TREE, 24))?;
                load_pathnode_tree(s, child)?;
            }
        }
    } else {
        let count = s.i32_at(t, union_off)?.max(0) as usize;
        s.follow_array(t, s.layout(12, 16), 2, 2, count)?;
    }
    Ok(())
}

fn load_vehicle_track(s: &mut ZoneStream<'_>, track: Ptr) -> Result<()> {
    let segment_count = s.u32_at(track, s.layout(4, 8))? as usize;
    if let Some(segments) = s.follow_array(
        track,
        0,
        4,
        s.layout(sz::VEHICLE_SEGMENT, 72),
        segment_count,
    )? {
        for i in 0..segment_count {
            load_vehicle_segment(s, segments.at(i * s.layout(sz::VEHICLE_SEGMENT, 72)))?;
        }
    }
    Ok(())
}

fn load_vehicle_segment(s: &mut ZoneStream<'_>, seg: Ptr) -> Result<()> {
    follow_name(s, seg, 0)?;
    let sector_count = s.u32_at(seg, s.layout(8, 16))? as usize;
    if let Some(sectors) = s.follow_array(
        seg,
        s.layout(4, 8),
        4,
        s.layout(sz::VEHICLE_SECTOR, 72),
        sector_count,
    )? {
        for i in 0..sector_count {
            let sector = sectors.at(i * s.layout(sz::VEHICLE_SECTOR, 72));
            let obstacle_count = s.u32_at(sector, s.layout(56, 64))? as usize;
            s.plain_array(
                sector,
                s.layout(52, 56),
                4,
                sz::VEHICLE_OBSTACLE,
                obstacle_count,
            )?;
        }
    }
    // Branch lists hold pointers to fellow segments (shared ⇒ bare offsets).
    for (field, count_off) in [
        (s.layout(12, 24), s.layout(16, 32)),
        (s.layout(20, 40), s.layout(24, 48)),
    ] {
        let branch_count = s.u32_at(seg, count_off)? as usize;
        if let Some(branches) = s.follow_array(
            seg,
            field,
            s.pointer_bytes(),
            s.pointer_bytes(),
            branch_count,
        )? {
            for k in 0..branch_count {
                if s.begin_body(branches.at(k * s.pointer_bytes()))? {
                    let target = s.alloc_load(4, s.layout(sz::VEHICLE_SEGMENT, 72))?;
                    load_vehicle_segment(s, target)?;
                }
            }
        }
    }
    Ok(())
}

/// Pointer form of `G_GlassData`, shared with `GameWorldMp`'s inline variant.
fn load_glass_ptr(s: &mut ZoneStream<'_>, p: Ptr, field: usize) -> Result<()> {
    if !s.begin_body(p.at(field))? {
        return Ok(());
    }
    let glass = s.alloc_load(4, s.layout(sz::G_GLASS_DATA, 144))?;
    let piece_count = s.u32_at(glass, s.layout(4, 8))? as usize;
    let name_count = s.u32_at(glass, s.layout(12, 16))? as usize;
    s.plain_array(glass, 0, 4, sz::G_GLASS_PIECE, piece_count)?;
    if s.begin_body(glass.at(s.layout(16, 24)))? {
        let stride = s.layout(sz::G_GLASS_NAME, 24);
        let names = s.alloc_load(4, stride * name_count)?;
        for i in 0..name_count {
            let g = names.at(i * stride);
            follow_name(s, g, 0)?;
            let count = s.u16_at(g, s.layout(6, 10))? as usize;
            s.plain_array(g, s.layout(8, 16), 2, 2, count)?;
        }
    }
    Ok(())
}

/// `AddonMapEnts`: the `MapEnts` head (name, entity string, triggers) with no
/// stages. Exists so small ops zones (`so_*`, asset 0) walk past it.
pub(super) fn load_addonmapents(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::ADDON_MAP_ENTS, 72))?;
    let entity_chars = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    s.plain_array(p, s.layout(4, 8), 1, 1, entity_chars)?;

    let triggers = p.at(s.layout(12, 24));
    let model_count = s.i32_at(triggers, 0)?.max(0) as usize;
    let hull_count = s.i32_at(triggers, s.layout(8, 16))?.max(0) as usize;
    let slab_count = s.i32_at(triggers, s.layout(16, 32))?.max(0) as usize;
    s.plain_array(triggers, s.layout(4, 8), 4, sz::TRIGGER_MODEL, model_count)?;
    s.plain_array(triggers, s.layout(12, 24), 4, sz::TRIGGER_HULL, hull_count)?;
    s.plain_array(triggers, s.layout(20, 40), 4, sz::TRIGGER_SLAB, slab_count)?;

    s.pop()
}

pub(super) fn load_mapents(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::MAP_ENTS, 88))?;
    let entity_chars = s.i32_at(p, s.layout(8, 16))?.max(0) as usize;
    let stage_count = s.u8_at(p, s.layout(40, 80))? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let entity_string = s.plain_array(p, s.layout(4, 8), 1, 1, entity_chars)?;

    let triggers = p.at(s.layout(12, 24));
    let model_count = s.i32_at(triggers, 0)?.max(0) as usize;
    let hull_count = s.i32_at(triggers, s.layout(8, 16))?.max(0) as usize;
    let slab_count = s.i32_at(triggers, s.layout(16, 32))?.max(0) as usize;
    let trigger_models =
        s.plain_array(triggers, s.layout(4, 8), 4, sz::TRIGGER_MODEL, model_count)?;
    let trigger_hulls =
        s.plain_array(triggers, s.layout(12, 24), 4, sz::TRIGGER_HULL, hull_count)?;
    let trigger_slabs =
        s.plain_array(triggers, s.layout(20, 40), 4, sz::TRIGGER_SLAB, slab_count)?;
    s.record_map_ents(MapEntsGeometry {
        entity_string,
        entity_chars,
        trigger_models,
        trigger_model_count: model_count,
        trigger_hulls,
        trigger_hull_count: hull_count,
        trigger_slabs,
        trigger_slab_count: slab_count,
    });

    if s.begin_body(p.at(s.layout(36, 72)))? {
        let arr = s.alloc_load(4, s.layout(sz::STAGE, 24) * stage_count)?;
        for i in 0..stage_count {
            follow_name(s, arr.at(i * s.layout(sz::STAGE, 24)), 0)?;
        }
    }

    s.pop()
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
