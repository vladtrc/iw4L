use bevy::math::{Mat4, Vec3};
use bevy::prelude::*;
use bevy::render::mesh::Mesh;

use crate::anim::fpv::{
    EquippedFpv, FpvAuthoritySample, FpvPresentState, tick_equipped_fpv_with_predicted_fire,
};
use crate::anim::fpv_pose::{FpvBoltFrame, PosedClip, PosedModelSurface, pose_eye_blended};
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

pub struct FpvPosedFrame {
    pub hands: Vec<PosedModelSurface>,
    pub gun: Vec<PosedModelSurface>,
    pub lens: Mat4,
    pub bolts: [Option<FpvBoltFrame>; 2],
    pub idle_sampled: bool,
    pub notetracks: Vec<String>,
    pub scope_xmodel: Option<String>,
}

#[derive(Resource, Default)]
pub struct FpvPoseProduct {
    pub drawgun: Option<i32>,
    pub kind: FpvPoseKind,
}

pub struct FpvGenerateArgs<'a> {
    pub dt: f32,
    pub equipped: &'a mut EquippedFpv,
    pub cursor: &'a mut FpvPresentState,
    pub catalog: &'a FpvMeshCatalog,
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
        cursor,
        catalog,
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
    let anims: Vec<PosedClip<'_>> = equipped
        .controller
        .active_anims()
        .map(|a| PosedClip {
            clip: a.clip,
            time: a.time,
            weight: a.weight,
        })
        .collect();
    if anims.is_empty() {
        return FpvPoseKind::Refuse(FpvPoseRefuse::NoActiveClips);
    }
    let Some(posed) = pose_eye_blended(
        catalog,
        equipped.namespace,
        &equipped.gun_xmodel,
        &equipped.hands,
        &anims,
        hide_tags,
        scope_name,
        rocket_name,
    ) else {
        return FpvPoseKind::Refuse(FpvPoseRefuse::EyePoseFailed {
            gun_xmodel: equipped.gun_xmodel.clone(),
        });
    };
    drop(anims);
    let (mut hands, mut gun, stats, lens) = (posed.hands, posed.gun, posed.stats, posed.lens);
    let mut bolts = [Some(posed.bolt), None];
    if dual {
        if let Some(left) = equipped.left.as_ref() {
            let left_anims: Vec<PosedClip<'_>> = left
                .active_anims()
                .map(|a| PosedClip {
                    clip: a.clip,
                    time: a.time,
                    weight: a.weight,
                })
                .collect();
            if !left_anims.is_empty() {
                if let Some(left_posed) = pose_eye_blended(
                    catalog,
                    equipped.namespace,
                    &equipped.gun_xmodel,
                    &equipped.hands,
                    &left_anims,
                    hide_tags,
                    None,
                    None,
                ) {
                    let (mut lh, mut lg, mut left_bolt) =
                        (left_posed.hands, left_posed.gun, left_posed.bolt);
                    if let Some(offset) = dual_offset.filter(|o| *o != 0.0) {
                        translate_posed_surfaces(&mut lh, Vec3::new(offset, 0.0, 0.0));
                        translate_posed_surfaces(&mut lg, Vec3::new(offset, 0.0, 0.0));
                        let shift = Mat4::from_translation(Vec3::new(offset, 0.0, 0.0));
                        for bone in &mut left_bolt.bones {
                            *bone = shift * *bone;
                        }
                    }
                    bolts[1] = Some(left_bolt);
                    hands.extend(lh);
                    gun.extend(lg);
                }
            }
        }
    }
    FpvPoseKind::Posed(FpvPosedFrame {
        hands,
        gun,
        lens,
        bolts,
        idle_sampled: stats.idle_sampled,
        notetracks: notifies,
        scope_xmodel: scope_name.map(str::to_owned),
    })
}

pub fn translate_posed_surfaces(surfaces: &mut [PosedModelSurface], delta: Vec3) {
    if delta == Vec3::ZERO {
        return;
    }
    for surface in surfaces {
        if let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(pos)) =
            surface.mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
        {
            for p in pos.iter_mut() {
                p[0] += delta.x;
                p[1] += delta.y;
                p[2] += delta.z;
            }
        }
        for row in &mut surface.packed_vertices {
            let mut xyz = [0.0f32; 3];
            xyz[0] = f32::from_le_bytes([row[0], row[1], row[2], row[3]]) + delta.x;
            xyz[1] = f32::from_le_bytes([row[4], row[5], row[6], row[7]]) + delta.y;
            xyz[2] = f32::from_le_bytes([row[8], row[9], row[10], row[11]]) + delta.z;
            row[0..4].copy_from_slice(&xyz[0].to_le_bytes());
            row[4..8].copy_from_slice(&xyz[1].to_le_bytes());
            row[8..12].copy_from_slice(&xyz[2].to_le_bytes());
        }
    }
}
