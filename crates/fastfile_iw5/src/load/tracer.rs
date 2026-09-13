use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Result, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_tracer(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::TRACER_DEF, 120))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(4, 8)))?;
    s.pop()
}
