use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{
    ComWorldGeometry, GfxLightDefGeometry, Ptr, Result, XFILE_BLOCK_RUNTIME, XFILE_BLOCK_VIRTUAL,
    ZonePtr, ZoneStream,
};

pub(super) fn load_light_def(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::GFX_LIGHT_DEF, 48))?;
    let image_off = s.layout(sz::GFX_LIGHT_DEF_IMAGE_OFF, 8);
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    asset_ptr_at(s, links, AssetType::Image, p.at(image_off))?;
    let name = match s.ptr_at(p, 0)? {
        ZonePtr::Offset(n) => Some(n),
        _ => None,
    };
    let image_slot = s.ptr_at(p, image_off)?;
    let image_ptr = match image_slot {
        ZonePtr::Offset(img) => Some(s.resolve_alias(img)),
        _ => None,
    };
    let atten_latest = matches!(image_slot, ZonePtr::Following | ZonePtr::Insert)
        .then(|| s.latest_image())
        .flatten();
    asset_ptr_at(
        s,
        links,
        AssetType::Image,
        p.at(s.layout(sz::GFX_LIGHT_DEF_CUCOLORIS_IMAGE_OFF, 24)),
    )?;
    let attenuation_width = match image_ptr {
        Some(img) => Some(s.u16_at(img, s.layout(20, 24))?),
        None => atten_latest.map(|g| g.width),
    };
    let attenuation_image_name = match image_ptr {
        Some(img) => match s.ptr_at(img, s.layout(sz::GFX_IMAGE_NAME_OFF, 32))? {
            ZonePtr::Offset(n) => Some(s.resolve_alias(n)),
            _ => atten_latest.and_then(|g| g.name),
        },
        None => atten_latest.and_then(|g| g.name),
    };
    s.record_light_def(GfxLightDefGeometry {
        name,
        lmap_lookup_start: s.i32_at(p, s.layout(sz::GFX_LIGHT_DEF_LMAP_LOOKUP_OFF, 40))?,
        attenuation_width,
        attenuation_image: image_ptr,
        attenuation_image_name,
        attenuation_sampler: s.u8_at(p, s.layout(sz::GFX_LIGHT_DEF_SAMPLER_OFF, 16))?,
    });
    s.pop()
}

pub(super) fn load_comworld(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::COM_WORLD, 24))?;
    let light_count = s.u32_at(p, s.layout(8, 12))? as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    let mut primary_lights = None;
    if s.begin_body(p.at(s.layout(12, 16)))? {
        let light = s.layout(sz::COM_PRIMARY_LIGHT, 88);
        let def_name = s.layout(sz::COM_PRIMARY_LIGHT_DEF_NAME_OFF, 80);
        let arr = s.alloc_load(4, light * light_count)?;
        primary_lights = Some(arr);
        for i in 0..light_count {
            follow_name(s, arr.at(i * light), def_name)?;
        }
    }
    s.record_com_world(ComWorldGeometry {
        primary_lights,
        primary_light_count: light_count,
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

    if s.begin_body(glass.at(48))? {
        let def = s.layout(sz::FX_GLASS_DEF, 48);
        let arr = s.alloc_load(4, def * def_count)?;
        for i in 0..def_count {
            let d = arr.at(i * def);

            asset_ptr_at(
                s,
                links,
                AssetType::PhysPreset,
                d.at(s.layout(sz::FX_GLASS_DEF_PHYS_OFF, 40)),
            )?;
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                d.at(sz::FX_GLASS_DEF_MATERIAL_OFF),
            )?;
            asset_ptr_at(
                s,
                links,
                AssetType::Material,
                d.at(s.layout(sz::FX_GLASS_DEF_MATERIAL_SHATTERED_OFF, 32)),
            )?;
        }
    }

    for (field, align, elem, count) in [
        (s.layout(52, 56), 4, sz::FX_GLASS_PIECE_PLACE, piece_limit),
        (s.layout(56, 64), 4, sz::FX_GLASS_PIECE_STATE, piece_limit),
        (
            s.layout(60, 72),
            4,
            sz::FX_GLASS_PIECE_DYNAMICS,
            piece_limit,
        ),
        (s.layout(64, 80), 4, sz::FX_GLASS_GEOMETRY_DATA, geo_limit),
        (s.layout(68, 88), 4, 4, word_count),
        (s.layout(72, 96), 4, 4, word_count * cell_count),
        (s.layout(76, 104), 16, 1, piece_limit.div_ceil(16) * 16),
        (s.layout(80, 112), 4, sz::FX_GLASS_LINK_ORG, piece_limit),
        (s.layout(84, 120), 16, 4, piece_limit.div_ceil(4) * 4),
    ] {
        runtime_array(s, glass, field, align, elem, count)?;
    }

    s.plain_array(glass, s.layout(88, 128), 2, 2, init_pieces)?;
    s.plain_array(
        glass,
        s.layout(92, 136),
        4,
        sz::FX_GLASS_INIT_PIECE_STATE,
        init_pieces,
    )?;
    s.plain_array(
        glass,
        s.layout(96, 144),
        4,
        sz::FX_GLASS_GEOMETRY_DATA,
        init_geo,
    )?;

    s.pop()
}

pub(super) fn load_glass_world(s: &mut ZoneStream<'_>) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::GLASS_WORLD, 16))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;

    if s.begin_body(p.at(s.layout(4, 8)))? {
        let glass = s.alloc_load(4, s.layout(sz::G_GLASS_DATA, 144))?;
        let pieces = s.u32_at(glass, s.layout(4, 8))? as usize;
        let names = s.u32_at(glass, s.layout(12, 16))? as usize;
        s.plain_array(glass, 0, 4, sz::G_GLASS_PIECE, pieces)?;
        if s.begin_body(glass.at(s.layout(16, 24)))? {
            let name_row = s.layout(sz::G_GLASS_NAME, 24);
            let arr = s.alloc_load(4, name_row * names)?;
            for i in 0..names {
                let g = arr.at(i * name_row);
                follow_name(s, g, 0)?;
                let count = s.u16_at(g, s.layout(6, 10))? as usize;
                s.plain_array(g, s.layout(8, 16), 2, 2, count)?;
            }
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
        s.alloc_load(align, elem.saturating_mul(count))?;
        s.pop()?;
    }
    Ok(())
}
