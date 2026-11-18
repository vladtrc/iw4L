use super::{AssetLinkSink, always_array, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_glasses(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, 0x38)?;
    let count = s.u32_at(p, 4)? as usize;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    if let Some(glasses) = always_array(s, p.at(8), 4, count * 0x7c)? {
        for i in 0..count {
            load_glass(s, links, glasses.at(i * 0x7c))?;
        }
    }

    s.pop()
}

fn load_glass(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink, p: Ptr) -> Result<()> {
    if s.begin_body(p)? {
        let def = s.alloc_load(4, 0x3c)?;
        s.fixup_slot(p, def)?;
        follow_name(s, def, 0)?;

        for offset in [0x1c, 0x20, 0x24] {
            asset_ptr_at(s, links, AssetType::Material, def.at(offset))?;
        }
        for offset in [0x28, 0x2c, 0x30] {
            follow_name(s, def, offset)?;
        }
        for offset in [0x34, 0x38] {
            asset_ptr_at(s, links, AssetType::Fx, def.at(offset))?;
        }
    }

    let vertices = s.u8_at(p, 0x3d)? as usize;
    always_array(s, p.at(0x40), 4, vertices * 8)?;
    Ok(())
}
