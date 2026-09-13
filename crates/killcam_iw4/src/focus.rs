use crate::camtime::SERVER_FRAME_SECONDS;
use crate::task::Millis;

pub const NO_KILLCAM_ENTITY: i32 = -1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entity {
    pub entity_number: i32,

    pub birthtime: Option<Millis>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Inflictor<'a> {
    pub entity: Entity,

    pub is_attacker: bool,
    pub classname: &'a str,

    pub script_gameobjectname: Option<&'a str>,

    pub kill_cam_ent: Option<Entity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusRule {
    NoInflictor,

    InflictorIsAttacker,

    Ac130,

    CobraMinigun,

    AirstrikeCamEnt,

    BombzoneCamEnt,

    ScriptEntityLooksBad,

    RemoteMissile,

    Ac130Duplicate,

    Inflictor,
}

pub fn get_killcam_entity(
    inflictor: Option<Inflictor>,
    weapon: &str,
) -> (Option<Entity>, FocusRule) {
    let Some(inflictor) = inflictor else {
        return (None, FocusRule::NoInflictor);
    };
    if inflictor.is_attacker {
        return (None, FocusRule::InflictorIsAttacker);
    }
    if weapon.contains("ac130_") {
        return (None, FocusRule::Ac130);
    }
    if weapon == "cobra_player_minigun_mp" {
        return (None, FocusRule::CobraMinigun);
    }
    if weapon == "artillery_mp" || weapon == "stealth_bomb_mp" || weapon == "pavelow_minigun_mp" {
        return (inflictor.kill_cam_ent, FocusRule::AirstrikeCamEnt);
    }
    if inflictor.script_gameobjectname == Some("bombzone") {
        return (inflictor.kill_cam_ent, FocusRule::BombzoneCamEnt);
    }
    if matches!(
        inflictor.classname,
        "script_origin" | "script_model" | "script_brushmodel"
    ) {
        return (None, FocusRule::ScriptEntityLooksBad);
    }
    if weapon.contains("remotemissile_") {
        return (None, FocusRule::RemoteMissile);
    }
    if weapon.contains("ac130_") {
        return (None, FocusRule::Ac130Duplicate);
    }
    (Some(inflictor.entity), FocusRule::Inflictor)
}

pub fn killcam_entity_index(focus: Option<Entity>) -> (i32, Millis) {
    match focus {
        None => (NO_KILLCAM_ENTITY, 0),
        Some(e) => (e.entity_number, e.birthtime.unwrap_or(0)),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FocusDelay {
    SetNow,

    WaitOneFrameThenRecheck,

    WaitSeconds(f32),
}

pub fn archive_clock_ms(now_ms: Millis, offset_seconds: f32) -> f32 {
    now_ms as f32 - offset_seconds * 1000.0
}

pub fn focus_first_check(now_ms: Millis, killcamoffset: f32, starttime: Millis) -> FocusDelay {
    if starttime as f32 > archive_clock_ms(now_ms, killcamoffset) {
        FocusDelay::WaitOneFrameThenRecheck
    } else {
        FocusDelay::SetNow
    }
}

pub fn focus_second_check(now_ms: Millis, archivetime: f32, starttime: Millis) -> FocusDelay {
    let clock = archive_clock_ms(now_ms, archivetime);
    if starttime as f32 > clock {
        FocusDelay::WaitSeconds((starttime as f32 - clock) / 1000.0)
    } else {
        FocusDelay::SetNow
    }
}

pub const FIRST_RECHECK_DELAY_SECONDS: f32 = SERVER_FRAME_SECONDS;
