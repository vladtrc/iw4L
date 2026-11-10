use glam::{Mat4, Vec3};

use crate::LightAttenuationBind;
use render_material::{RuntimeCodeSources, RuntimeImageId};

#[derive(Clone, Copy, Debug, Default)]
pub struct OutdoorLookup {
    pub image: Option<RuntimeImageId>,
    pub lookup: [u32; 16],
}

#[derive(Clone, Debug, Default)]
pub struct MaterialExecFrame {
    pub code_sources: RuntimeCodeSources,
    pub view_origin: Vec3,
    pub float_time: f32,
    pub clip_from_world: Option<Mat4>,
    pub view_from_world: Option<Mat4>,
    pub clip_from_view: Option<Mat4>,
    pub outdoor: Option<OutdoorLookup>,
    pub viewmodel_clip_from_world: Option<Mat4>,

    pub viewmodel_near: Option<f32>,
    pub inv_image_height: Option<f32>,
    pub primary_lights: Vec<lighting_iw4::GfxLightPack>,
    pub attenuation: Vec<LightAttenuationBind>,
    pub local_light_bindings: Vec<render_material::CompiledConstantOverlay>,
    pub reflection_probe_sh: Vec<Option<[[f32; 4]; 3]>>,
    pub smodel_lighting_sh: std::sync::Arc<Vec<Option<[[f32; 4]; 3]>>>,
    pub xmodel_lighting_base: u32,
    pub xmodel_lighting_sh: std::sync::Arc<Vec<Option<[[f32; 4]; 3]>>>,

    pub spot_receivers: Vec<Option<SpotShadowReceiver>>,

    /// The projectile_flight atlas seen from the camera's world, when the map has one.
    pub portal: Option<crate::PortalView>,
    /// For each atlas world other than the camera's that a door chain
    /// reaches, the camera carried into it (`PortalView::moves`).
    pub world_cameras: Vec<Option<WorldCamera>>,
    /// Authored far-fog colour by physical world, including unselected views.
    pub world_fog_colors: Vec<[f32; 4]>,
}

/// The camera as an atlas world sees it: the frame's camera moved back
/// through the doors into that world's own coordinates. Its draws read their
/// eye, fog and sky from here, so they light and fog as in their own map.
#[derive(Clone, Debug, Default)]
pub struct WorldCamera {
    pub sun_shadow: bool,
    pub exit_plane_clip: Option<[f32; 4]>,
    pub exit_plane_world: Option<[f64; 4]>,
    pub code_sources: RuntimeCodeSources,
    pub view_origin: Vec3,
    pub clip_from_world: Mat4,
    pub view_from_world: Mat4,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpotShadowReceiver {
    pub lookup: [f32; 16],

    pub pixel_adjust: [f32; 4],
}
