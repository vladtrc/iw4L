use crate::task::Millis;

pub const SERVER_FRAME_SECONDS: f32 = 0.05;

pub const SERVER_FRAME_MS: Millis = 50;

pub const DEFAULT_POSTDELAY_SECONDS: f32 = 2.0;

pub const MIN_MAXTIME_SECONDS: f32 = 2.0;

pub const ARCHIVE_TRIM_EPSILON: f32 = 0.0001;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CamtimeBranch {
    DvarOverride,

    ArtilleryOrStealthBomb,

    FinalKillcam,

    Javelin,

    RemoteMissile,

    NoRespawnOrLongRespawn,

    Grenade,

    Default,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CamtimeInput<'a> {
    pub now_ms: Millis,

    pub weapon: &'a str,

    pub killcam_entity_start_time: Millis,

    pub predelay: f32,

    pub showing_final_killcam: bool,

    pub time_until_respawn: f32,

    pub scr_killcam_time: Option<f32>,
}

pub fn camtime(input: &CamtimeInput) -> (f32, CamtimeBranch) {
    if let Some(forced) = input.scr_killcam_time {
        return (forced, CamtimeBranch::DvarOverride);
    }
    let w = input.weapon;
    if w == "artillery_mp" || w == "stealth_bomb_mp" {
        let elapsed = (input.now_ms - input.killcam_entity_start_time) as f32 / 1000.0;
        return (
            elapsed - input.predelay - 0.1,
            CamtimeBranch::ArtilleryOrStealthBomb,
        );
    }
    if input.showing_final_killcam {
        return (4.0, CamtimeBranch::FinalKillcam);
    }
    if w == "javelin_mp" {
        return (8.0, CamtimeBranch::Javelin);
    }
    if w.contains("remotemissile_") {
        return (5.0, CamtimeBranch::RemoteMissile);
    }

    if input.time_until_respawn == 0.0 || input.time_until_respawn > 5.0 {
        return (5.0, CamtimeBranch::NoRespawnOrLongRespawn);
    }
    if w == "frag_grenade_mp" || w == "frag_grenade_short_mp" || w == "semtex_mp" {
        return (4.25, CamtimeBranch::Grenade);
    }
    (2.5, CamtimeBranch::Default)
}

pub fn postdelay(scr_killcam_posttime: Option<f32>) -> f32 {
    match scr_killcam_posttime {
        None => DEFAULT_POSTDELAY_SECONDS,
        Some(v) if v < SERVER_FRAME_SECONDS => SERVER_FRAME_SECONDS,
        Some(v) => v,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub camtime: f32,
    pub postdelay: f32,

    pub killcamlength: f32,

    pub killcamoffset: f32,

    pub trim: MaxtimeTrim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaxtimeTrim {
    None,

    CamtimeClamped,

    PostdelayReduced,

    CamtimeReduced,

    CamtimeClampedThenReduced,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WindowPlan {
    NotShown,
    Show(Window),
}

pub fn plan_window(
    camtime_from_chain: f32,
    postdelay: f32,
    predelay: f32,
    maxtime: Option<f32>,
) -> WindowPlan {
    let mut camtime = camtime_from_chain;
    let mut postdelay = postdelay;
    let mut trim = MaxtimeTrim::None;

    if let Some(maxtime) = maxtime {
        if camtime > maxtime {
            camtime = maxtime;
            trim = MaxtimeTrim::CamtimeClamped;
        }
        if camtime < SERVER_FRAME_SECONDS {
            camtime = SERVER_FRAME_SECONDS;
            trim = MaxtimeTrim::CamtimeClamped;
        }
    }

    let mut killcamlength = camtime + postdelay;

    if let Some(maxtime) = maxtime
        && killcamlength > maxtime
    {
        if maxtime < MIN_MAXTIME_SECONDS {
            return WindowPlan::NotShown;
        }
        let second = if maxtime - camtime >= 1.0 {
            postdelay = maxtime - camtime;
            MaxtimeTrim::PostdelayReduced
        } else {
            postdelay = 1.0;
            camtime = maxtime - 1.0;
            MaxtimeTrim::CamtimeReduced
        };
        trim = if trim == MaxtimeTrim::CamtimeClamped {
            MaxtimeTrim::CamtimeClampedThenReduced
        } else {
            second
        };
        killcamlength = camtime + postdelay;
    }

    WindowPlan::Show(Window {
        camtime,
        postdelay,
        killcamlength,
        killcamoffset: camtime + predelay,
        trim,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Recalc {
    ArchiveGrew,

    Cancel,
    Continue {
        camtime: f32,
        killcamlength: f32,

        trimmed_by: f32,
    },
}

pub fn recalc_after_first_frame(
    archivetime: f32,
    killcamoffset: f32,
    predelay: f32,
    postdelay: f32,
) -> Recalc {
    if archivetime > killcamoffset + ARCHIVE_TRIM_EPSILON {
        return Recalc::ArchiveGrew;
    }
    let camtime = archivetime - SERVER_FRAME_SECONDS - predelay;
    if camtime <= 0.0 {
        return Recalc::Cancel;
    }
    Recalc::Continue {
        camtime,
        killcamlength: camtime + postdelay,
        trimmed_by: killcamoffset - archivetime,
    }
}
