use fx_iw4::{FX_PLAY_BOLT_NONE, FX_SPAWN_BOLT_NONE};

use crate::def::FxEffectDefInfo;
use crate::spawn::start_new_effect;
use crate::system::{FxBoltTarget, FxSystemHost, SpawnFail};

#[derive(Clone, Copy, Debug)]
pub struct FxPlayPose {
    pub origin: [f32; 3],

    pub axis: [[f32; 3]; 3],
    pub msec: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayResult {
    PlayedReleased { handle: u16 },

    Held { handle: u16 },
    Failed(SpawnFail),
}

impl PlayResult {
    pub fn handle(self) -> Option<u16> {
        match self {
            Self::PlayedReleased { handle } | Self::Held { handle } => Some(handle),
            Self::Failed(_) => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FxPlayRequest<'a> {
    pub def_name: &'a str,
    pub pose: FxPlayPose,

    pub wants_spotlight: bool,

    pub def: Option<FxEffectDefInfo<'a>>,

    pub catalog_index: u16,
}

pub fn play_oriented(host: &mut FxSystemHost, req: FxPlayRequest<'_>) -> PlayResult {
    play_with_bolt(host, req, None, true)
}

pub fn play_at_origin(host: &mut FxSystemHost, req: FxPlayRequest<'_>) -> PlayResult {
    let _ = FX_PLAY_BOLT_NONE;
    play_with_bolt(host, req, None, true)
}

pub fn spawn_oriented(host: &mut FxSystemHost, req: FxPlayRequest<'_>) -> PlayResult {
    play_with_bolt(host, req, None, false)
}

pub fn play_bolted(
    host: &mut FxSystemHost,
    req: FxPlayRequest<'_>,
    target: FxBoltTarget,
) -> PlayResult {
    play_with_bolt(host, req, Some(target), true)
}

fn play_with_bolt(
    host: &mut FxSystemHost,
    req: FxPlayRequest<'_>,
    target: Option<FxBoltTarget>,
    release: bool,
) -> PlayResult {
    let bolt = target
        .map(|target| target.dobj)
        .unwrap_or(FX_SPAWN_BOLT_NONE);
    match host.spawn_effect(
        req.def_name,
        req.pose.origin,
        req.pose.axis,
        req.pose.msec,
        bolt,
        req.wants_spotlight,
        req.catalog_index,
    ) {
        Err(e) => PlayResult::Failed(e),
        Ok(handle) => {
            if let Some(target) = target
                && let Some(effect) = host.slot_for_handle_mut(handle)
            {
                let teleport = fx_iw4::fx_bolt_centity_teleport_for_compare(
                    target.dobj,
                    target.centity_teleport,
                );
                effect.bolt_packed =
                    fx_iw4::fx_bolt_pack(target.dobj, teleport, u32::from(target.bone));
                effect.bolt_centity_teleport = teleport;
                effect.bolt_bone_pose = Some((target.orientation.origin, target.orientation.axis));
                effect.origin = target.orientation.origin;
                effect.axis = target.orientation.axis;
            }
            if let Some(def) = req.def {
                start_new_effect(host, handle, def);
            }
            if release {
                host.play_release_ownership(handle);
                PlayResult::PlayedReleased { handle }
            } else {
                PlayResult::Held { handle }
            }
        }
    }
}

pub fn axis_from_hit_normal(normal: [f32; 3]) -> [[f32; 3]; 3] {
    fx_iw4::fx_vector_vectors(normal)
}

pub fn axis_from_impact_velocity(pre_vel: [f32; 3]) -> [[f32; 3]; 3] {
    fx_iw4::fx_vector_vectors(pre_vel)
}

pub fn spawn_impact_or_death_effect(host: &mut FxSystemHost, req: FxPlayRequest<'_>) -> PlayResult {
    play_oriented(host, req)
}
