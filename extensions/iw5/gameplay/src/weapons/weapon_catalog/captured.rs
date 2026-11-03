use super::configuration::AttachmentSelection;
use asset_game::{WeaponCaptureData, WeaponSoundAliases};
use asset_iw4::size::weap_anim;
use weapon_iw4::WEAPON_ANIM_SLOTS;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AnimOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub anim_tree_type: u32,
    pub override_anim: Option<String>,
    pub altmode_anim: Option<String>,
    pub anim_time_ms: i32,
    pub alt_time_ms: i32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub sound_type: u32,
    pub override_sound: Option<String>,
    pub altmode_sound: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FxOverride {
    pub attachment1: u16,
    pub attachment2: u16,
    pub fx_type: u32,
    pub override_fx: Option<String>,
    pub altmode_fx: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NotetrackOverride {
    pub attachment: u16,
    pub sound_map: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default)]
pub struct CapturedWeapon {
    pub configuration: Option<(u32, AttachmentSelection)>,
    pub reload_overrides: Vec<ext_iw5_fastfile::ReloadOverride>,
    pub anim_overrides: Vec<AnimOverride>,
    pub fx_overrides: Vec<FxOverride>,
    pub notetrack_overrides: Vec<NotetrackOverride>,
    pub sound_overrides: Vec<SoundOverride>,
}
impl WeaponCaptureData for CapturedWeapon {
    fn clone_box(&self) -> Box<dyn WeaponCaptureData> {
        Box::new(self.clone())
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any + Send + Sync> {
        self
    }
    fn fill_missing(&mut self, donor: Box<dyn WeaponCaptureData>) {
        let donor = donor
            .into_any()
            .downcast::<Self>()
            .unwrap_or_else(|_| panic!("IW5 weapon capture schema mismatch"));
        if self.reload_overrides.is_empty() {
            self.reload_overrides = donor.reload_overrides;
        }
        if self.anim_overrides.is_empty() {
            self.anim_overrides = donor.anim_overrides;
        }
        if self.fx_overrides.is_empty() {
            self.fx_overrides = donor.fx_overrides;
        }
        if self.notetrack_overrides.is_empty() {
            self.notetrack_overrides = donor.notetrack_overrides;
        }
        if self.sound_overrides.is_empty() {
            self.sound_overrides = donor.sound_overrides;
        }
    }
}
impl CapturedWeapon {
    pub fn capture(
        stream: &ext_iw5_fastfile::ZoneStream<'_>,
        strings: &ext_iw5_fastfile::ScriptStrings,
        geometry: &ext_iw5_fastfile::WeaponGeometry,
        fx_name_at_slot: &dyn Fn(ext_iw5_fastfile::Ptr) -> Option<String>,
    ) -> Self {
        Self {
            configuration: None,
            reload_overrides: geometry.reload_overrides(stream).collect(),
            anim_overrides: leftover_iw5_anim_overrides(stream, geometry),
            fx_overrides: read_iw5_fx_overrides(stream, geometry, fx_name_at_slot),
            notetrack_overrides: read_iw5_notetrack_overrides(stream, strings, geometry),
            sound_overrides: if geometry.weap_def.is_some() {
                leftover_iw5_sound_overrides(stream, geometry)
            } else {
                Vec::new()
            },
        }
    }
    pub fn sound_dependencies(&self) -> Option<Vec<String>> {
        if self.sound_overrides.is_empty() {
            return None;
        }
        Some(
            self.sound_overrides
                .iter()
                .flat_map(|row| [&row.override_sound, &row.altmode_sound])
                .filter_map(|s| s.as_ref())
                .filter(|s| !s.is_empty())
                .cloned()
                .collect(),
        )
    }
}
pub fn apply_default_animations(
    sz: &mut [Option<String>; WEAPON_ANIM_SLOTS],
    overrides: &[AnimOverride],
) {
    for ov in overrides {
        if ov.attachment1 != 0 || ov.attachment2 != 0 {
            continue;
        }
        let Some(slot) = anim_slot(ov.anim_tree_type) else {
            continue;
        };
        if let Some(name) = ov.override_anim.clone() {
            sz[slot] = Some(name);
        }
    }
}

pub fn apply_default_sounds(sounds: &mut WeaponSoundAliases, overrides: &[SoundOverride]) {
    use ext_iw5_fastfile::size as sz;
    for ov in overrides {
        if ov.attachment1 != 0 || ov.attachment2 != 0 {
            continue;
        }
        match ov.sound_type {
            x if x == sz::SND_OVERRIDE_TYPE_FIRE => {
                if sounds.fire.is_none() {
                    sounds.fire = ov.override_sound.clone();
                }
            }
            x if x == sz::SND_OVERRIDE_TYPE_PLAYER_FIRE => {
                if sounds.fire_player.is_none() {
                    sounds.fire_player = ov.override_sound.clone();
                }
            }
            x if x == sz::SND_OVERRIDE_TYPE_PLAYER_LASTSHOT => {
                if sounds.fire_last_player.is_none() {
                    sounds.fire_last_player = ov.override_sound.clone();
                }
            }
            _ => {}
        }
    }
}

fn leftover_iw5_anim_overrides(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
) -> Vec<AnimOverride> {
    geometry
        .anim_overrides(stream)
        .map(|row| AnimOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            anim_tree_type: row.anim_tree_type,
            override_anim: row
                .override_anim
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            altmode_anim: row
                .altmode_anim
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            anim_time_ms: row.anim_time_ms,
            alt_time_ms: row.alt_time_ms,
        })
        .collect()
}

fn leftover_iw5_sound_overrides(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
) -> Vec<SoundOverride> {
    geometry
        .sound_overrides(stream)
        .map(|row| SoundOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            sound_type: row.sound_type,
            override_sound: row
                .override_sound
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
            altmode_sound: row
                .altmode_sound
                .and_then(|ptr| leftover_cstr_iw5(stream, ptr)),
        })
        .collect()
}

fn read_iw5_fx_overrides(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
    fx_name_at_slot: &dyn Fn(ext_iw5_fastfile::Ptr) -> Option<String>,
) -> Vec<FxOverride> {
    geometry
        .overrides(stream)
        .map(|row| FxOverride {
            attachment1: row.attachment1,
            attachment2: row.attachment2,
            fx_type: row.fx_type,
            override_fx: row.override_fx.and_then(fx_name_at_slot),
            altmode_fx: row.altmode_fx.and_then(fx_name_at_slot),
        })
        .collect()
}

fn read_iw5_notetrack_overrides(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    geometry: &ext_iw5_fastfile::WeaponGeometry,
) -> Vec<NotetrackOverride> {
    geometry
        .note_track_overrides(stream)
        .map(|row| NotetrackOverride {
            attachment: row.attachment,
            sound_map: row
                .sound_map
                .map(|map| iw5_script_string_pairs(stream, strings, map, 24))
                .unwrap_or(Vec::new()),
        })
        .collect()
}

pub(super) fn anim_slot(ty: u32) -> Option<usize> {
    let ty = ty as usize;
    if ty <= weap_anim::ADS_RECHAMBER {
        Some(ty)
    } else if ty == ext_iw5_fastfile::size::WEAPON_ANIM_ADS_UP {
        Some(weap_anim::ADS_UP)
    } else if ty == ext_iw5_fastfile::size::WEAPON_ANIM_ADS_DOWN {
        Some(weap_anim::ADS_DOWN)
    } else {
        None
    }
}

fn leftover_cstr_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    ptr: ext_iw5_fastfile::Ptr,
) -> Option<String> {
    stream
        .cstr(ptr)
        .ok()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

fn iw5_script_string_pairs(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    strings: &ext_iw5_fastfile::ScriptStrings,
    map: ext_iw5_fastfile::ScriptStringMap,
    cap: usize,
) -> Vec<(String, String)> {
    map.pairs(stream, cap)
        .filter_map(|(key_id, val_id)| {
            let key = strings.get(stream, key_id).filter(|s| !s.is_empty())?;
            let val = strings
                .get(stream, val_id)
                .filter(|s| !s.is_empty())
                .unwrap_or(key);
            Some((key.to_owned(), val.to_owned()))
        })
        .collect()
}
pub(super) fn iw5_best_pair_override<'a, T>(
    rows: &'a [T],
    selection: AttachmentSelection,
    override_type: u32,
    fields: impl Fn(&T) -> (u16, u16, u32),
) -> Option<&'a T> {
    let candidates = selection.override_candidates();
    let mut best = None;
    let mut best_score = 0;
    for row in rows {
        let (first, second, row_type) = fields(row);
        if row_type != override_type {
            continue;
        }
        let score = u8::from(first != 0 && candidates.contains(&first))
            + u8::from(second != 0 && candidates.contains(&second));
        if score > best_score {
            best = Some(row);
            best_score = score;
        }
    }
    best
}

pub(super) fn remap_iw5_weap_type(raw: i32) -> i32 {
    match raw {
        1 => 0,
        2 => 1,
        3 => 2,
        4 => 3,
        other => other,
    }
}
pub(super) fn read_sz_xanims_iw5(
    stream: &ext_iw5_fastfile::ZoneStream<'_>,
    arr: ext_iw5_fastfile::Ptr,
) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut iw5 = [const { None }; ext_iw5_fastfile::size::WEAPON_ANIM_COUNT];

    let step = stream.pointer_bytes();
    for (i, slot) in iw5.iter_mut().enumerate() {
        let name_ptr = match stream.ptr_at(arr, i * step) {
            Ok(ext_iw5_fastfile::ZonePtr::Offset(q)) => Some(stream.resolve_alias(q)),
            _ => None,
        };
        *slot = name_ptr
            .and_then(|ptr| stream.cstr(ptr).ok())
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
    }
    remap_iw5_sz_xanims(&iw5)
}

pub(super) fn remap_iw5_sz_xanims(iw5: &[Option<String>]) -> [Option<String>; WEAPON_ANIM_SLOTS] {
    let mut out = [const { None }; WEAPON_ANIM_SLOTS];
    for (i, name) in iw5.iter().enumerate() {
        let Some(slot) = anim_slot(i as u32) else {
            continue;
        };
        out[slot] = name.clone();
    }
    out
}
