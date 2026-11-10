#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptAmbient {
    pub alias: Option<String>,
    pub start_ms: i32,
    pub end_ms: i32,
    pub rate_percent: u8,
}

impl Default for ScriptAmbient {
    fn default() -> Self {
        Self {
            alias: None,
            start_ms: 0,
            end_ms: 0,
            rate_percent: 100,
        }
    }
}

impl ScriptAmbient {
    pub fn valid(&self) -> bool {
        self.start_ms >= 0
            && self.end_ms >= self.start_ms
            && self.rate_percent <= 100
            && self
                .alias
                .as_ref()
                .is_none_or(|name| !name.is_empty() && name.len() <= 1023)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScriptEqBand {
    pub kind: u8,
    pub gain_db: f32,
    pub frequency: f32,
    pub q: f32,
}

impl ScriptEqBand {
    pub fn valid(&self) -> bool {
        self.kind < 5
            && self.gain_db.is_finite()
            && self.frequency.is_finite()
            && (0.0..=20000.0).contains(&self.frequency)
            && self.q.is_finite()
            && self.q > 0.0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScriptAudioCommand {
    EntitySound {
        entity: u16,
        alias_index: u16,
        alias: String,
        origin: [f32; 3],
        completion_client: crate::ClientId,
        master: bool,
    },
    EqBands {
        client: crate::ClientId,
        channel: String,
        track: u8,
        filters: Vec<ScriptEqBand>,
    },
    Eq {
        client: crate::ClientId,
        channel: String,
        track: u8,
        band: u8,
        filter: ScriptEqBand,
    },
    DeactivateEq {
        client: crate::ClientId,
        track: u8,
        channel: Option<String>,
        band: Option<u8>,
    },
    EqLerp {
        client: crate::ClientId,
        fraction: f32,
    },
    MusicPlay(String),
    MusicStop {
        fade_ms: i32,
    },
    SoundFade {
        volume: f32,
        fade_ms: i32,
    },
    ChannelVolumes {
        client: crate::ClientId,
        priority: u8,
        volumes: Option<std::collections::BTreeMap<String, f32>>,
        fade_ms: i32,
    },
    DeactivateChannelVolumes {
        client: crate::ClientId,
        priority: u8,
        fade_ms: i32,
    },
    Reverb {
        client: crate::ClientId,
        priority: u8,
        room: String,
        dry: f32,
        wet: f32,
        fade_ms: i32,
    },
    DeactivateReverb {
        client: crate::ClientId,
        priority: u8,
        fade_ms: i32,
    },
}

impl ScriptAudioCommand {
    pub fn target(&self) -> Option<crate::ClientId> {
        match self {
            Self::ChannelVolumes { client, .. }
            | Self::DeactivateChannelVolumes { client, .. }
            | Self::Reverb { client, .. }
            | Self::DeactivateReverb { client, .. }
            | Self::EqBands { client, .. }
            | Self::Eq { client, .. }
            | Self::DeactivateEq { client, .. }
            | Self::EqLerp { client, .. } => Some(*client),
            _ => None,
        }
    }

    pub fn valid(&self) -> bool {
        match self {
            Self::EntitySound {
                entity,
                alias_index,
                alias,
                origin,
                ..
            } => {
                *entity < 2046
                    && (1..=255).contains(alias_index)
                    && !alias.is_empty()
                    && alias.len() <= 1023
                    && origin.iter().all(|v| v.is_finite())
            }
            Self::EqBands {
                channel,
                track,
                filters,
                ..
            } => {
                channel.len() <= 64
                    && *track < 2
                    && (1..=3).contains(&filters.len())
                    && filters.iter().all(ScriptEqBand::valid)
            }
            Self::Eq {
                channel,
                track,
                band,
                filter,
                ..
            } => channel.len() <= 64 && *track < 2 && *band < 3 && filter.valid(),
            Self::DeactivateEq {
                track,
                channel,
                band,
                ..
            } => {
                *track < 2
                    && channel.as_ref().is_none_or(|name| name.len() <= 64)
                    && band.is_none_or(|band| band < 3)
                    && (band.is_none() || channel.is_some())
            }
            Self::EqLerp { fraction, .. } => fraction.is_finite() && (0.0..=1.0).contains(fraction),
            Self::Reverb {
                priority,
                room,
                dry,
                wet,
                fade_ms,
                ..
            } => {
                (1..=2).contains(priority)
                    && room.len() <= 1023
                    && dry.is_finite()
                    && wet.is_finite()
                    && *fade_ms >= 0
            }
            Self::DeactivateReverb {
                priority, fade_ms, ..
            } => (1..=2).contains(priority) && *fade_ms >= 0,
            Self::MusicPlay(alias) => alias.len() <= 1023,
            Self::MusicStop { fade_ms } => *fade_ms >= 0,
            Self::ChannelVolumes {
                priority,
                volumes,
                fade_ms,
                ..
            } => {
                (1..=3).contains(priority)
                    && *fade_ms >= 0
                    && volumes.as_ref().is_none_or(|volumes| {
                        !volumes.is_empty()
                            && volumes.len() <= 64
                            && volumes.iter().all(|(name, gain)| {
                                !name.is_empty()
                                    && name.len() <= 64
                                    && *name == name.to_ascii_lowercase()
                                    && gain.is_finite()
                                    && (0.0..=1.0).contains(gain)
                            })
                    })
            }
            Self::DeactivateChannelVolumes {
                priority, fade_ms, ..
            } => (1..=3).contains(priority) && *fade_ms >= 0,
            Self::SoundFade { volume, fade_ms } => {
                volume.is_finite() && *volume >= 0.0 && *fade_ms >= 0
            }
        }
    }
}
