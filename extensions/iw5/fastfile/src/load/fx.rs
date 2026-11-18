use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::size::fx_elem;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::FX_EFFECT_DEF, 64))?;
    let looping = s.i32_at(p, s.layout(sz::FX_EFFECT_LOOPING_OFF, 20))?.max(0) as usize;
    let one_shot = s.i32_at(p, s.layout(sz::FX_EFFECT_ONESHOT_OFF, 24))?.max(0) as usize;
    let emission = s
        .i32_at(p, s.layout(sz::FX_EFFECT_EMISSION_OFF, 28))?
        .max(0) as usize;
    let count = looping + one_shot + emission;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let mut elem_defs = None;
    if s.begin_body(p.at(s.layout(sz::FX_EFFECT_ELEMS_OFF, 56)))? {
        let elem = s.layout(sz::FX_ELEM_DEF, 288);
        let arr = s.alloc_load(4, elem * count)?;
        elem_defs = Some(arr);
        for i in 0..count {
            load_fx_elem_def(s, links, arr.at(i * elem))?;
        }
    }
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    links.capture_fx(
        s,
        crate::zone::FxEffectDefGeometry {
            header: p,
            name,
            looping_count: looping as i32,
            one_shot_count: one_shot as i32,
            emission_count: emission as i32,
            elem_defs,
            elem_def_count: count,
        },
    )?;
    s.pop()
}

fn load_fx_elem_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    let elem_type = s.u8_at(p, sz::FX_ELEM_TYPE_OFF)?;
    let visual_count = s.u8_at(p, sz::FX_ELEM_TYPE_OFF + 1)? as usize;
    let vel_count = s.u8_at(p, sz::FX_ELEM_TYPE_OFF + 2)? as usize + 1;
    let vis_count = s.u8_at(p, sz::FX_ELEM_TYPE_OFF + 3)? as usize + 1;

    s.plain_array(
        p,
        s.layout(sz::FX_ELEM_VEL_SAMPLES_OFF, 184),
        4,
        sz::FX_ELEM_VEL_STATE_SAMPLE,
        vel_count,
    )?;
    s.plain_array(
        p,
        s.layout(sz::FX_ELEM_VIS_SAMPLES_OFF, 192),
        4,
        sz::FX_ELEM_VIS_STATE_SAMPLE,
        vis_count,
    )?;

    let vis = p.at(s.layout(sz::FX_ELEM_VISUALS_OFF, 200));
    if elem_type == fx_elem::DECAL {
        if s.begin_body(vis)? {
            let mark = s.layout(sz::FX_ELEM_MARK_VISUALS, 16);
            let arr = s.alloc_load(4, mark * visual_count)?;
            for i in 0..visual_count {
                let m = arr.at(i * mark);
                asset_ptr_at(s, links, AssetType::Material, m.at(0))?;
                asset_ptr_at(s, links, AssetType::Material, m.at(s.layout(4, 8)))?;
            }
        }
    } else if visual_count > 1 {
        if s.begin_body(vis)? {
            let visual = s.layout(sz::FX_ELEM_VISUALS, 8);
            let arr = s.alloc_load(4, visual * visual_count)?;
            for i in 0..visual_count {
                load_fx_elem_visuals(s, links, arr.at(i * visual), elem_type)?;
            }
        }
    } else {
        load_fx_elem_visuals(s, links, vis, elem_type)?;
    }

    follow_name(s, p, s.layout(sz::FX_ELEM_EFFECT_ON_IMPACT_OFF, 232))?;
    follow_name(s, p, s.layout(sz::FX_ELEM_EFFECT_ON_DEATH_OFF, 240))?;
    follow_name(s, p, s.layout(sz::FX_ELEM_EFFECT_EMITTED_OFF, 248))?;

    let extended = p.at(s.layout(sz::FX_ELEM_EXTENDED_OFF, 272));
    match elem_type {
        fx_elem::TRAIL => {
            if s.begin_body(extended)? {
                let t = s.alloc_load(4, s.layout(sz::FX_TRAIL_DEF, 48))?;
                let vert_count = s.i32_at(t, sz::FX_TRAIL_VERT_COUNT_OFF)?.max(0) as usize;
                let ind_count = s
                    .i32_at(t, s.layout(sz::FX_TRAIL_IND_COUNT_OFF, 32))?
                    .max(0) as usize;
                s.plain_array(
                    t,
                    sz::FX_TRAIL_VERTS_OFF,
                    4,
                    sz::FX_TRAIL_VERTEX,
                    vert_count,
                )?;
                s.plain_array(t, s.layout(sz::FX_TRAIL_INDS_OFF, 40), 2, 2, ind_count)?;
            }
        }
        fx_elem::SPARK_FOUNTAIN => {
            if s.begin_body(extended)? {
                s.alloc_load(4, sz::FX_SPARK_FOUNTAIN_DEF)?;
            }
        }
        fx_elem::SPOT_LIGHT => {
            if s.begin_body(extended)? {
                s.alloc_load(4, sz::FX_SPOT_LIGHT_DEF)?;
            }
        }
        _ => {
            if s.begin_body(extended)? {
                s.alloc_load(1, 1)?;
            }
        }
    }
    Ok(())
}

fn load_fx_elem_visuals(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    p: Ptr,
    elem_type: u8,
) -> Result<()> {
    if fx_elem::is_sprite(elem_type) {
        asset_ptr_at(s, links, AssetType::Material, p)
    } else if elem_type == fx_elem::MODEL {
        asset_ptr_at(s, links, AssetType::XModel, p)
    } else if elem_type == fx_elem::RUNNER || elem_type == fx_elem::SOUND {
        follow_name(s, p, 0)
    } else if elem_type == fx_elem::SPOT_LIGHT {
        asset_ptr_at(s, links, AssetType::LightDef, p)
    } else {
        Ok(())
    }
}

pub(super) fn load_impact_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::FX_IMPACT_TABLE, 16))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let width = s.pointer_bytes();
        let entry = s.layout(sz::FX_IMPACT_ENTRY, 280);
        let arr = s.alloc_load(4, entry * 15)?;
        for i in 0..15 {
            let e = arr.at(i * entry);
            for j in 0..sz::SURF_TYPE_NUM {
                asset_ptr_at(s, links, AssetType::Fx, e.at(j * width))?;
            }
            for j in 0..4 {
                asset_ptr_at(
                    s,
                    links,
                    AssetType::Fx,
                    e.at(sz::SURF_TYPE_NUM * width + j * width),
                )?;
            }
        }
    }
    s.pop()
}

pub(super) fn load_surface_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::SURFACE_FX_TABLE, 16))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if s.begin_body(p.at(s.layout(4, 8)))? {
        let width = s.pointer_bytes();
        let entry = s.layout(sz::SURFACE_FX_ENTRY, 248);
        let arr = s.alloc_load(4, entry * 6)?;
        for i in 0..6 {
            let e = arr.at(i * entry);
            for j in 0..sz::SURF_TYPE_NUM {
                asset_ptr_at(s, links, AssetType::Fx, e.at(j * width))?;
            }
        }
    }
    s.pop()
}
