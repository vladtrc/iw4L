use asset_audio::playback_policy::compiler::{
    CueCompiler, CueSemantics, GroupSelection, native_curve,
};
use asset_audio::*;

pub struct T6CueCompiler;

impl CueCompiler for T6CueCompiler {
    fn prepare(&self, row: &CapturedAlias, _channel: Option<&EntChannel>) -> CueSemantics {
        CueSemantics {
            spatial: match row.flags {
                Some(flags) if flags & 2 != 0 => Some(
                    row.near_falloff
                        .as_ref()
                        .ok_or(SpatialPolicyFailure::MissingFalloffCurve)
                        .and_then(|_| native_curve(row)),
                ),
                Some(_) => None,
                None => Some(Err(SpatialPolicyFailure::Unsupported(FamilyId::new("t6")))),
            },
            limits: [
                t6_limit(row, 25, row.limit_count, false),
                t6_limit(row, 27, row.entity_limit_count, true),
            ],
            group: row.flags.map_or(GroupSelection::Unknown, |flags| {
                GroupSelection::Index((flags >> 17) & 0x1f)
            }),
            speaker_gains: stereo_pan(row),
            mixer_group_supported: true,
            zero_volume_unity: false,
            secondary: (
                SecondaryActivation::OnResolution,
                SecondaryPolicySource::IndependentLayers,
            ),
        }
    }
}

fn stereo_pan(row: &CapturedAlias) -> Option<[[f32; 2]; 2]> {
    let [front, back, center, lfe, left, right] = row.surround_pan?;
    if [front, back, center, lfe, left, right]
        .iter()
        .any(|gain| !gain.is_finite() || *gain < 0.0)
    {
        return None;
    }
    let surround = std::f32::consts::FRAC_1_SQRT_2;
    let shared = center * surround + lfe * 0.5;
    let gains = [
        (front + back * surround) * left + shared,
        (front + back * surround) * right + shared,
    ];
    gains
        .iter()
        .all(|gain| gain.is_finite())
        .then_some([gains; 2])
}

fn t6_limit(row: &CapturedAlias, shift: u32, count: Option<u8>, per_emitter: bool) -> VoiceLimit {
    let Some(count) = count else {
        return VoiceLimit::default();
    };
    let mode = match (row.flags.unwrap_or(0) >> shift) & 3 {
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

#[derive(Default)]
pub struct T6SoundCapture {
    pub catalog: Option<SoundCatalog>,
}
impl NativeSoundCapture for T6SoundCapture {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
    fn catalog_mut(&mut self) -> Option<&mut SoundCatalog> {
        self.catalog.as_mut()
    }
    fn finish(self: Box<Self>) -> Result<SoundCatalog, String> {
        self.catalog
            .ok_or_else(|| "no T6 aliases were read".to_owned())
    }
}
pub const POLICY: AudioFamily = AudioFamily {
    decoder_telemetry: None,
    explosion_surfaces: None,
    ui_aliases: &[
        ("ui_pulse_text_type", "uin_notify_data"),
        ("ui_pulse_text_delete", "uin_alert_slideout"),
    ],
    mix_bus: MixBusStage {
        order: 30,
        trim_db: 0.0,
        compressor: Some(MixBusCompressor {
            threshold_db: -40.0,
            ratio: 4.0,
            makeup_db: 29.5,
            attack_s: 0.002,
            release_s: 0.212,
        }),
    },
    cue: &T6CueCompiler,
    decoder: &PcmAudioDecoder,
    capture: |_, _| Box::<T6SoundCapture>::default(),
    walk: |path| {
        Err(format!(
            "{}: T6 sounds come from the common walk, which has not run",
            path.display()
        ))
    },
    common_zones: &["common_mp"],
    accept_partial_walk: true,
    finalize_catalog: |_| {},
};
