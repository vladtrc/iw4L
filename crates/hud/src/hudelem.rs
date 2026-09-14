use bevy::prelude::Resource;
use hud_iw4::{HUDELEM_SOUND_SLOTS, HudElem, TextPulseFx, TextPulseSound};

use crate::draw2d::TextRunFx;

#[derive(Resource)]
pub struct HudElemSoundLatch {
    last_played_time: [i32; HUDELEM_SOUND_SLOTS],
}

impl Default for HudElemSoundLatch {
    fn default() -> Self {
        Self {
            last_played_time: [0; HUDELEM_SOUND_SLOTS],
        }
    }
}

impl HudElemSoundLatch {
    fn slot(&mut self, sound_id: i32) -> Option<&mut i32> {
        let index = usize::try_from(sound_id).ok()?;
        self.last_played_time.get_mut(index)
    }
}

pub(crate) fn hudelem_pulse_sound(
    elem: &HudElem,
    text: &str,
    cg_time: i32,
    latch: &mut HudElemSoundLatch,
) -> Option<&'static str> {
    if elem.fx_birth_time == 0 {
        return None;
    }
    let birth_time = elem.fx_birth_time.min(cg_time);
    let last_played_time = latch.slot(elem.sound_id)?;
    hud_iw4::cl_play_text_fx_pulse_sounds(
        cg_time,
        hud_iw4::seh_print_strlen(text),
        birth_time,
        elem.fx_letter_time,
        elem.fx_decay_start_time,
        last_played_time,
    )
    .map(TextPulseSound::alias)
}

pub(crate) fn hudelem_text_fx(elem: &HudElem, cg_time: i32) -> Option<TextRunFx> {
    if elem.fx_birth_time == 0 {
        return None;
    }
    Some(TextRunFx {
        scene_time: cg_time,
        fx: TextPulseFx {
            birth_time: elem.fx_birth_time.min(cg_time),
            letter_time: elem.fx_letter_time,
            decay_start_time: elem.fx_decay_start_time,
            decay_duration: elem.fx_decay_duration,
        },
    })
}
