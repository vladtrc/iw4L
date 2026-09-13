use crate::notify::NotifyKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupEntry {
    NormalEnd { clear_state: bool },

    Spawned { clear_state: bool },

    GameEnded { clear_state: bool },
}

impl CleanupEntry {
    pub const fn clear_state(self) -> bool {
        match self {
            CleanupEntry::NormalEnd { clear_state }
            | CleanupEntry::Spawned { clear_state }
            | CleanupEntry::GameEnded { clear_state } => clear_state,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CleanupStep {
    HideHud,

    ClearKillcamFlag,

    ClearLowerMessage,

    RestoreSpectatePermissions,

    NotifyKillcamEnded,

    ClearKillcamState,
}

pub fn killcam_cleanup_steps(entry: CleanupEntry, out: &mut [CleanupStep; 6]) -> usize {
    out[0] = CleanupStep::HideHud;
    out[1] = CleanupStep::ClearKillcamFlag;
    out[2] = CleanupStep::ClearLowerMessage;
    out[3] = CleanupStep::RestoreSpectatePermissions;
    out[4] = CleanupStep::NotifyKillcamEnded;
    if entry.clear_state() {
        out[5] = CleanupStep::ClearKillcamState;
        6
    } else {
        5
    }
}

pub const CLEANUP_NOTIFY: NotifyKind = NotifyKind::KillcamEnded;
