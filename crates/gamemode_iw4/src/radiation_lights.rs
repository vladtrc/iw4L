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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_is_green_after_prematch_until_the_first_move() {
        assert_eq!(
            switch_light_phase(false, None, 0),
            SwitchLightPhase::Prematch
        );
        assert_eq!(switch_light_phase(true, None, 0), SwitchLightPhase::Green);
        assert!(switch_panel_exploder(SwitchLightPhase::Green));
    }

    #[test]
    fn switch_stays_red_through_move_and_cooldown() {
        assert_eq!(
            switch_light_phase(true, Some(1_000), 1_000),
            SwitchLightPhase::MovingRed
        );
        assert_eq!(
            switch_light_phase(true, Some(1_000), 9_000),
            SwitchLightPhase::CooldownRed
        );
        assert_eq!(
            switch_light_phase(true, Some(1_000), 29_000),
            SwitchLightPhase::Green
        );
        assert!(!switch_panel_exploder(SwitchLightPhase::CooldownRed));
    }

    #[test]
    fn tunnel_green_when_closed_blink_while_moving_off_when_open() {
        assert_eq!(
            tunnel_light_phase(false, None, false, 0),
            TunnelLightPhase::SolidGreen
        );
        assert_eq!(
            tunnel_light_phase(false, Some(0), false, 100),
            TunnelLightPhase::Blink
        );
        assert_eq!(
            tunnel_light_phase(true, Some(0), true, 8_000),
            TunnelLightPhase::Off
        );
        assert!(tunnel_plays_fx(TunnelLightPhase::SolidGreen));
        assert!(tunnel_plays_fx(TunnelLightPhase::Blink));
        assert!(!tunnel_plays_fx(TunnelLightPhase::Off));
    }
}
