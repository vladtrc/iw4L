use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{FxEffectDefGeometry, Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_fx(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, sz::FX_EFFECT_DEF)?;
    let looping = s.i32_at(p, sz::FX_EFFECT_DEF_LOOPING_OFF)?.max(0) as usize;
    let one_shot = s.i32_at(p, sz::FX_EFFECT_DEF_ONESHOT_OFF)?.max(0) as usize;
    let emission = s.i32_at(p, sz::FX_EFFECT_DEF_EMISSION_OFF)?.max(0) as usize;
    let count = looping + one_shot + emission;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    let mut elem_defs = None;
    if s.begin_body(p.at(sz::FX_EFFECT_DEF_ELEMS_OFF))? {
        let arr = s.alloc_load(4, sz::FX_ELEM_DEF * count)?;
        elem_defs = Some(arr);
        for i in 0..count {
            load_fx_elem_def(s, links, arr.at(i * sz::FX_ELEM_DEF))?;
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
    let elem_type = s.u8_at(p, sz::FX_ELEM_TYPE_OFF)?;
    let visual_count = s.u8_at(p, sz::FX_ELEM_TYPE_OFF + 1)? as usize;
    let vel_count = s.u8_at(p, sz::FX_ELEM_TYPE_OFF + 2)? as usize + 1;
    let vis_count = s.u8_at(p, sz::FX_ELEM_TYPE_OFF + 3)? as usize + 1;

    s.plain_array(
        p,
        sz::FX_ELEM_VEL_SAMPLES_OFF,
        4,
        sz::FX_ELEM_VEL_STATE_SAMPLE,
        vel_count,
    )?;
    s.plain_array(
        p,
        sz::FX_ELEM_VIS_SAMPLES_OFF,
        4,
        sz::FX_ELEM_VIS_STATE_SAMPLE,
        vis_count,
    )?;

    let vis = p.at(sz::FX_ELEM_VISUALS_OFF);
    if elem_type == sz::FX_ELEM_DECAL {
        if let Some(arr) = s.follow_array(
            p,
            sz::FX_ELEM_VISUALS_OFF,
            4,
            sz::FX_ELEM_MARK_VISUALS,
            visual_count,
        )? {
            for i in 0..visual_count {
                let m = arr.at(i * sz::FX_ELEM_MARK_VISUALS);
                asset_ptr_at(s, links, AssetType::Material, m.at(0))?;
                asset_ptr_at(s, links, AssetType::Material, m.at(4))?;
            }
        }
    } else if visual_count > 1 {
        if let Some(arr) = s.follow_array(
            p,
            sz::FX_ELEM_VISUALS_OFF,
            4,
            sz::FX_ELEM_VISUALS,
            visual_count,
        )? {
            for i in 0..visual_count {
                load_fx_elem_visuals(s, links, arr.at(i * sz::FX_ELEM_VISUALS), elem_type)?;
            }
        }
    } else {
        load_fx_elem_visuals(s, links, vis, elem_type)?;
    }

    follow_name(s, p, sz::FX_ELEM_EFFECT_ON_IMPACT_OFF)?;
    follow_name(s, p, sz::FX_ELEM_EFFECT_ON_DEATH_OFF)?;
    follow_name(s, p, sz::FX_ELEM_EFFECT_EMITTED_OFF)?;
    follow_name(s, p, sz::FX_ELEM_EFFECT_ATTACHED_OFF)?;

    if s.begin_body(p.at(sz::FX_ELEM_TRAIL_DEF_OFF))? {
        let t = s.alloc_load(4, sz::FX_TRAIL_DEF)?;
        let vert_count = s.i32_at(t, 12)?.max(0) as usize;
        let ind_count = s.i32_at(t, 20)?.max(0) as usize;
        s.plain_array(t, 16, 4, sz::FX_TRAIL_VERTEX, vert_count)?;
        s.plain_array(t, 24, 2, 2, ind_count)?;
    }

    follow_name(s, p, sz::FX_ELEM_SPAWN_SOUND_OFF)?;
    Ok(())
}

fn load_fx_elem_visuals(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
    slot: Ptr,
    elem_type: u8,
) -> Result<()> {
    match elem_type {
        sz::FX_ELEM_MODEL => asset_ptr_at(s, links, AssetType::XModel, slot),
        sz::FX_ELEM_RUNNER => follow_name(s, slot, 0),
        sz::FX_ELEM_SOUND => follow_name(s, slot, 0),
        sz::FX_ELEM_OMNI_LIGHT | sz::FX_ELEM_SPOT_LIGHT => Ok(()),
        _ => asset_ptr_at(s, links, AssetType::Material, slot),
    }
}
