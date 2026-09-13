use crate::use_prox::GameObjectTeam;

pub const STATUS_DIALOG_DEBOUNCE_MS: i32 = 5_000;

pub const WAYPOINT_CAPTURE_PREFIX: &str = "waypoint_capture";

pub const SOUND_OBJECTIVE_TAKEN: &str = "mp_war_objective_taken";
pub const SOUND_OBJECTIVE_LOST: &str = "mp_war_objective_lost";

const LABEL_CAP: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScriptLabel {
    bytes: [u8; LABEL_CAP],
    len: u8,
}

impl ScriptLabel {
    pub const fn empty() -> Self {
        Self {
            bytes: [0; LABEL_CAP],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

pub fn get_label(script_label: &str) -> ScriptLabel {
    let mut out = ScriptLabel::empty();
    if script_label.is_empty() {
        return out;
    }
    let prefix = script_label.as_bytes()[0] != b'_';
    let need = script_label.len() + usize::from(prefix);
    if need > LABEL_CAP {
        return out;
    }
    if prefix {
        out.bytes[0] = b'_';
        out.len = 1;
    }
    let start = out.len as usize;
    out.bytes[start..start + script_label.len()].copy_from_slice(script_label.as_bytes());
    out.len = need as u8;
    out
}

pub fn other_team(team: GameObjectTeam) -> Option<GameObjectTeam> {
    match team {
        GameObjectTeam::Axis => Some(GameObjectTeam::Allies),
        GameObjectTeam::Allies => Some(GameObjectTeam::Axis),
        GameObjectTeam::Neutral | GameObjectTeam::None => None,
    }
}

pub fn status_dialog_allowed(now_ms: i32, last_status_ms: i32, force: bool) -> bool {
    if force {
        return true;
    }
    now_ms >= last_status_ms.saturating_add(STATUS_DIALOG_DEBOUNCE_MS)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomStatusDialogKind {
    Secured,
    EnemyHas,
    SecureAll,
    LostAll,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DomStatusLine {
    pub kind: DomStatusDialogKind,
    pub team: GameObjectTeam,
    pub label: ScriptLabel,
}

pub fn on_use_status_lines(
    old_owner: GameObjectTeam,
    new_owner: GameObjectTeam,
    owned_count: i32,
    flags_size: i32,
    label: ScriptLabel,
) -> Option<[DomStatusLine; 2]> {
    if old_owner == GameObjectTeam::Neutral {
        let other = other_team(new_owner)?;
        return Some([
            DomStatusLine {
                kind: DomStatusDialogKind::Secured,
                team: new_owner,
                label,
            },
            DomStatusLine {
                kind: DomStatusDialogKind::EnemyHas,
                team: other,
                label,
            },
        ]);
    }
    let old = match old_owner {
        GameObjectTeam::Axis | GameObjectTeam::Allies => old_owner,
        GameObjectTeam::Neutral | GameObjectTeam::None => return None,
    };
    if owned_count == flags_size {
        Some([
            DomStatusLine {
                kind: DomStatusDialogKind::SecureAll,
                team: new_owner,
                label,
            },
            DomStatusLine {
                kind: DomStatusDialogKind::LostAll,
                team: old,
                label,
            },
        ])
    } else {
        Some([
            DomStatusLine {
                kind: DomStatusDialogKind::Secured,
                team: new_owner,
                label,
            },
            DomStatusLine {
                kind: DomStatusDialogKind::Lost,
                team: old,
                label,
            },
        ])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnUseSounds {
    pub taken: &'static str,
    pub lost: Option<&'static str>,
    pub taken_team: GameObjectTeam,
    pub lost_team: Option<GameObjectTeam>,
}

pub fn on_use_sounds(old_owner: GameObjectTeam, new_owner: GameObjectTeam) -> OnUseSounds {
    if old_owner == GameObjectTeam::Neutral {
        OnUseSounds {
            taken: SOUND_OBJECTIVE_TAKEN,
            lost: None,
            taken_team: new_owner,
            lost_team: other_team(new_owner),
        }
    } else {
        OnUseSounds {
            taken: SOUND_OBJECTIVE_TAKEN,
            lost: Some(SOUND_OBJECTIVE_LOST),
            taken_team: new_owner,
            lost_team: Some(old_owner),
        }
    }
}
