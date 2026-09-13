use bevy::math::{Mat3, Mat4, Quat, Vec3, Vec4};

use crate::anim::pose_types::{
    AnimClip, AnimInstance, AssetNamespace, Attach, DObj, DObjError, FpvMeshCatalog, FpvSkel,
    ModelPoseSrc, PoseStats,
};
pub use crate::anim::xmodel_pose::{FpvSurfOwner, PosedModelSurface};
use crate::anim::xmodel_pose::{mark_fpv_owner, skin_model};
use assets::FpvHands;

const SCOPE_ATTACH_TAGS: &[&str] = &[
    "tag_scope",
    "tag_acog",
    "tag_red_dot",
    "tag_reflex",
    "tag_hybrid",
    "tag_thermal",
    "tag_thermal_scope",
    "tag_eotech",
];

pub fn tag_view_to_bevy_mat3() -> Mat3 {
    Mat3::from_cols(
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(-1.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    )
}

pub fn tag_view_to_bevy_camera() -> Mat4 {
    let f = tag_view_to_bevy_mat3();
    Mat4::from_cols(
        f.x_axis.extend(0.0),
        f.y_axis.extend(0.0),
        f.z_axis.extend(0.0),
        Vec4::W,
    )
}

pub fn placement_angles_to_bevy_camera_quat(angles_deg: [f32; 3]) -> Quat {
    let [fwd, left, up] = math_iw4::angles_to_axis(angles_deg);
    let r = Mat3::from_cols(
        Vec3::from_array(fwd),
        Vec3::from_array(left),
        Vec3::from_array(up),
    );
    let f = tag_view_to_bevy_mat3();
    Quat::from_mat3(&(f * r * f.inverse()))
}

#[inline]
pub fn tag_camera_lens_local(view_world: Mat4, camera_world: Mat4) -> Mat4 {
    let fold = tag_view_to_bevy_camera();
    let eye_from_world = fold * view_world.inverse();
    eye_from_world * camera_world * fold.inverse()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FpvBoltTags {
    pub flash: Option<u16>,
    pub flash_silenced: Option<u16>,
    pub brass: Option<u16>,

    pub knife: Option<u16>,

    pub laser: Option<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct FpvBoltFrame {
    pub bones: Vec<Mat4>,
    pub tags: FpvBoltTags,
}

pub struct PosedFpvEye {
    pub hands: Vec<PosedModelSurface>,
    pub gun: Vec<PosedModelSurface>,

    pub lens: Mat4,
    pub bolt: FpvBoltFrame,
    pub stats: PoseStats,
}

fn bolt_tag_bone(dobj: &DObj, tag: &str) -> Option<u16> {
    dobj.find(tag).and_then(|index| u16::try_from(index).ok())
}

pub fn pose_fpv_to_eye(
    hands: &FpvSkel,
    gun: Option<&FpvSkel>,
    scope: Option<&FpvSkel>,
    rocket: Option<&FpvSkel>,
    gun_hide_tags: &[String],
) -> Option<(Vec<PosedModelSurface>, Option<Vec<PosedModelSurface>>, Mat4)> {
    let view_i = hands.tag_view?;
    let tv = hands.bones.get(view_i)?.to_mat4();
    let eye_from_model = tag_view_to_bevy_camera() * tv.inverse();
    let lens = hands
        .tag_camera()
        .and_then(|ci| {
            hands
                .bones
                .get(ci)
                .map(|b| tag_camera_lens_local(tv, b.to_mat4()))
        })
        .unwrap_or(Mat4::IDENTITY);

    let mut hands_surfaces = skin_model(
        hands,
        |bone| eye_from_model * hands.bones[bone].to_mat4(),
        &[],
    )?;
    mark_fpv_owner(&mut hands_surfaces, FpvSurfOwner::Hands);

    let mut gun_surfaces = match gun {
        Some(gun) => {
            let weapon_i = hands.tag_weapon?;
            let tw = hands.bones.get(weapon_i)?.to_mat4();
            let gun_root = gun.bones.first()?.to_mat4();
            let attach = tw * gun_root.inverse();
            let mut surfaces = skin_model(
                gun,
                |bone| eye_from_model * attach * gun.bones[bone].to_mat4(),
                gun_hide_tags,
            )?;
            mark_fpv_owner(&mut surfaces, FpvSurfOwner::Gun);
            Some(surfaces)
        }
        None => None,
    };
    if let (Some(gun), Some(scope), Some(surfaces)) = (gun, scope, gun_surfaces.as_mut()) {
        if let Some(mut extra) = bind_scope_surfaces(hands, gun, scope, eye_from_model) {
            mark_fpv_owner(&mut extra, FpvSurfOwner::Scope);
            surfaces.extend(extra);
        }
    }
    if let (Some(gun), Some(rocket), Some(surfaces)) = (gun, rocket, gun_surfaces.as_mut()) {
        if let Some(mut extra) = bind_rocket_surfaces(hands, gun, rocket, eye_from_model) {
            mark_fpv_owner(&mut extra, FpvSurfOwner::Rocket);
            surfaces.extend(extra);
        }
    }
    Some((hands_surfaces, gun_surfaces, lens))
}

#[derive(Clone, Copy)]
pub struct PosedClip<'a> {
    pub clip: &'a AnimClip,
    pub time: f32,
    pub weight: f32,
}

pub fn pose_fpv_anims(
    hands: &FpvSkel,
    gun: &FpvSkel,
    scope: Option<&FpvSkel>,
    rocket: Option<&FpvSkel>,
    anims: &[PosedClip<'_>],
    gun_hide_tags: &[String],
) -> Option<PosedFpvEye> {
    let hands_pose = hands.pose.as_ref()?;
    let gun_pose = gun.pose.as_ref()?;
    let scope_pose = scope.and_then(|s| s.pose.as_ref());
    let rocket_pose = rocket.and_then(|s| s.pose.as_ref());
    let (dobj, extras) =
        build_fpv_dobj(hands_pose, gun_pose, scope_pose, rocket_pose, scope, rocket).ok()?;
    let view_i = dobj.find("tag_view")?;
    let tracks: Vec<Vec<Option<usize>>> = anims.iter().map(|a| dobj.tracks_for(a.clip)).collect();
    if !tracks.iter().flatten().any(|bone| bone.is_some()) {
        return None;
    }
    let parts = dobj.all_parts();
    let instances: Vec<AnimInstance<'_>> = anims
        .iter()
        .zip(tracks.iter())
        .map(|(a, tracks)| AnimInstance {
            clip: a.clip,
            tracks,
            time: a.time,
            weight: a.weight,
            parts: None,
        })
        .collect();
    let world = dobj.pose(&instances, &parts, Mat4::IDENTITY);
    let skin = dobj.skin_matrices(&world);
    let eye_from_world = tag_view_to_bevy_camera() * world[view_i].inverse();
    let lens = dobj
        .find("tag_camera")
        .map(|ci| tag_camera_lens_local(world[view_i], world[ci]))
        .unwrap_or(Mat4::IDENTITY);

    let bolt = FpvBoltFrame {
        bones: world.iter().map(|bone| eye_from_world * *bone).collect(),
        tags: FpvBoltTags {
            flash: bolt_tag_bone(&dobj, "tag_flash"),
            flash_silenced: bolt_tag_bone(&dobj, "tag_flash_silenced"),
            brass: bolt_tag_bone(&dobj, "tag_brass"),
            knife: bolt_tag_bone(&dobj, "tag_knife_fx"),
            laser: bolt_tag_bone(&dobj, fx_iw4::FX_LASER_TAG),
        },
    };

    let hands_count = hands_pose.num_bones;
    let gun_count = gun_pose.num_bones;
    let mut hands_surfaces = skin_model(hands, |bone| eye_from_world * skin[bone], &[])?;
    mark_fpv_owner(&mut hands_surfaces, FpvSurfOwner::Hands);
    let mut gun_surfaces = skin_model(
        gun,
        |bone| eye_from_world * skin[hands_count + bone],
        gun_hide_tags,
    )?;
    mark_fpv_owner(&mut gun_surfaces, FpvSurfOwner::Gun);
    let mut extra_base = hands_count + gun_count;
    for (skel, owner) in extras {
        let count = skel.pose.as_ref().map(|p| p.num_bones).unwrap_or(0);
        if let Some(mut extra) =
            skin_model(skel, |bone| eye_from_world * skin[extra_base + bone], &[])
        {
            mark_fpv_owner(&mut extra, owner);
            gun_surfaces.extend(extra);
        }
        extra_base += count;
    }
    Some(PosedFpvEye {
        hands: hands_surfaces,
        gun: gun_surfaces,
        lens,
        bolt,
        stats: PoseStats {
            hands_rigid: hands.rigid_verts,
            hands_blend: hands.blend_verts,
            gun_rigid: gun.rigid_verts,
            gun_blend: gun.blend_verts,
            idle_sampled: true,
        },
    })
}

pub fn pose_eye(
    catalog: &FpvMeshCatalog,
    ns: AssetNamespace,
    gun_name: &str,
    hands: &FpvHands,
    idle: Option<&AnimClip>,
    time: f32,
    gun_hide_tags: &[String],
    scope_name: Option<&str>,
    rocket_name: Option<&str>,
) -> Option<PosedFpvEye> {
    if let Some(clip) = idle {
        return pose_eye_animated(
            catalog,
            ns,
            gun_name,
            hands,
            clip,
            time,
            gun_hide_tags,
            scope_name,
            rocket_name,
        );
    }
    pose_eye_bind_diagnostic(
        catalog,
        ns,
        gun_name,
        hands,
        gun_hide_tags,
        scope_name,
        rocket_name,
    )
}

pub fn pose_eye_animated(
    catalog: &FpvMeshCatalog,
    ns: AssetNamespace,
    gun_name: &str,
    hands: &FpvHands,
    idle: &AnimClip,
    time: f32,
    gun_hide_tags: &[String],
    scope_name: Option<&str>,
    rocket_name: Option<&str>,
) -> Option<PosedFpvEye> {
    pose_eye_blended(
        catalog,
        ns,
        gun_name,
        hands,
        &[PosedClip {
            clip: idle,
            time,
            weight: 1.0,
        }],
        gun_hide_tags,
        scope_name,
        rocket_name,
    )
}

pub fn pose_eye_blended(
    catalog: &FpvMeshCatalog,
    ns: AssetNamespace,
    gun_name: &str,
    hands: &FpvHands,
    anims: &[PosedClip<'_>],
    gun_hide_tags: &[String],
    scope_name: Option<&str>,
    rocket_name: Option<&str>,
) -> Option<PosedFpvEye> {
    let hands = catalog.get_hands(hands)?;
    let gun = catalog.get(ns, gun_name)?;
    let scope = scope_name.and_then(|name| catalog.get(ns, name));
    let rocket = rocket_name.and_then(|name| catalog.get(ns, name));
    pose_fpv_anims(
        &hands.skel,
        &gun.skel,
        scope.map(|entry| &entry.skel),
        rocket.map(|entry| &entry.skel),
        anims,
        gun_hide_tags,
    )
}

pub fn pose_eye_bind_diagnostic(
    catalog: &FpvMeshCatalog,
    ns: AssetNamespace,
    gun_name: &str,
    hands: &FpvHands,
    gun_hide_tags: &[String],
    scope_name: Option<&str>,
    rocket_name: Option<&str>,
) -> Option<PosedFpvEye> {
    let hands = catalog.get_hands(hands)?;
    let gun = catalog.get(ns, gun_name)?;
    let scope = scope_name.and_then(|name| catalog.get(ns, name));
    let rocket = rocket_name.and_then(|name| catalog.get(ns, name));
    let (hands_surfaces, Some(gun_surfaces), lens) = pose_fpv_to_eye(
        &hands.skel,
        Some(&gun.skel),
        scope.map(|entry| &entry.skel),
        rocket.map(|entry| &entry.skel),
        gun_hide_tags,
    )?
    else {
        return None;
    };
    Some(PosedFpvEye {
        hands: hands_surfaces,
        gun: gun_surfaces,
        lens,

        bolt: FpvBoltFrame::default(),
        stats: PoseStats {
            hands_rigid: hands.skel.rigid_verts,
            hands_blend: hands.skel.blend_verts,
            gun_rigid: gun.skel.rigid_verts,
            gun_blend: gun.skel.blend_verts,
            idle_sampled: false,
        },
    })
}

fn scope_attach_tag_name<'a>(gun_bones: &'a [String], scope_bones: &[String]) -> Option<&'a str> {
    if let Some(root) = scope_bones.first()
        && let Some(hit) = gun_bones
            .iter()
            .find(|name| name.eq_ignore_ascii_case(root))
    {
        return Some(hit.as_str());
    }
    for tag in SCOPE_ATTACH_TAGS {
        let on_scope = scope_bones
            .iter()
            .any(|name| name.eq_ignore_ascii_case(tag));
        if !on_scope {
            continue;
        }
        if let Some(hit) = gun_bones.iter().find(|name| name.eq_ignore_ascii_case(tag)) {
            return Some(hit.as_str());
        }
    }
    gun_bones.iter().find_map(|name| {
        SCOPE_ATTACH_TAGS
            .iter()
            .copied()
            .find(|tag| name.eq_ignore_ascii_case(tag))
            .map(|_| name.as_str())
    })
}

fn scope_attach_tag(gun: &ModelPoseSrc, scope: &ModelPoseSrc) -> Option<String> {
    scope_attach_tag_name(&gun.bone_names, &scope.bone_names).map(str::to_owned)
}

const ROCKET_ATTACH_TAG: &str = "tag_clip";

fn build_fpv_dobj<'a>(
    hands: &'a ModelPoseSrc,
    gun: &'a ModelPoseSrc,
    scope: Option<&'a ModelPoseSrc>,
    rocket: Option<&'a ModelPoseSrc>,
    scope_skel: Option<&'a FpvSkel>,
    rocket_skel: Option<&'a FpvSkel>,
) -> Result<(DObj, Vec<(&'a FpvSkel, FpvSurfOwner)>), DObjError> {
    let mut specs: Vec<(&ModelPoseSrc, Option<Attach>)> = vec![
        (hands, None),
        (
            gun,
            Some(Attach {
                parent_model: 0,
                tag: "tag_weapon".into(),
            }),
        ),
    ];
    let mut extras: Vec<(&FpvSkel, FpvSurfOwner)> = Vec::new();
    let mut dobj = DObj::build(&specs)?;

    if let (Some(scope), Some(skel)) = (scope, scope_skel) {
        if let Some(tag) = scope_attach_tag(gun, scope) {
            specs.push((
                scope,
                Some(Attach {
                    parent_model: 1,
                    tag,
                }),
            ));
            match DObj::build(&specs) {
                Ok(next) => {
                    dobj = next;
                    extras.push((skel, FpvSurfOwner::Scope));
                }
                Err(DObjError::AttachTag { .. }) => {
                    specs.pop();
                }
                Err(error) => return Err(error),
            }
        }
    }

    if let (Some(rocket), Some(skel)) = (rocket, rocket_skel) {
        if gun
            .bone_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(ROCKET_ATTACH_TAG))
        {
            specs.push((
                rocket,
                Some(Attach {
                    parent_model: 1,
                    tag: ROCKET_ATTACH_TAG.into(),
                }),
            ));
            match DObj::build(&specs) {
                Ok(next) => {
                    dobj = next;
                    extras.push((skel, FpvSurfOwner::Rocket));
                }
                Err(DObjError::AttachTag { .. }) => {
                    specs.pop();
                }
                Err(error) => return Err(error),
            }
        }
    }

    Ok((dobj, extras))
}

fn bind_scope_surfaces(
    hands: &FpvSkel,
    gun: &FpvSkel,
    scope: &FpvSkel,
    eye_from_model: Mat4,
) -> Option<Vec<PosedModelSurface>> {
    let tag = scope_attach_tag_name(&gun.bone_names, &scope.bone_names)?;
    let tag_i = gun
        .bone_names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(tag))?;
    let weapon_i = hands.tag_weapon?;
    let tw = hands.bones.get(weapon_i)?.to_mat4();
    let gun_root = gun.bones.first()?.to_mat4();
    let gun_attach = tw * gun_root.inverse();
    let tag_mat = gun.bones.get(tag_i)?.to_mat4();
    let scope_i = scope
        .bone_names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(tag))
        .unwrap_or(0);
    let scope_anchor = scope.bones.get(scope_i)?.to_mat4();
    let attach = gun_attach * tag_mat * scope_anchor.inverse();
    skin_model(
        scope,
        |bone| eye_from_model * attach * scope.bones[bone].to_mat4(),
        &[],
    )
}

fn bind_rocket_surfaces(
    hands: &FpvSkel,
    gun: &FpvSkel,
    rocket: &FpvSkel,
    eye_from_model: Mat4,
) -> Option<Vec<PosedModelSurface>> {
    let tag_i = gun
        .bone_names
        .iter()
        .position(|name| name.eq_ignore_ascii_case(ROCKET_ATTACH_TAG))?;
    let weapon_i = hands.tag_weapon?;
    let tw = hands.bones.get(weapon_i)?.to_mat4();
    let gun_root = gun.bones.first()?.to_mat4();
    let gun_attach = tw * gun_root.inverse();
    let tag_mat = gun.bones.get(tag_i)?.to_mat4();
    let rocket_anchor = rocket.bones.first()?.to_mat4();
    let attach = gun_attach * tag_mat * rocket_anchor.inverse();
    skin_model(
        rocket,
        |bone| eye_from_model * attach * rocket.bones[bone].to_mat4(),
        &[],
    )
}
