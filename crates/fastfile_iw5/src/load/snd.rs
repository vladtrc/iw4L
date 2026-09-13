use super::follow_name;
use crate::size as sz;
use crate::zone::{Ptr, Result, ZonePtr, ZoneStream};

pub(super) fn follow_snd_alias_custom(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
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
            s.fixup_slot(slot, n)?;
            follow_name(s, n, 0)?;
            Ok(())
        }
    }
}

pub(super) fn follow_snd_alias_array(
    s: &mut ZoneStream<'_>,
    p: Ptr,
    field: usize,
    count: usize,
) -> Result<()> {
    match s.ptr_at(p, field)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(())
        }
        _ => {
            if !s.begin_body(p.at(field))? {
                return Ok(());
            }
            let custom = s.layout(sz::SND_ALIAS_CUSTOM, 8);
            let arr = s.alloc_load(4, custom * count)?;
            s.fixup_slot(p.at(field), arr)?;
            for i in 0..count {
                follow_snd_alias_custom(s, arr.at(i * custom))?;
            }
            Ok(())
        }
    }
}
