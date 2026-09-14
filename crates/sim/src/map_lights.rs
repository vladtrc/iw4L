use crate::frame::FrameWorld;
use crate::{MatchPhase, Tick};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadiationLights {
    pub switch_count: u32,
    pub tunnel_count: u32,
    pub last_switch: gamemode_iw4::SwitchLightPhase,
    pub last_tunnel: gamemode_iw4::TunnelLightPhase,
}

pub(crate) fn advance(world: &mut FrameWorld, tick: Tick) {
    let Some(mut lights) = world.radiation_lights.take() else {
        return;
    };
    let (started_at, open, completed) = {
        let Some(doors) = world.map_doors.as_ref() else {
            world.radiation_lights = Some(lights);
            return;
        };
        (doors.started_at, doors.open, doors.completed)
    };
    let now = tick.0.saturating_mul(50);
    let playing = world.phase() == MatchPhase::Playing;
    let switch = gamemode_iw4::switch_light_phase(playing, started_at, now);
    let tunnel = gamemode_iw4::tunnel_light_phase(open, started_at, completed, now);
    if lights.switch_count > 0 && switch != lights.last_switch {
        if gamemode_iw4::switch_panel_exploder(switch)
            && switch != gamemode_iw4::SwitchLightPhase::MovingRed
        {
            world
                .script_gaps_mut()
                .raise(gamemode_iw4::ScriptGapCause::RadiationSwitchExploder);
        }
        lights.last_switch = switch;
    }
    if lights.tunnel_count > 0 && tunnel != lights.last_tunnel {
        if gamemode_iw4::tunnel_plays_fx(tunnel) {
            world
                .script_gaps_mut()
                .raise(gamemode_iw4::ScriptGapCause::RadiationTunnelLightFx);
        }
        lights.last_tunnel = tunnel;
    }
    world.radiation_lights = Some(lights);
}
