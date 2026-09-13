use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_vehicle(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::VEHICLE_DEF, 1008))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    s.follow_string(p, s.layout(8, 16))?;

    s.follow_string(p, s.layout(172, 192))?;
    asset_ptr_at(s, links, AssetType::PhysPreset, p.at(s.layout(176, 200)))?;
    s.follow_string(p, s.layout(180, 208))?;

    s.follow_string(p, s.layout(408, 448))?;
    asset_ptr_at(s, links, AssetType::Weapon, p.at(s.layout(412, 456)))?;

    follow_snd_alias_custom(s, p.at(s.layout(436, 488)))?;
    follow_snd_alias_custom(s, p.at(s.layout(440, 496)))?;
    for (field, wide) in [(472, 536), (476, 544)] {
        asset_ptr_at(s, links, AssetType::Material, p.at(s.layout(field, wide)))?;
    }
    for (field, wide) in [
        (488, 560),
        (492, 568),
        (496, 576),
        (500, 584),
        (508, 600),
        (516, 616),
        (520, 624),
        (524, 632),
        (528, 640),
        (536, 656),
        (544, 672),
        (552, 688),
        (560, 704),
        (568, 720),
    ] {
        follow_snd_alias_custom(s, p.at(s.layout(field, wide)))?;
    }

    s.follow_string(p, s.layout(576, 736))?;
    for i in 0..sz::SURF_TYPE_NUM {
        follow_snd_alias_custom(s, p.at(s.layout(580, 744) + i * s.pointer_bytes()))?;
    }

    s.pop()
}

fn follow_snd_alias_custom(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(())
        }
        _ => {
            if !s.begin_body(slot)? {
                return Ok(());
            }
            let n = s.alloc_load(4, s.layout(sz::SND_ALIAS_CUSTOM, 8))?;
            follow_name(s, n, 0)?;
            Ok(())
        }
    }
}
