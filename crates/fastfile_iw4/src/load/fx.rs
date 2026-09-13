use asset_iw4::size::{self as sz, fx_elem};

use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{FxEffectDefGeometry, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::FX_EFFECT_DEF, 40))?;

    let looping = s.i32_at(p, s.layout(16, 20))?.max(0) as usize;
    let one_shot = s.i32_at(p, s.layout(20, 24))?.max(0) as usize;
    let emission = s.i32_at(p, s.layout(24, 28))?.max(0) as usize;
    let count = looping + one_shot + emission;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let mut elem_defs = None;
    if s.begin_body(p.at(s.layout(28, 32)))? {
        let arr = s.alloc_load(4, s.layout(sz::FX_ELEM_DEF, 288) * count)?;
        elem_defs = Some(arr);
        for i in 0..count {
            load_fx_elem_def(s, links, arr.at(i * s.layout(sz::FX_ELEM_DEF, 288)))?;
        }
    }

    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => None,
    };
    links.capture_fx(
        s,
        FxEffectDefGeometry {
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
    let elem_type = s.u8_at(p, 176)?;
    let visual_count = s.u8_at(p, 177)? as usize;

    let vel_count = s.u8_at(p, 178)? as usize + 1;
    let vis_count = s.u8_at(p, 179)? as usize + 1;
    s.plain_array(
        p,
        s.layout(180, 184),
        4,
        sz::FX_ELEM_VEL_STATE_SAMPLE,
        vel_count,
    )?;
    s.plain_array(
        p,
        s.layout(184, 192),
        4,
        sz::FX_ELEM_VIS_STATE_SAMPLE,
        vis_count,
    )?;

    let vis = p.at(s.layout(188, 200));
    if elem_type == fx_elem::DECAL {
        if let Some(arr) = s.follow_array(
            p,
            s.layout(188, 200),
            4,
            s.layout(sz::FX_ELEM_MARK_VISUALS, 16),
            visual_count,
        )? {
            for i in 0..visual_count {
                let m = arr.at(i * s.layout(sz::FX_ELEM_MARK_VISUALS, 16));
                asset_ptr_at(s, links, AssetType::Material, m.at(0))?;
                asset_ptr_at(s, links, AssetType::Material, m.at(s.layout(4, 8)))?;
            }
        }
    } else if visual_count > 1 {
        if let Some(arr) = s.follow_array(
            p,
            s.layout(188, 200),
            4,
            s.layout(sz::FX_ELEM_VISUALS, 8),
            visual_count,
        )? {
            for i in 0..visual_count {
                load_fx_elem_visuals(
                    s,
                    links,
                    arr.at(i * s.layout(sz::FX_ELEM_VISUALS, 8)),
                    elem_type,
                )?;
            }
        }
    } else {
        load_fx_elem_visuals(s, links, vis, elem_type)?;
    }

    follow_name(s, p, s.layout(216, 232))?;
    follow_name(s, p, s.layout(220, 240))?;
    follow_name(s, p, s.layout(224, 248))?;

    match elem_type {
        fx_elem::TRAIL => {
            if s.begin_body(p.at(s.layout(244, 272)))? {
                let t = s.alloc_load(4, s.layout(sz::FX_TRAIL_DEF, 48))?;

                s.fixup_slot(p.at(s.layout(244, 272)), t)?;
                let vert_count = s.i32_at(t, 20)?.max(0) as usize;
                let ind_count = s.i32_at(t, s.layout(28, 32))?.max(0) as usize;
                s.plain_array(t, 24, 4, sz::FX_TRAIL_VERTEX, vert_count)?;
                s.plain_array(t, s.layout(32, 40), 2, 2, ind_count)?;
            }
        }
        fx_elem::SPARK_FOUNTAIN => {
            if s.begin_body(p.at(s.layout(244, 272)))? {
                let b = s.alloc_load(4, sz::FX_SPARK_FOUNTAIN_DEF)?;
                s.fixup_slot(p.at(s.layout(244, 272)), b)?;
            }
        }
        _ => {
            if s.begin_body(p.at(s.layout(244, 272)))? {
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
    } else {
        Ok(())
    }
}
