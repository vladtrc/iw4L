pub const TEAM_FREE: i32 = 0;
pub const TEAM_AXIS: i32 = 1;
pub const TEAM_ALLIES: i32 = 2;
pub const TEAM_SPECTATOR: i32 = 3;

#[must_use]
pub fn cg_get_team_name(team: i32) -> &'static str {
    match team {
        TEAM_FREE => "TEAM_FREE",
        TEAM_AXIS => "TEAM_AXIS",
        TEAM_ALLIES => "TEAM_ALLIES",
        TEAM_SPECTATOR => "TEAM_SPECTATOR",
        _ => "",
    }
}

pub fn client_state_team_from_sessionteam(team_based: bool, sessionteam: &str) -> i32 {
    if !team_based {
        return TEAM_FREE;
    }
    match sessionteam {
        "axis" => TEAM_AXIS,
        "allies" => TEAM_ALLIES,
        "spectator" => TEAM_SPECTATOR,
        _ => TEAM_FREE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientState {
    pub team: i32,
    pub modelindex: i32,
    pub dual_wielding: i32,
    pub riot_shield_next: i32,
    pub attach_model_index_0: i32,
    pub attach_model_index_1: i32,
    pub attach_model_index_2: i32,
    pub attach_model_index_3: i32,
    pub attach_model_index_4: i32,
    pub attach_model_index_5: i32,
    pub attach_tag_index_0: i32,
    pub attach_tag_index_1: i32,
    pub attach_tag_index_2: i32,
    pub attach_tag_index_3: i32,
    pub attach_tag_index_4: i32,
    pub attach_tag_index_5: i32,
    pub name_0: i32,
    pub name_4: i32,
    pub name_8: i32,
    pub name_12: i32,
    pub max_sprint_time_multiplier: f32,
    pub prestige: i32,
    pub dive_state: i32,
    pub voice_connectivity_bits: u32,
    pub player_card_icon: u32,
    pub player_card_title: u32,
    pub player_card_nameplate: u32,
}

pub const CLIENT_STATE_NAME_LEN: usize = 16;

pub fn client_state_name_bytes(name_0: i32, name_4: i32, name_8: i32, name_12: i32) -> [u8; 16] {
    let mut bytes = [0u8; CLIENT_STATE_NAME_LEN];
    bytes[0..4].copy_from_slice(&name_0.to_le_bytes());
    bytes[4..8].copy_from_slice(&name_4.to_le_bytes());
    bytes[8..12].copy_from_slice(&name_8.to_le_bytes());
    bytes[12..16].copy_from_slice(&name_12.to_le_bytes());
    bytes
}

pub fn client_state_name(bytes: &[u8; CLIENT_STATE_NAME_LEN]) -> Option<&str> {
    let end = bytes
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(CLIENT_STATE_NAME_LEN);
    if end == 0 {
        return None;
    }
    core::str::from_utf8(&bytes[..end])
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn pack_client_state_name(s: &str) -> [u8; CLIENT_STATE_NAME_LEN] {
    let mut bytes = [0u8; CLIENT_STATE_NAME_LEN];
    let cap = CLIENT_STATE_NAME_LEN - 1;
    let mut n = 0usize;
    for b in s.as_bytes() {
        if n >= cap {
            break;
        }
        bytes[n] = *b;
        n += 1;
    }
    bytes
}
