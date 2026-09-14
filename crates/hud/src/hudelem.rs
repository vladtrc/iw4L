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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProducerStatus {
    Implemented,

    Unreachable(&'static str),

    Gap,
}

pub struct ScriptedProducer {
    pub script_fn: &'static str,

    pub cite: &'static str,

    pub produces: &'static str,
    pub status: ProducerStatus,
}

pub const SCRIPTED_PRODUCERS: &[ScriptedProducer] = &[
    ScriptedProducer {
        script_fn: "_hud_message::init",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L5-41",
        produces: "precached outcome strings and splash defaults",
        status: ProducerStatus::Gap,
    },
    ScriptedProducer {
        script_fn: "_hud_message::initNotifyMessage",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L69-145",
        produces: "title / text / text2 / icon / overlay elements, fonts, offsets, glow",
        status: ProducerStatus::Gap,
    },
    ScriptedProducer {
        script_fn: "_hud_message::notifyMessage",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L163-188",
        produces: "the four splash queues and their serialisation",
        status: ProducerStatus::Gap,
    },
    ScriptedProducer {
        script_fn: "_hud_message::showNotifyMessage",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L221-374",
        produces: "the splash timeline: text/value, pulse, icon layering, fade, scale",
        status: ProducerStatus::Gap,
    },
    ScriptedProducer {
        script_fn: "_hud_message::lowerMessageThink",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L720-742",
        produces: "the lower message and its timer",
        status: ProducerStatus::Gap,
    },
    ScriptedProducer {
        script_fn: "_hud_message::matchOutcomeNotify",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L766-806",
        produces: "match outcome presentation",
        status: ProducerStatus::Gap,
    },
    ScriptedProducer {
        script_fn: "_hud_message::outcomeNotify",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L1038-1171",
        produces: "FFA title + reason + HE_TYPE_PLAYERNAME 1st/2nd/3rd leftover",
        status: ProducerStatus::Implemented,
    },
    ScriptedProducer {
        script_fn: "_hud_message::hintMessage",
        cite: "gsc:maps/mp/gametypes/_hud_message.gsc@sha256:e5773c6ad4e840b7#L58-66",
        produces: "OBJECTIVES_DM_HINT leftover (notifyText, 4s, glow 0.3/0.6/0.3); not the splash queue",
        status: ProducerStatus::Implemented,
    },
    ScriptedProducer {
        script_fn: "_damagefeedback::updateDamageFeedback",
        cite: "gsc:maps/mp/gametypes/_damagefeedback.gsc@sha256:141664ce14ccb2d7#L28-93",
        produces: "attacker hit X on g_hudelems (archived=true); CopyInUseHudElems + BG_LerpHudColors at cg.time",
        status: ProducerStatus::Implemented,
    },
    ScriptedProducer {
        script_fn: "_rank::scorePopup",
        cite: "gsc:maps/mp/gametypes/_rank.gsc@sha256:4b31033a8baf6c9a#L504-557",
        produces: "center HE_TYPE_VALUE + MP_PLUS; FFA !rankingEnabled colour from givePlayerScore",
        status: ProducerStatus::Implemented,
    },
    ScriptedProducer {
        script_fn: "_gamelogic::matchStartTimer",
        cite: "gsc:maps/mp/gametypes/_gamelogic.gsc@sha256:75b0836bafb28b3f#L985-1022",
        produces: "server FontString loc + pulsing hudbig setValue; leftover CG_DrawHudElem",
        status: ProducerStatus::Implemented,
    },
    ScriptedProducer {
        script_fn: "_events::firstBlood",
        cite: "gsc:maps/mp/_events.gsc@sha256:88cc647a71e0925e#L563-572",
        produces: "SplashNotifyDelayed firstblood when level.numKills==1; ActivateSplash stand-in",
        status: ProducerStatus::Implemented,
    },
];

pub fn open_producer_count() -> usize {
    SCRIPTED_PRODUCERS
        .iter()
        .filter(|p| p.status == ProducerStatus::Gap)
        .count()
}
