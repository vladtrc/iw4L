use super::*;
pub mod common;
pub mod iw4;
pub use common::native_curve;

pub struct CueSemantics {
    pub spatial: Option<Result<SpatialPlaybackPolicy, SpatialPolicyFailure>>,
    pub limits: [VoiceLimit; 2],
    pub group: GroupSelection,
    pub zero_volume_unity: bool,
    pub speaker_gains: Option<[[f32; 2]; 2]>,
    pub mixer_group_supported: bool,
    pub secondary: (SecondaryActivation, SecondaryPolicySource),
}

pub enum GroupSelection {
    Ungrouped,
    Index(u32),
    Unknown,
}

pub trait CueCompiler: Send + Sync {
    fn prepare(&self, row: &CapturedAlias, channel: Option<&EntChannel>) -> CueSemantics;
}

pub(super) fn compile(
    catalog: &crate::SoundCatalog,
    alias: usize,
    variant: usize,
) -> Option<AliasPlaybackPolicy> {
    let sound = catalog.sound_at(alias)?;
    let row = sound.aliases.get(variant)?;
    let namespace = sound.game;
    let channels = catalog.channels_in(namespace);
    let group_volumes = catalog.mixer_group_volumes_in(namespace);
    let channel = sound
        .ent_channel(variant)
        .map(|id| ChannelKey { namespace, id });
    let channel_info = channel.and_then(|key| channels?.get(key.id as usize));
    let compiler = crate::audio_family(namespace)?.cue;
    let semantics = compiler.prepare(row, channel_info);
    let group_gain = match semantics.group {
        GroupSelection::Index(native_index) => {
            match group_volumes.and_then(|volumes| volumes.get(native_index as usize)) {
                Some(&Ok(scale)) => GroupGainPolicy::Resolved {
                    native_index,
                    scale,
                },
                Some(&Err(error)) => GroupGainPolicy::Invalid {
                    native_index,
                    error,
                },
                None => GroupGainPolicy::MissingGroupUnityCompatibility { native_index },
            }
        }
        GroupSelection::Unknown => GroupGainPolicy::UnknownIndexUnityCompatibility,
        GroupSelection::Ungrouped => GroupGainPolicy::Ungrouped,
    };
    Some(AliasPlaybackPolicy {
        master_slave: (namespace == FamilyId::IW4)
            .then_some(row.flags)
            .flatten()
            .map(|flags| MasterSlavePolicy {
                master: flags & asset_iw4::snd_alias::SND_ALIAS_FLAG_MASTER != 0,
                slave: flags & asset_iw4::snd_alias::SND_ALIAS_FLAG_SLAVE != 0,
                percentage: row.slave_percentage,
            }),
        reverb_flags: (namespace == FamilyId::IW4).then_some(row.flags).flatten(),
        namespace,
        looping: LoopingPolicy::compile(row.is_looping()),
        composition: CueCompositionPolicy::compile(
            row,
            semantics.mixer_group_supported,
            semantics.speaker_gains,
            semantics.secondary.0,
            semantics.secondary.1,
        ),
        stereo_speaker_gains: semantics.speaker_gains,
        loaded_binding_origin: row.loaded_binding_origin,
        channel,
        channel_admission: channel
            .zip(channel_info)
            .map(|(key, info)| ChannelAdmission {
                key,
                maximum: info.max_voices,
                restricted: info.is_restricted,
            }),
        priority: row.voice_priority.clone(),
        spatial: semantics.spatial,
        limits: semantics.limits,
        volume_range: if semantics.zero_volume_unity && row.vol_min == 0.0 && row.vol_max == 0.0 {
            ScalarRangePolicy::ZeroRangeUnityCompatibility
        } else {
            ScalarRangePolicy::Authored([row.vol_min, row.vol_max])
        },
        group_gain,
        pitch_range: if row.pitch_min == 0.0 && row.pitch_max == 0.0 {
            ScalarRangePolicy::ZeroRangeUnityCompatibility
        } else {
            ScalarRangePolicy::Authored([row.pitch_min, row.pitch_max])
        },
    })
}

impl CueCompositionPolicy {
    fn compile(
        row: &CapturedAlias,
        mixer_group_supported: bool,
        speaker_gains: Option<[[f32; 2]; 2]>,
        activation: SecondaryActivation,
        source: SecondaryPolicySource,
    ) -> Self {
        let secondary = row
            .secondary
            .as_ref()
            .filter(|alias| !alias.is_empty())
            .map(|alias| SecondaryLayerPolicy {
                alias: alias.clone(),
                activation,
                source,
                lifetime: LayerLifetime::ParentGroup,
                pitch: LayerPitch::IndependentAuthoredRange,
                failure: LayerFailure::Independent,
            });
        let mut unsupported = Vec::new();
        if row.start_delay < 0 {
            unsupported.push(UnsupportedCueFeature::StartDelay(row.start_delay));
        }
        if let Some(name) = row.chain.as_ref().filter(|name| !name.is_empty()) {
            unsupported.push(UnsupportedCueFeature::Chain(name.clone()));
        }
        if !mixer_group_supported
            && let Some(name) = row.mixer_group.as_ref().filter(|name| !name.is_empty())
        {
            unsupported.push(UnsupportedCueFeature::MixerGroup(name.clone()));
        }
        if speaker_gains.is_none()
            && let Some(name) = row.speaker_map.as_ref().filter(|name| !name.is_empty())
        {
            unsupported.push(UnsupportedCueFeature::SpeakerMap(name.clone()));
        }
        if row.stereo_speaker_gains.is_some() && speaker_gains.is_none() {
            unsupported.push(UnsupportedCueFeature::SpeakerGains);
        }
        Self {
            start_delay_ms: row.start_delay.max(0) as u32,
            secondary,
            unsupported: unsupported.into(),
        }
    }
}
