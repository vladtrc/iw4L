use asset_audio::playback_policy::compiler::{
    CueCompiler, CueSemantics, GroupSelection, native_curve,
};
use asset_audio::*;

pub struct T5CueCompiler;

impl CueCompiler for T5CueCompiler {
    fn prepare(&self, row: &CapturedAlias, _channel: Option<&EntChannel>) -> CueSemantics {
        CueSemantics {
            spatial: match row.flags {
                Some(flags) if flags & 2 != 0 => Some(native_curve(row)),
                Some(_) => None,
                None => Some(Err(SpatialPolicyFailure::Unsupported(FamilyId::new("t5")))),
            },
            limits: [
                t5_limit(row, 25, row.limit_count, false),
                t5_limit(row, 27, row.entity_limit_count, true),
            ],
            group: row.flags.map_or(GroupSelection::Unknown, |flags| {
                GroupSelection::Index((flags >> 16) & 0x3f)
            }),
            speaker_gains: None,
            mixer_group_supported: false,
            zero_volume_unity: false,
            secondary: (
                SecondaryActivation::OnResolution,
                SecondaryPolicySource::IndependentLayers,
            ),
        }
    }
}

fn t5_limit(row: &CapturedAlias, shift: u32, count: Option<u8>, per_emitter: bool) -> VoiceLimit {
    let Some(count) = count else {
        return VoiceLimit::default();
    };
    let mode = match (row.flags.unwrap_or(0) >> shift) & 3u32 {
        1 => VoiceLimitMode::Oldest,
        2 => VoiceLimitMode::Reject,
        3 => VoiceLimitMode::Priority,
        _ => VoiceLimitMode::Unlimited,
    };
    VoiceLimit {
        mode,
        count: if mode == VoiceLimitMode::Oldest {
            count.max(1)
        } else {
            count
        },
        per_emitter,
        source: if row.flags.is_none() {
            VoiceLimitSource::UnknownFlagsUnlimitedCompatibility
        } else if mode == VoiceLimitMode::Oldest && count == 0 {
            VoiceLimitSource::ZeroOldestCountOneCompatibility
        } else {
            VoiceLimitSource::Native
        },
    }
}

pub const POLICY: AudioFamily = AudioFamily {
    decoder_telemetry: Some(decoder_telemetry),
    explosion_surfaces: None,
    ui_aliases: &[],
    mix_bus: MixBusStage {
        order: 10,
        trim_db: 7.0,
        compressor: None,
    },
    cue: &T5CueCompiler,
    decoder: &super::decode::T5AudioDecoder,
    capture: |path, _| Box::new(super::T5SoundCapture::for_zone(path)),
    walk: super::load_sound_catalog_t5,
    common_zones: &["code_post_gfx_mp", "common_mp", "localized_common_mp"],
    accept_partial_walk: true,
    finalize_catalog,
};
fn finalize_catalog(_: &mut SoundCatalog) {}

fn decoder_telemetry() -> DecoderTelemetry {
    let cost = super::sound_wma_t5::xwma_decode_cost();
    DecoderTelemetry {
        backend: "native_wma2",
        cache_hit: cost.hit,
        cache_miss: cost.miss,
        native_decodes: cost.native,
        failed: cost.failed,
        pcm_bytes: cost.pcm_bytes,
        decode_ms: cost.decode_ms,
        key_ms: cost.key_ms,
        cache_io_ms: cost.io_ms,
    }
}
