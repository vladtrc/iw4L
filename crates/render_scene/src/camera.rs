use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldCameraPose {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
}

pub fn place_simulation_pose(
    space: Option<&asset_core::world_frame::SceneSpace>,
    origin: [f32; 3],
    angles: [f32; 3],
) -> WorldCameraPose {
    let Some(space @ asset_core::world_frame::SceneSpace::Shared(_)) = space else {
        return WorldCameraPose { origin, angles };
    };
    WorldCameraPose {
        origin: space.simulation_point(origin),
        angles: math_iw4::axis_to_angles(
            math_iw4::angles_to_axis(angles).map(|axis| space.simulation_vector(origin, axis)),
        ),
    }
}

#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CameraWorld(pub usize);

#[derive(Component)]
pub struct FpvLens;

#[derive(Component)]
#[require(CameraWorld)]
#[require(bevy::prelude::DespawnOnExit<frame::MatchScope> = bevy::prelude::DespawnOnExit(frame::MatchScope::Live))]
pub struct FlyCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub speed: f32,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SimCamera {
    pub enabled: bool,

    pub freeze_fly: bool,
    pub view_override: Option<CameraView>,
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

#[derive(Clone, Copy, Debug)]
pub struct CameraView {
    pub transform: Transform,
    pub horizontal_fov: f32,
}

pub fn place_simulation_transform(
    space: Option<&asset_core::world_frame::SceneSpace>,
    mut pose: Transform,
) -> Transform {
    let Some(space @ asset_core::world_frame::SceneSpace::Shared(_)) = space else {
        return pose;
    };
    let sim = asset_core::world_frame::SimPoint::from_slab(pose.translation.to_array());
    let axes = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        .map(|a| Vec3::from_array(space.vector_to_scene(sim.world(), a)));
    pose.translation = Vec3::from_array(space.to_scene(sim).xyz());
    pose.rotation = Quat::from_mat3(&Mat3::from_cols(axes[0], axes[1], axes[2])) * pose.rotation;
    pose
}
