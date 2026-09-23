use crate::size as sz;
use crate::zone::{Ptr, WeaponGeometry, ZonePtr, ZoneStream};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub anim_tree_type: u32,
    pub override_anim: Option<Ptr>,
    pub altmode_anim: Option<Ptr>,
    pub anim_time_ms: i32,
    pub alt_time_ms: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub sound_type: u32,
    pub override_sound: Option<Ptr>,
    pub altmode_sound: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReloadOverride {
    pub attachment: u16,
    pub reload_add_time_ms: i32,
    pub reload_start_add_time_ms: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub fx_type: u32,
    pub override_fx: Option<Ptr>,
    pub altmode_fx: Option<Ptr>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoteTrackOverride {
    pub attachment: u16,
    pub sound_map: Option<ScriptStringMap>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptStringMap {
    keys: Ptr,
    values: Ptr,
}

impl ScriptStringMap {
    pub fn at(s: &ZoneStream<'_>, body: Ptr, keys_off: usize, values_off: usize) -> Option<Self> {
        Some(Self {
            keys: offset_at(s, body, keys_off)?,
            values: offset_at(s, body, values_off)?,
        })
    }

    pub fn pairs<'a>(
        self,
        s: &'a ZoneStream<'_>,
        cap: usize,
    ) -> impl Iterator<Item = (u16, u16)> + 'a {
        (0..cap).map_while(move |i| {
            let key = s.u16_at(self.keys, i * 2).ok().filter(|&key| key != 0)?;
            Some((key, s.u16_at(self.values, i * 2).ok()?))
        })
    }
}

impl WeaponGeometry {
    pub fn anim_overrides<'a>(
        &self,
        s: &'a ZoneStream<'_>,
    ) -> impl Iterator<Item = AnimOverride> + 'a {
        rows(
            self.anim_overrides,
            self.anim_override_count,
            s.layout(sz::ANIM_OVERRIDE_ENTRY, 40),
        )
        .map_while(move |row| {
            Some(AnimOverride {
                attachment1: s.u16_at(row, 0).ok()?,
                attachment2: s.u16_at(row, 2).ok()?,
                anim_tree_type: s
                    .i32_at(row, s.layout(sz::ANIM_OVERRIDE_ANIM_TREE_TYPE_OFF, 24))
                    .ok()? as u32,
                override_anim: offset_at(s, row, s.layout(sz::ANIM_OVERRIDE_OVERRIDE_ANIM_OFF, 8)),
                altmode_anim: offset_at(s, row, s.layout(sz::ANIM_OVERRIDE_ALTMODE_ANIM_OFF, 16)),
                anim_time_ms: s
                    .i32_at(row, s.layout(sz::ANIM_OVERRIDE_ANIM_TIME_OFF, 28))
                    .ok()?,
                alt_time_ms: s
                    .i32_at(row, s.layout(sz::ANIM_OVERRIDE_ALT_TIME_OFF, 36))
                    .ok()?,
            })
        })
    }

    pub fn sound_overrides<'a>(
        &self,
        s: &'a ZoneStream<'_>,
    ) -> impl Iterator<Item = SoundOverride> + 'a {
        rows(
            self.sound_overrides,
            self.sound_override_count,
            s.layout(sz::SOUND_OVERRIDE_ENTRY, 32),
        )
        .map_while(move |row| {
            Some(SoundOverride {
                attachment1: s.u16_at(row, 0).ok()?,
                attachment2: s.u16_at(row, 2).ok()?,
                sound_type: s.i32_at(row, s.layout(12, 24)).ok()? as u32,
                override_sound: snd_alias_name_at(s, row, s.layout(4, 8)),
                altmode_sound: snd_alias_name_at(s, row, s.layout(8, 16)),
            })
        })
    }

    pub fn reload_overrides<'a>(
        &self,
        s: &'a ZoneStream<'_>,
    ) -> impl Iterator<Item = ReloadOverride> + 'a {
        rows(
            self.reload_overrides,
            self.reload_override_count,
            sz::RELOAD_STATE_TIMER_ENTRY,
        )
        .map_while(move |row| {
            Some(ReloadOverride {
                attachment: s.u16_at(row, 0).ok()?,
                reload_add_time_ms: s.i32_at(row, 4).ok()?,
                reload_start_add_time_ms: s.i32_at(row, 8).ok()?,
            })
        })
    }

    pub fn fx_overrides<'a>(&self, s: &'a ZoneStream<'_>) -> impl Iterator<Item = FxOverride> + 'a {
        rows(
            self.fx_overrides,
            self.fx_override_count,
            s.layout(sz::FX_OVERRIDE_ENTRY, 32),
        )
        .map_while(move |row| {
            Some(FxOverride {
                attachment1: s.u16_at(row, 0).ok()?,
                attachment2: s.u16_at(row, 2).ok()?,
                fx_type: s.i32_at(row, s.layout(12, 24)).ok()? as u32,
                override_fx: offset_at(s, row, s.layout(4, 8)),
                altmode_fx: offset_at(s, row, s.layout(8, 16)),
            })
        })
    }

    pub fn note_track_overrides<'a>(
        &self,
        s: &'a ZoneStream<'_>,
    ) -> impl Iterator<Item = NoteTrackOverride> + 'a {
        rows(
            self.note_track_overrides,
            self.note_track_override_count,
            s.layout(sz::NOTE_TRACK_SOUND_ENTRY, 24),
        )
        .map_while(move |row| {
            Some(NoteTrackOverride {
                attachment: s.u16_at(row, 0).ok()?,
                sound_map: ScriptStringMap::at(s, row, s.layout(4, 8), s.layout(8, 16)),
            })
        })
    }
}

fn rows(arr: Option<Ptr>, count: i32, stride: usize) -> impl Iterator<Item = Ptr> {
    arr.into_iter()
        .flat_map(move |arr| (0..count.max(0) as usize).map(move |i| arr.at(i * stride)))
}

fn offset_at(s: &ZoneStream<'_>, body: Ptr, off: usize) -> Option<Ptr> {
    match s.ptr_at(body, off).ok()? {
        ZonePtr::Offset(q) => Some(s.resolve_alias(q)),
        _ => None,
    }
}

fn snd_alias_name_at(s: &ZoneStream<'_>, body: Ptr, off: usize) -> Option<Ptr> {
    let wrapper = offset_at(s, body, off)?;
    match s.ptr_at(wrapper, 0).ok()? {
        ZonePtr::Offset(name) => Some(s.resolve_alias(name)),
        _ => Some(wrapper),
    }
}
