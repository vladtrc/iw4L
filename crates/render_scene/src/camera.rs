use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCameraPose {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

#[derive(Component)]
pub struct FpvLens;

#[derive(Component)]
pub struct FlyCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SimCamera {
    pub enabled: bool,

    pub freeze_fly: bool,
}

pub fn transform_from_iw_view(view: WorldCameraPose) -> Transform {
    let [pitch, yaw, roll] = view.angles.map(f32::to_radians);
    let (sp, cp) = pitch.sin_cos();
    let (sy, cy) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();

    let forward = Vec3::new(cp * cy, cp * sy, -sp);
    let right = Vec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = Vec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    Transform {
        translation: Vec3::from_array(view.origin),
        rotation: Quat::from_mat3(&Mat3::from_cols(right, up, -forward)),
        ..default()
    }
}
