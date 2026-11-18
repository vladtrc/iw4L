use super::*;

pub const MSS_PCM: i32 = 1;

#[derive(Clone, Debug)]
pub struct LoadedSoundPcm {
    pub name: String,

    pub game: FamilyId,
    pub format: i32,
    pub rate: u32,
    pub bits: i32,
    pub channels: i32,
    pub samples: u32,
    pub block_size: u32,
    pub pcm: crate::encoded_audio::EncodedAudio,

    pub zone: ZoneOwner,

    pub seek_table: Vec<u32>,

    pub external_media: Option<crate::ExternalAudioSource>,
}

impl LoadedSoundPcm {
    pub fn game(&self) -> FamilyId {
        self.game
    }
    pub fn captured(
        game: FamilyId,
        name: impl Into<String>,
        format: i32,
        rate: u32,
        channels: i32,
        pcm: Vec<u8>,
        seek_table: Vec<u32>,
    ) -> Self {
        Self {
            name: name.into(),
            game,
            format,
            rate,
            bits: 16,
            channels,
            samples: 0,
            block_size: 0,
            pcm: pcm.into(),
            zone: ZoneOwner::default(),
            seek_table,
            external_media: None,
        }
    }

    pub fn channels(&self) -> i32 {
        self.channels
    }

    pub fn encoded_bytes(&self) -> &[u8] {
        self.pcm.bytes()
    }

    pub fn encoded_shared(&self) -> std::sync::Arc<[u8]> {
        self.pcm.shared()
    }

    pub fn encoded_content_id(&self) -> [u8; 32] {
        self.pcm.content_id()
    }

    pub fn format(&self) -> i32 {
        self.format
    }

    pub fn bits(&self) -> i32 {
        self.bits
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CapturedSndCurve {
    pub name: String,
    pub knots: Vec<(f32, f32)>,
}

#[derive(Clone, Debug, Default)]
pub struct CapturedAlias {
    pub secondary: Option<String>,

    pub chain: Option<String>,

    pub mixer_group: Option<String>,

    pub loaded_name: Option<String>,

    pub loaded: LoadedSoundEdge,
    pub loaded_binding_origin: crate::LoadedBindingOrigin,

    pub streamed: Option<(String, String)>,

    pub file_type: Option<u8>,

    pub file_exists: Option<u8>,

    pub file_name: Option<String>,

    pub sequence: i32,

    pub vol_min: f32,
    pub vol_max: f32,
    pub vol_mod_index: Option<u32>,
    pub pitch_min: f32,
    pub pitch_max: f32,

    pub dist_min: f32,
    pub dist_max: f32,

    pub flags: Option<u32>,
    pub slave_percentage: Option<f32>,
    pub channel_id: Option<u32>,

    pub looping: Option<bool>,

    pub probability: f32,

    pub start_delay: i32,

    pub volume_falloff: Option<CapturedSndCurve>,

    pub distance_curve_names: Option<[String; 2]>,

    pub near_falloff: Option<CapturedSndCurve>,

    pub voice_priority: Option<VoicePriority>,

    pub speaker_map: Option<String>,

    pub stereo_speaker_gains: Option<[[f32; 2]; 2]>,

    pub surround_pan: Option<[f32; 6]>,

    pub limit_count: Option<u8>,

    pub entity_limit_count: Option<u8>,
}

impl CapturedAlias {
    pub fn file_kind(&self) -> &'static str {
        match self.file_type {
            None => "none",
            Some(1) => "loaded",
            Some(2) => "streamed",
            Some(3) => "primed",
            Some(_) => "unknown",
        }
    }

    pub fn is_null_file(&self) -> bool {
        is_null_sound_name(self.file_name.as_deref())
            || self
                .streamed
                .as_ref()
                .is_some_and(|(dir, name)| dir.is_empty() && is_null_sound_name(Some(name)))
            || is_null_sound_name(self.loaded_name.as_deref())
    }

    pub fn loaded_present_name(&self) -> Option<&str> {
        self.loaded
            .is_bound()
            .then_some(self.loaded_name.as_deref())
            .flatten()
            .filter(|name| !name.is_empty())
    }

    pub fn is_looping(&self) -> Option<bool> {
        self.looping
    }
}

pub(super) fn is_null_sound_name(name: Option<&str>) -> bool {
    let Some(name) = name else {
        return false;
    };
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name);
    crate::AssetRef::bare_name(file).eq_ignore_ascii_case("null.wav")
}

pub(super) fn capture_loaded_edge(
    sound_file: Option<ZonePtr>,
    file_type: Option<u8>,
    file_u: Option<&str>,
    file_u_deref: Option<&str>,
    loaded_name: Option<&str>,
) -> AssetEdge<crate::LoadedSoundSpace> {
    match sound_file {
        None | Some(ZonePtr::Null) => AssetEdge::Absent,
        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => {
            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
        }
        Some(ZonePtr::Offset(_)) => match file_type {
            Some(2) | Some(3) => AssetEdge::Absent,
            Some(1) => {
                if is_null_sound_name(loaded_name) {
                    return AssetEdge::Absent;
                }
                match file_u {
                    Some("null") | None => AssetEdge::Absent,
                    Some("following") | Some("insert") => {
                        if loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        }
                    }
                    Some("offset") => {
                        let stub =
                            file_u_deref == Some("following") || file_u_deref == Some("insert");
                        if stub && !loaded_name.is_some_and(|name| !name.is_empty()) {
                            AssetEdge::Unresolved(AssetEdgeReason::TempFieldNotAliasable)
                        } else {
                            AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
                        }
                    }
                    _ => AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss),
                }
            }
            _ => AssetEdge::Absent,
        },
    }
}

#[derive(Clone, Debug)]
pub struct VoicePriority {
    pub thresholds: [u8; 2],
    pub values: [u8; 2],
    pub distance_max: f32,
}

impl VoicePriority {
    pub fn evaluate(&self, distance: Option<f32>) -> f32 {
        let volume = distance.map_or(0.0, |distance| {
            if self.distance_max > 0.0 {
                1.0 - (distance / self.distance_max).clamp(0.0, 1.0)
            } else {
                0.0
            }
        });
        let [low, high] = self.thresholds.map(|value| f32::from(value) / 255.0);
        let [min, max] = self.values.map(f32::from);
        if volume <= low {
            min
        } else if volume >= high {
            max
        } else {
            min + (max - min) * (volume - low) / (high - low)
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapturedSound {
    pub name: String,
    pub aliases: Vec<CapturedAlias>,

    pub game: FamilyId,

    pub zone: ZoneOwner,
}

impl CapturedSound {
    pub fn game(&self) -> FamilyId {
        self.game
    }
    pub fn ent_channel(&self, variant: usize) -> Option<u32> {
        self.aliases.get(variant)?.channel_id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChannelKey {
    pub namespace: FamilyId,
    pub id: u32,
}
