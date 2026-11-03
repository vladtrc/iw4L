use crate::playback_policy::compiler::{CueCompiler, iw4::Iw4CueCompiler};
use crate::{FamilyId, SoundCatalog};
use std::{any::Any, collections::BTreeMap, path::Path, sync::OnceLock};

pub trait NativeSoundCapture: Any + Send {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn catalog_mut(&mut self) -> Option<&mut SoundCatalog>;
    fn finish(self: Box<Self>) -> Result<SoundCatalog, String>;
}
impl NativeSoundCapture for SoundCatalog {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn catalog_mut(&mut self) -> Option<&mut SoundCatalog> {
        Some(self)
    }
    fn finish(mut self: Box<Self>) -> Result<SoundCatalog, String> {
        self.finalize();
        Ok(*self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixBusCompressor {
    pub threshold_db: f32,
    pub ratio: f32,
    pub makeup_db: f32,
    pub attack_s: f32,
    pub release_s: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixBusStage {
    pub order: u16,
    pub trim_db: f32,
    pub compressor: Option<MixBusCompressor>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecoderTelemetry {
    pub backend: &'static str,
    pub cache_hit: u64,
    pub cache_miss: u64,
    pub native_decodes: u64,
    pub failed: u64,
    pub pcm_bytes: u64,
    pub decode_ms: f64,
    pub key_ms: f64,
    pub cache_io_ms: f64,
}

#[derive(Clone, Copy)]
pub struct AudioFamily {
    pub decoder_telemetry: Option<fn() -> DecoderTelemetry>,
    pub explosion_surfaces: Option<fn(i32, u8) -> Option<(String, String)>>,
    pub ui_aliases: &'static [(&'static str, &'static str)],
    pub mix_bus: MixBusStage,
    pub cue: &'static dyn CueCompiler,
    pub decoder: &'static dyn crate::AudioDecoder,
    pub capture: fn(&Path, FamilyId) -> Box<dyn NativeSoundCapture>,
    pub walk: fn(&Path) -> Result<SoundCatalog, String>,
    pub common_zones: &'static [&'static str],
    pub accept_partial_walk: bool,
    pub finalize_catalog: fn(&mut SoundCatalog),
}
fn iw4_capture(path: &Path, family: FamilyId) -> Box<dyn NativeSoundCapture> {
    let mut catalog = SoundCatalog::default();
    catalog.set_capture_zone(crate::ZoneOwner::from_zone_path(path));
    catalog.set_capture_game(family);
    Box::new(catalog)
}
pub const REFERENCE_AUDIO_FAMILY: AudioFamily = AudioFamily {
    decoder_telemetry: None,
    explosion_surfaces: Some(crate::reference_explosion_surface_aliases),
    ui_aliases: &[],
    mix_bus: MixBusStage {
        order: 0,
        trim_db: 0.0,
        compressor: None,
    },
    cue: &Iw4CueCompiler,
    decoder: &crate::PcmAudioDecoder,
    capture: iw4_capture,
    walk: crate::load_sound_catalog,
    common_zones: &[
        "code_post_gfx_mp",
        "localized_code_post_gfx_mp",
        "patch_mp",
        "common_mp",
        "localized_common_mp",
    ],
    accept_partial_walk: false,
    finalize_catalog: |_| {},
};
#[derive(Default)]
pub struct AudioFamilies(BTreeMap<FamilyId, AudioFamily>);
impl AudioFamilies {
    pub fn register(&mut self, id: FamilyId, family: AudioFamily) {
        assert!(!self.0.contains_key(&id), "duplicate audio family");
        self.0.insert(id, family);
    }
    pub fn iter(&self) -> impl Iterator<Item = (FamilyId, &AudioFamily)> {
        self.0.iter().map(|(&id, family)| (id, family))
    }
}
static FAMILIES: OnceLock<AudioFamilies> = OnceLock::new();
pub fn install_audio_families(families: AudioFamilies) -> Result<(), &'static str> {
    FAMILIES
        .set(families)
        .map_err(|_| "audio families already installed")
}
pub fn audio_family(id: FamilyId) -> Option<&'static AudioFamily> {
    FAMILIES.get()?.0.get(&id)
}
pub fn audio_families() -> &'static AudioFamilies {
    FAMILIES.get().expect("builder must install audio families")
}
