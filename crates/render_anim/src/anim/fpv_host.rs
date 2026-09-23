use std::sync::Arc;

use bevy::math::{Mat4, Vec3};
use bevy::prelude::*;

use crate::anim::fpv::{
    EquippedFpv, FpvAuthoritySample, FpvPresentState, tick_equipped_fpv_with_predicted_fire,
};
use crate::anim::fpv_pose::{FpvBoltFrame, PosedClip};
use crate::anim::fpv_prepared::FpvRigSet;
use crate::anim::fpv_rig::{FpvHandPose, PreparedFpvRig};
use assets::FpvMeshIndex;

#[derive(Resource, Default)]
pub struct FpvPresentCursor(pub FpvPresentState);

#[derive(Resource, Default)]
pub struct PendingFpvSpawn(pub Option<PendingFpvSpawnRequest>);

#[derive(Clone, Debug)]
pub struct PendingFpvSpawnRequest {
    pub gun_index: FpvMeshIndex,
    pub catalog_id: u64,
    pub weapon_id: u32,
}

#[derive(Resource, Default)]
pub struct PendingFpvNotetracks {
    pub weapon: u32,
    pub names: Vec<String>,
}

#[derive(Resource, Default)]
pub struct FpvHeldSettled(pub Option<u32>);

#[derive(Resource, Default)]
pub struct FpvHeldLife(pub Option<u32>);

#[derive(Resource, Default)]
pub struct FpvBoltTargets {
    pub pose: [Option<FpvBoltFrame>; 2],
    pub flash: [Option<fx::FxBoltTarget>; 2],
    pub brass: [Option<fx::FxBoltTarget>; 2],
    pub knife: [Option<fx::FxBoltTarget>; 2],
    pub laser: [Option<fx::FxBoltTarget>; 2],
}

impl FpvBoltTargets {
    pub fn clear(&mut self) {
        self.pose = [None, None];
        self.flash = [None, None];
        self.brass = [None, None];
        self.knife = [None, None];
        self.laser = [None, None];
    }

    pub fn set_pose(&mut self, hand: usize, bolt: FpvBoltFrame) {
        self.pose[hand] = Some(bolt);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FpvPoseRefuse {
    CatalogMissing,
    NoActiveClips,
    EyePoseFailed {
        gun_xmodel: String,
    },
    DependencyUnresolved {
        weapon_id: u32,
        role: &'static str,
        name: String,
    },
}

pub enum FpvPoseKind {
    Hide,
    Refuse(FpvPoseRefuse),
    Posed(FpvPosedFrame),
}

impl Default for FpvPoseKind {
    fn default() -> Self {
        Self::Hide
    }
}

/// What one frame asked of the rig: bones per hand, and the lens the right hand
/// carries. No geometry — the surfaces this rig draws were settled when it was
/// prepared, and the vertices are written straight into the published plan.
pub struct FpvPosedFrame {
    pub poses: [Option<FpvHandPose>; 2],
    pub lens: Mat4,
    pub idle_sampled: bool,
    pub notetracks: Vec<String>,
}

#[derive(Resource, Default)]
pub struct FpvPoseProduct {
    pub drawgun: Option<i32>,
    pub kind: FpvPoseKind,
}

pub struct FpvGenerateArgs<'a> {
    pub dt: f32,
    pub equipped: &'a mut EquippedFpv,
    pub rigs: &'a FpvRigSet,
    pub active: &'a mut Option<Arc<PreparedFpvRig>>,
    pub cursor: &'a mut FpvPresentState,
    pub rocket: bool,
    pub sample: Option<FpvAuthoritySample>,
    pub predicted_fire: bool,
    pub dual: bool,
    pub dual_offset: Option<f32>,
}

pub fn generate_fpv_pose(args: FpvGenerateArgs<'_>) -> FpvPoseKind {
    let FpvGenerateArgs {
        dt,
        equipped,
        rigs,
        active,
        cursor,
        rocket,
        sample,
        predicted_fire,
        dual,
        dual_offset,
    } = args;
    let (_pose, notifies) =
        tick_equipped_fpv_with_predicted_fire(equipped, cursor, sample, predicted_fire, dt);

    let right: Vec<PosedClip<'_>> = equipped
        .controller
        .active_anims()
        .map(|a| PosedClip {
            node: a.node,
            clip: a.clip,
            time: a.time,
            weight: a.weight,
        })
        .collect();
    if right.is_empty() {
        return FpvPoseKind::Refuse(FpvPoseRefuse::NoActiveClips);
    }
    let left: Vec<PosedClip<'_>> = match (dual, equipped.left.as_ref()) {
        (true, Some(left)) => left
            .active_anims()
            .map(|a| PosedClip {
                node: a.node,
                clip: a.clip,
                time: a.time,
                weight: a.weight,
            })
            .collect(),
        _ => Vec::new(),
    };
    let dual_drawn = !left.is_empty();

    let Some(prepared) = rigs.pick(rocket, dual_drawn) else {
        *active = None;
        return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
            gun_xmodel: equipped.gun_xmodel.clone(),
        });
    };
    if active
        .as_ref()
        .is_none_or(|current| current.generation() != prepared.generation())
    {
        *active = Some(Arc::clone(prepared));
    }

    let Some(right_pose) = prepared.pose_hand(0, &right, Vec3::ZERO) else {
        return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
            gun_xmodel: equipped.gun_xmodel.clone(),
        });
    };
    let lens = right_pose.lens;
    let left_pose = if dual_drawn {
        let offset = dual_offset
            .filter(|offset| *offset != 0.0)
            .map(|offset| Vec3::new(offset, 0.0, 0.0))
            .unwrap_or(Vec3::ZERO);
        // The rig laid out a left hand, so a left hand that cannot be posed is
        // a plan with a hole in it. Refusing the frame is the honest answer.
        let Some(pose) = prepared.pose_hand(1, &left, offset) else {
            return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
                gun_xmodel: equipped.gun_xmodel.clone(),
            });
        };
        Some(pose)
    } else {
        None
    };

    let poses = [Some(right_pose), left_pose];
    FpvPoseKind::Posed(FpvPosedFrame {
        poses,
        lens,
        idle_sampled: true,
        notetracks: notifies,
    })
}
