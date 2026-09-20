pub const SWITCH_STRUCT: &str = "switch_struct";

pub const TUNNEL_STRUCT: &str = "tunnel_light_spot";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwitchLightPhase {
    #[default]
    Prematch,
    Green,
    MovingRed,
    CooldownRed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TunnelLightPhase {
    #[default]
    Off,
    SolidGreen,
    Blink,
}

pub fn is_switch_struct(targetname: &str) -> bool {
    targetname == SWITCH_STRUCT
}

pub fn is_tunnel_light(targetname: &str) -> bool {
    targetname == TUNNEL_STRUCT
}

pub fn switch_light_phase(playing: bool, started_at: Option<u32>, now_ms: u32) -> SwitchLightPhase {
    if !playing {
        return SwitchLightPhase::Prematch;
    }
    let Some(at) = started_at else {
        return SwitchLightPhase::Green;
    };
    let elapsed = now_ms.saturating_sub(at);
    if elapsed < super::radiation_doors::DOOR_TIME_MS {
        SwitchLightPhase::MovingRed
    } else if elapsed < super::radiation_doors::UNAVAILABLE_MS {
        SwitchLightPhase::CooldownRed
    } else {
        SwitchLightPhase::Green
    }
}

pub fn tunnel_light_phase(
    open: bool,
    started_at: Option<u32>,
    completed: bool,
    now_ms: u32,
) -> TunnelLightPhase {
    if let Some(at) = started_at {
        let elapsed = now_ms.saturating_sub(at);
        if !completed && elapsed < super::radiation_doors::DOOR_TIME_MS {
            return TunnelLightPhase::Blink;
        }
    }
    if open {
        TunnelLightPhase::Off
    } else {
        TunnelLightPhase::SolidGreen
    }
}

pub fn switch_panel_exploder(phase: SwitchLightPhase) -> bool {
    matches!(phase, SwitchLightPhase::Green | SwitchLightPhase::MovingRed)
}

pub fn tunnel_plays_fx(phase: TunnelLightPhase) -> bool {
    matches!(
        phase,
        TunnelLightPhase::SolidGreen | TunnelLightPhase::Blink
    )
}
