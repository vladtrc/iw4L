use super::{AssetLinkSink, always_alloc, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_destructible_def(
    s: &mut ZoneStream<'_>,
    links: &mut dyn AssetLinkSink,
) -> Result<()> {
    let p = s.alloc_load(4, sz::DESTRUCTIBLE_DEF)?;
    let num_pieces = s.i32_at(p, 12)?.max(0) as usize;

    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(4))?;
    asset_ptr_at(s, links, AssetType::XModel, p.at(8))?;

    if always_alloc(s, p.at(16))? {
        let arr = s.alloc_load(4, sz::DESTRUCTIBLE_PIECE * num_pieces)?;
        s.fixup_slot(p.at(16), arr)?;
        for i in 0..num_pieces {
            load_piece(s, links, arr.at(i * sz::DESTRUCTIBLE_PIECE))?;
        }
    }
    links.capture_destructible(s, p)?;
    s.pop()
}

fn load_piece(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    for i in 0..5 {
        load_stage(s, links, p.at(i * sz::DESTRUCTIBLE_STAGE))?;
    }
    asset_ptr_at(
        s,
        links,
        AssetType::PhysConstraints,
        p.at(sz::DESTRUCTIBLE_PIECE_PHYS_OFF),
    )?;
    follow_name(s, p, sz::DESTRUCTIBLE_PIECE_DAMAGE_SOUND_OFF)?;
    asset_ptr_at(
        s,
        links,
        AssetType::Fx,
        p.at(sz::DESTRUCTIBLE_PIECE_BURN_FX_OFF),
    )?;
    follow_name(s, p, sz::DESTRUCTIBLE_PIECE_BURN_SOUND_OFF)?;

    Ok(())
}

fn load_stage(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    asset_ptr_at(s, links, AssetType::Fx, p.at(16))?;
    follow_name(s, p, 20)?;
    follow_name(s, p, 24)?;
    follow_name(s, p, 28)?;
    for i in 0..3 {
        asset_ptr_at(s, links, AssetType::XModel, p.at(32 + i * 4))?;
    }
    asset_ptr_at(s, links, AssetType::PhysPreset, p.at(44))?;
    Ok(())
}
