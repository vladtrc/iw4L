pub const SND_VOICE_FINISHED_FRACTION: f32 = 1.0;

#[derive(Clone, Copy, Debug)]
pub struct SndVoiceOccupant {
    pub priority: i32,

    pub looping: bool,

    pub finished: bool,

    pub metric: f32,

    pub has_subtitle: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct SndVoiceRequest {
    pub priority: i32,
    pub metric: f32,
    pub has_subtitle: bool,
}

pub fn snd_voice_metric_2d(volume: f32) -> f32 {
    -volume
}

pub fn snd_has_free_voice(voice_count: i32, loading_streams: i32, max_voices: i32) -> bool {
    if max_voices <= 0 {
        return false;
    }
    voice_count.saturating_add(loading_streams) < max_voices
}

pub fn snd_entity_channel_matches(
    occupant_snd_ent: u32,
    occupant_channel: u32,
    snd_ent: u32,
    channel: u32,
) -> bool {
    occupant_snd_ent == snd_ent && occupant_channel == channel
}

pub fn snd_pick_voice_slot(
    request: &SndVoiceRequest,
    occupants: &[SndVoiceOccupant],
) -> Option<usize> {
    let mut bar_prio = request.priority;
    let mut best_metric = request.metric;
    let mut replaceable = None;
    for (index, occupant) in occupants.iter().enumerate() {
        if !occupant.looping && occupant.finished {
            return Some(index);
        }
        if !request.has_subtitle && occupant.has_subtitle {
            continue;
        }
        if occupant.priority < bar_prio {
            bar_prio = occupant.priority;
            best_metric = occupant.metric;
            replaceable = Some(index);
        } else if occupant.priority == bar_prio && best_metric < occupant.metric {
            best_metric = occupant.metric;
            replaceable = Some(index);
        }
    }
    replaceable
}
