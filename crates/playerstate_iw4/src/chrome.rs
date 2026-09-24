use crate::ENTITYNUM_NONE;

pub const KILLCAM_DEFAULT_LERP_MS: i32 = 300;

pub const KILLCAM_TURRET_LERP_MS: i32 = 900;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillcamEnterStep {
    ResetLocals,

    ResetTurretFx,

    SelectWeaponIndex,

    EnterFxPass,

    ResetScriptMoverTrees,

    StopExplosionFx,

    FreeActiveFx,
}

pub const KILLCAM_ENTER_STEPS: [KillcamEnterStep; 7] = [
    KillcamEnterStep::ResetLocals,
    KillcamEnterStep::ResetTurretFx,
    KillcamEnterStep::SelectWeaponIndex,
    KillcamEnterStep::EnterFxPass,
    KillcamEnterStep::ResetScriptMoverTrees,
    KillcamEnterStep::StopExplosionFx,
    KillcamEnterStep::FreeActiveFx,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillcamExitStep {
    ResetTurretFx,

    ClearInKillCam,

    EnterFxPass,

    ResetScriptMoverTrees,

    RestoreSnapEntities,

    FreeActiveFx,
}

pub const KILLCAM_EXIT_STEPS: [KillcamExitStep; 6] = [
    KillcamExitStep::ResetTurretFx,
    KillcamExitStep::ClearInKillCam,
    KillcamExitStep::EnterFxPass,
    KillcamExitStep::ResetScriptMoverTrees,
    KillcamExitStep::RestoreSnapEntities,
    KillcamExitStep::FreeActiveFx,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KillCamMode {
    Mode0 = 0,
    Mode1Heli = 1,
    Mode2Airstrike = 2,
    Mode3Missile = 3,
    Mode4MissileAlt = 4,
    Mode5Rocket = 5,
    Mode6Turret = 6,
    Mode7Javelin = 7,
    Mode8Remote = 8,
}

impl KillCamMode {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

#[inline]
pub fn mode0_from_kill_cam_entity(kill_cam_entity: i32) -> Option<KillCamMode> {
    if kill_cam_entity == ENTITYNUM_NONE {
        Some(KillCamMode::Mode0)
    } else {
        None
    }
}

pub fn killcam_lerp_deadline_ms(
    on_enter_frame: bool,
    focus_changed: bool,
    mode: KillCamMode,
    now_ms: i32,
) -> i32 {
    if on_enter_frame || !focus_changed {
        return 0;
    }
    let span = if mode == KillCamMode::Mode6Turret {
        KILLCAM_TURRET_LERP_MS
    } else {
        KILLCAM_DEFAULT_LERP_MS
    };
    now_ms.saturating_add(span)
}

#[inline]
pub fn third_person_in_killcam(in_killcam: bool, mode: KillCamMode) -> bool {
    in_killcam && mode != KillCamMode::Mode0
}
