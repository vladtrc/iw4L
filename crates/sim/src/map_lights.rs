#[derive(Clone, Debug, Default, PartialEq)]
pub struct RadiationLights {
    pub switch_count: u32,
    pub tunnel_count: u32,
    pub last_switch: gamemode_iw4::SwitchLightPhase,
    pub last_tunnel: gamemode_iw4::TunnelLightPhase,
}
