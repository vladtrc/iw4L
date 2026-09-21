use std::collections::HashMap;

use bevy::math::{Mat4, Vec3};
use bevy::prelude::*;
use render_scene::SmodelPassMaterial;

use crate::anim::fpv::{
    EquippedFpv, FpvAuthoritySample, FpvPresentState, tick_equipped_fpv_with_predicted_fire,
};
use crate::anim::fpv_pose::{FpvBoltFrame, PosedClip};
use crate::anim::fpv_rig::{FpvHandPose, FpvRigInputs, FpvRigKey, PreparedFpvRig};
use assets::FpvMeshCatalog;

#[derive(Resource, Default)]
pub struct FpvPresentCursor(pub FpvPresentState);

#[derive(Resource, Default)]
pub struct PendingFpvSpawn(pub Option<PendingFpvSpawnRequest>);

#[derive(Clone, Debug)]
pub struct PendingFpvSpawnRequest {
    pub gun_xmodel: String,
    pub weapon_id: u32,
    pub idle_anim: Option<String>,
    pub from_gun_xmodel: bool,
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
    EyePoseFailed { gun_xmodel: String },
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
    pub rig: &'a mut Option<PreparedFpvRig>,
    pub cursor: &'a mut FpvPresentState,
    pub catalog: &'a FpvMeshCatalog,
    pub materials: &'a [SmodelPassMaterial],
    pub material_by_authored: &'a HashMap<usize, u32>,
    pub hide_tags: &'a [String],
    pub scope_name: Option<&'a str>,
    pub rocket_name: Option<&'a str>,
    pub sample: Option<FpvAuthoritySample>,
    pub predicted_fire: bool,
    pub dual: bool,
    pub dual_offset: Option<f32>,
}

pub fn generate_fpv_pose(args: FpvGenerateArgs<'_>) -> FpvPoseKind {
    let FpvGenerateArgs {
        dt,
        equipped,
        rig,
        cursor,
        catalog,
        materials,
        material_by_authored,
        hide_tags,
        scope_name,
        rocket_name,
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

    let composed = rig.as_ref().is_some_and(|rig| {
        rig.matches(
            equipped.namespace,
            &equipped.gun_xmodel,
            &equipped.hands,
            scope_name,
            rocket_name,
            hide_tags,
            dual_drawn,
        )
    });
    if !composed {
        let key = FpvRigKey {
            namespace: equipped.namespace,
            gun: equipped.gun_xmodel.clone(),
            hands: equipped.hands.clone(),
            scope: scope_name.map(str::to_owned),
            rocket: rocket_name.map(str::to_owned),
            hide_tags: hide_tags.to_vec(),
            dual: dual_drawn,
        };
        match PreparedFpvRig::build(
            key,
            FpvRigInputs {
                catalog,
                materials,
                material_by_authored,
            },
        ) {
            Ok(prepared) => {
                diag::info!(
                    Fpv,
                    "fpv: prepared rig `{}` — {} draws, {} vertices, {} surfaces skipped{}",
                    equipped.gun_xmodel,
                    prepared.geometry.plan_draw_n,
                    prepared.geometry.dest_n,
                    prepared.geometry.plan_skip_n,
                    if dual_drawn { ", dual" } else { "" }
                );
                *rig = Some(prepared);
            }
            Err(error) => {
                diag::info!(
                    Fpv,
                    "fpv: cannot prepare `{}` — {error}",
                    equipped.gun_xmodel
                );
                *rig = None;
                return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
                    gun_xmodel: equipped.gun_xmodel.clone(),
                });
            }
        }
    }
    let Some(prepared) = rig.as_mut() else {
        return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
            gun_xmodel: equipped.gun_xmodel.clone(),
        });
    };

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
