use bevy::math::{Mat3, Mat4, Quat, Vec3, Vec4};

pub use crate::anim::xmodel_pose::{FpvSurfOwner, PosedModelSurface};
use assets::AnimClip;

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

/// One clip a hand is playing this frame. `node` is the scheduler slot it is
/// playing in, which is what the rig binds its bone tracks against — a clip
/// that stayed in its node keeps the binding it was given.
#[derive(Clone, Copy)]
pub struct PosedClip<'a> {
    pub node: usize,
    pub clip: &'a AnimClip,
    pub time: f32,
    pub weight: f32,
}
