use super::{AssetLinkSink, always_alloc, always_array, asset_ptr_at, follow_name, runtime_array};
use crate::ZonePtr;
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{ComWorldGeometry, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_comworld(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::COM_WORLD)?;
    let light_count = s.u32_at(p, 8)? as usize;
    let water_count = s.u32_at(p, 32)? as usize;
    let burn_count = s.u32_at(p, 56)? as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let primary_lights = if always_alloc(s, p.at(12))? {
        let arr = s.alloc_load(4, sz::COM_PRIMARY_LIGHT * light_count)?;
        s.fixup_slot(p.at(12), arr)?;
        for i in 0..light_count {
            follow_name(
                s,
                arr.at(i * sz::COM_PRIMARY_LIGHT),
                sz::COM_PRIMARY_LIGHT_DEF_NAME_OFF,
            )?;
        }
        Some(arr)
    } else {
        None
    };

    if always_alloc(s, p.at(36))? {
        let arr = s.alloc_load(4, sz::COM_WATER_CELL * water_count)?;
        s.fixup_slot(p.at(36), arr)?;
    }

    if always_alloc(s, p.at(60))? {
        let arr = s.alloc_load(4, sz::COM_BURNABLE_CELL * burn_count)?;
        s.fixup_slot(p.at(60), arr)?;
        for i in 0..burn_count {
            let cell = arr.at(i * sz::COM_BURNABLE_CELL);
            if always_alloc(s, cell.at(8))? {
                let data = s.alloc_load(1, 32)?;
                s.fixup_slot(cell.at(8), data)?;
            }
        }
    }

    s.record_com_world(ComWorldGeometry {
        primary_lights,
        primary_light_count: light_count,
    });
    s.pop()
}

pub(super) fn load_light_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::GFX_LIGHT_DEF)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let image_inline = matches!(s.ptr_at(p, 4)?, ZonePtr::Following | ZonePtr::Insert);
    asset_ptr_at(s, links, AssetType::Image, p.at(4))?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(n) => Some(s.resolve_alias(n)),
        _ => None,
    };
    let (attenuation_image_name, attenuation_width) = if image_inline {
        s.latest_image()
            .map(|img| (img.name, Some(img.width)))
            .unwrap_or((None, None))
    } else if let ZonePtr::Offset(img) = s.ptr_at(p, 4)? {
        let img = s.resolve_alias(img);
        let name = match s.ptr_at(img, sz::GFX_IMAGE_NAME_OFF)? {
            ZonePtr::Offset(n) => Some(s.resolve_alias(n)),
            _ => None,
        };
        (name, Some(s.u16_at(img, 20)?))
    } else {
        (None, None)
    };
    s.record_light_def(crate::zone::GfxLightDefGeometry {
        name,
        attenuation_image_name,
        attenuation_width,
        attenuation_sampler: s.u8_at(p, 8)?,
        lmap_lookup_start: s.i32_at(p, 12)?,
    })?;
    s.pop()
}

pub(super) fn load_game_world_mp(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::GAME_WORLD_MP)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    load_path_data(s, p.at(sz::PATH_DATA_OFF))?;
    s.pop()
}

/// `GameWorldSp`: same `{ name, PathData }` shell as the MP variant (OAT
/// `GameWorldSp.txt` walks the identical `PathData`, including the
/// `nodeCount + 128` overallocation).
pub(super) fn load_game_world_sp(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, sz::GAME_WORLD_SP)?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    load_path_data(s, p.at(sz::PATH_DATA_OFF))?;
    s.pop()
}

fn load_path_data(s: &mut ZoneStream<'_>, p: Ptr) -> Result<()> {
    let node_count = s.u32_at(p, 0)? as usize;
    let node_alloc = node_count + 128;
    let vis_bytes = s.i32_at(p, 24)?.max(0) as usize;
    let node_tree_count = s.i32_at(p, 32)?.max(0) as usize;

    if let Some(nodes) = always_array(s, p.at(4), 4, sz::PATH_NODE * node_alloc)? {
        for i in 0..node_alloc {
            let node = nodes.at(i * sz::PATH_NODE);
            let link_count = s.u16_at(node, sz::PATH_NODE_TOTAL_LINK_COUNT_OFF)? as usize;
            always_array(
                s,
                node.at(sz::PATH_NODE_LINKS_OFF),
                4,
                sz::PATH_LINK * link_count,
            )?;
        }
    }

    runtime_array(s, p.at(8), 16, sz::PATH_BASE_NODE * node_alloc)?;
    always_array(s, p.at(16), 2, 2 * node_count)?;
    always_array(s, p.at(20), 2, 2 * node_count)?;
    always_array(s, p.at(28), 1, vis_bytes)?;

    if let Some(trees) = always_array(s, p.at(36), 4, sz::PATH_NODE_TREE * node_tree_count)? {
        for i in 0..node_tree_count {
            load_path_node_tree(s, trees.at(i * sz::PATH_NODE_TREE))?;
        }
    }
    Ok(())
}

fn load_path_node_tree(s: &mut ZoneStream<'_>, tree: Ptr) -> Result<()> {
    let axis = s.i32_at(tree, 0)?;
    let info = tree.at(8);
    if axis < 0 {
        let node_count = s.i32_at(info, 0)?.max(0) as usize;
        always_array(s, info.at(4), 2, 2 * node_count)?;
    } else {
        for i in 0..2 {
            let slot = info.at(i * 4);
            if s.begin_body(slot)? {
                let child = s.alloc_load(4, sz::PATH_NODE_TREE)?;
                s.fixup_slot(slot, child)?;
                load_path_node_tree(s, child)?;
            }
        }
    }
    Ok(())
}
