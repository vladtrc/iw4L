use crate::phase::Team;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UseNotifySlots {
    pub on_begin_use: bool,
    pub on_end_use: bool,
    pub on_use: bool,
    pub on_use_update: bool,
}

impl UseNotifySlots {
    pub const fn dom_flag() -> Self {
        Self {
            on_begin_use: true,
            on_end_use: true,
            on_use: true,
            on_use_update: true,
        }
    }

    pub const fn dem_bombzone() -> Self {
        Self {
            on_begin_use: true,
            on_end_use: true,
            on_use: true,
            on_use_update: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UseCallbackKind {
    #[default]
    Unbound,
    DomFlag,
    DemBombzone,
}

impl UseCallbackKind {
    pub const fn on_use_gap_id(self) -> Option<&'static str> {
        match self {
            UseCallbackKind::Unbound => None,
            UseCallbackKind::DomFlag => Some("gsc.dom.onUse"),
            UseCallbackKind::DemBombzone => Some("gsc.dem.onUseObject"),
        }
    }

    pub fn on_use_gap(self) -> Option<crate::ScriptGap> {
        match self.on_use_gap_id() {
            Some(id) => crate::ScriptGap::from_use_gap_id(id),
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseScriptCall {
    BeginUse {
        player: u32,
    },
    EndUse {
        team: Team,
        player: Option<u32>,
        success: bool,
    },
    Use {
        player: u32,
    },
    UseUpdate {
        team: Team,

        progress_milli: i32,

        change_milli: i32,
    },
}

pub fn use_type_begin_calls(
    slots: UseNotifySlots,
    use_time_ms: i32,
    player: u32,
) -> [Option<UseScriptCall>; 1] {
    if use_time_ms > 0 && slots.on_begin_use {
        [Some(UseScriptCall::BeginUse { player })]
    } else {
        [None]
    }
}

pub fn use_type_after_hold_calls(
    slots: UseNotifySlots,
    use_time_ms: i32,
    team: Team,
    player: u32,
    result: bool,
) -> [Option<UseScriptCall>; 2] {
    let end = if use_time_ms > 0 && slots.on_end_use {
        Some(UseScriptCall::EndUse {
            team,
            player: Some(player),
            success: result,
        })
    } else {
        None
    };
    let use_call = if result && slots.on_use {
        Some(UseScriptCall::Use { player })
    } else {
        None
    };
    [end, use_call]
}

pub fn prox_complete_calls(
    slots: UseNotifySlots,
    claim_team: Team,
    credit_player: Option<u32>,
) -> [Option<UseScriptCall>; 2] {
    let end = if slots.on_end_use {
        Some(UseScriptCall::EndUse {
            team: claim_team,
            player: credit_player,
            success: credit_player.is_some(),
        })
    } else {
        None
    };
    let use_call = match (slots.on_use, credit_player) {
        (true, Some(player)) => Some(UseScriptCall::Use { player }),
        _ => None,
    };
    [end, use_call]
}

pub fn prox_unclaim_calls(
    slots: UseNotifySlots,
    claim_team: Team,
    claim_player: Option<u32>,
) -> [Option<UseScriptCall>; 1] {
    if slots.on_end_use {
        [Some(UseScriptCall::EndUse {
            team: claim_team,
            player: claim_player,
            success: false,
        })]
    } else {
        [None]
    }
}

pub fn prox_instant_use_calls(
    slots: UseNotifySlots,
    claim_player: Option<u32>,
) -> [Option<UseScriptCall>; 1] {
    match (slots.on_use, claim_player) {
        (true, Some(player)) => [Some(UseScriptCall::Use { player })],
        _ => [None],
    }
}

pub fn prox_begin_calls(
    slots: UseNotifySlots,
    use_time_ms: i32,
    claim_player: u32,
) -> [Option<UseScriptCall>; 1] {
    if use_time_ms > 0 && slots.on_begin_use {
        [Some(UseScriptCall::BeginUse {
            player: claim_player,
        })]
    } else {
        [None]
    }
}

pub fn prox_use_update_call(
    slots: UseNotifySlots,
    claim_team: Team,
    cur_progress: i32,
    use_time_ms: i32,
    use_rate: f32,
) -> Option<UseScriptCall> {
    if !slots.on_use_update || use_time_ms <= 0 {
        return None;
    }
    Some(UseScriptCall::UseUpdate {
        team: claim_team,
        progress_milli: (cur_progress * 1000) / use_time_ms,
        change_milli: ((50.0 * use_rate * 1000.0) / use_time_ms as f32) as i32,
    })
}
