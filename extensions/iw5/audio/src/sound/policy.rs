use asset_audio::playback_policy::compiler::{
    CueCompiler, CueSemantics, GroupSelection, common, native_curve,
};
use asset_audio::*;

pub struct Iw5CueCompiler;

impl CueCompiler for Iw5CueCompiler {
    fn prepare(&self, row: &CapturedAlias, channel: Option<&EntChannel>) -> CueSemantics {
        CueSemantics {
            spatial: match channel {
                Some(info) if info.is_3d => Some(native_curve(row)),
                Some(_) => None,
                None => Some(Err(SpatialPolicyFailure::MissingChannel(FamilyId::new(
                    "iw5",
                )))),
            },
            limits: [VoiceLimit::default(); 2],
            group: row
                .vol_mod_index
                .map_or(GroupSelection::Ungrouped, GroupSelection::Index),
            speaker_gains: common::iw_speaker_gains(row),
            mixer_group_supported: true,
            zero_volume_unity: true,
            secondary: (
                SecondaryActivation::OnPrimaryPrepared,
                SecondaryPolicySource::PrimaryPreparedCompatibility,
            ),
        }
    }
}

pub const POLICY: AudioFamily = AudioFamily {
    decoder_telemetry: None,
    explosion_surfaces: Some(asset_audio::reference_explosion_surface_aliases),
    ui_aliases: &[],
    mix_bus: MixBusStage {
        order: 20,
        trim_db: -2.5,
        compressor: None,
    },
    cue: &Iw5CueCompiler,
    decoder: &PcmAudioDecoder,
    capture: |path, _| Box::new(super::Iw5SoundCapture::for_zone(path)),
    walk: super::load_sound_catalog_iw5,
    common_zones: &["code_post_gfx_mp", "common_mp", "localized_common_mp"],
    accept_partial_walk: true,
    finalize_catalog,
};
fn finalize_catalog(catalog: &mut SoundCatalog) {
    if catalog.has_group_volumes(FamilyId::new("iw5")) {
        return;
    }
    let Some(file) =
        catalog.rawfile_bytes_in(FamilyId::new("iw5"), "soundaliases/volumemodgroups.svmod")
    else {
        return;
    };
    let volumes = String::from_utf8_lossy(file)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(',').map(str::trim);
            let name = fields.next()?;
            if name.is_empty() || name.starts_with('#') {
                return None;
            }
            Some(fields.next().and_then(|value| value.parse::<f32>().ok()))
        })
        .enumerate()
        .map(|(group, value)| match value {
            Some(gain) if gain.is_finite() && gain >= 0.0 => Ok(gain),
            _ => Err(MixerGroupError::InvalidAttenuation { group }),
        })
        .collect();
    catalog.ingest_group_volumes(FamilyId::new("iw5"), volumes);
}
