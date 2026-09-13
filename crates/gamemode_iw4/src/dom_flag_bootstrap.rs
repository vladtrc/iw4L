use crate::anchors;
use crate::gameobjects::gameobject_survives;
use crate::kind::GameModeKind;
use crate::use_bind::{TRIGGER_RADIUS, TriggerRadiusError};

pub const FLAG_SECONDARY: &str = "flag_secondary";

pub const DOM_FLAG_SET_USE_TIME_SECONDS: f32 = 10.0;

pub const MAX_DOM_FLAGS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DomFlagMapEnt<'a> {
    pub classname: &'a str,
    pub targetname: &'a str,
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub script_label: &'a str,
    pub gameobject: &'a str,
    pub radius: Option<f32>,
    pub height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomFlagBootstrapError {
    NotEnoughFlags { count: usize },
    TooManyFlags,
    UnsupportedClassname,
    MissingRadius,
    MissingHeight,
}

impl From<TriggerRadiusError> for DomFlagBootstrapError {
    fn from(err: TriggerRadiusError) -> Self {
        match err {
            TriggerRadiusError::MissingRadius => Self::MissingRadius,
            TriggerRadiusError::MissingHeight => Self::MissingHeight,
        }
    }
}

pub fn collect_dom_flag_indices(
    ents: &[DomFlagMapEnt<'_>],
    out: &mut [usize],
) -> Result<usize, DomFlagBootstrapError> {
    let mut n = 0;
    n = append_target(ents, anchors::FLAG_PRIMARY, out, n)?;
    n = append_target(ents, FLAG_SECONDARY, out, n)?;
    if n < 2 {
        return Err(DomFlagBootstrapError::NotEnoughFlags { count: n });
    }
    Ok(n)
}

fn append_target(
    ents: &[DomFlagMapEnt<'_>],
    targetname: &str,
    out: &mut [usize],
    mut n: usize,
) -> Result<usize, DomFlagBootstrapError> {
    for (index, ent) in ents.iter().enumerate() {
        if ent.targetname != targetname {
            continue;
        }
        if !gameobject_survives(ent.gameobject, GameModeKind::Domination) {
            continue;
        }
        if ent.classname != TRIGGER_RADIUS {
            return Err(DomFlagBootstrapError::UnsupportedClassname);
        }
        if ent.radius.is_none() {
            return Err(DomFlagBootstrapError::MissingRadius);
        }
        if ent.height.is_none() {
            return Err(DomFlagBootstrapError::MissingHeight);
        }
        if n >= out.len() {
            return Err(DomFlagBootstrapError::TooManyFlags);
        }
        out[n] = index;
        n += 1;
    }
    Ok(n)
}
