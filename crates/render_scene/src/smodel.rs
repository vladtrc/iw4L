use bevy::prelude::*;
use dpvs_iw4::{xmodel_get_lod_for_dist, xmodel_lod_camera_dist, xmodel_lod_scaled_dists};
use render_material::RuntimeMaterialCatalog;

#[derive(Clone, Debug, PartialEq)]
pub struct SmodelPassMaterial {
    pub model_lighting_required: bool,
    pub color: Option<Handle<Image>>,
    pub specular: Option<Handle<Image>>,
    pub probe: Option<Handle<Image>>,

    pub atlas: Option<Handle<Image>>,
    pub alpha_mode: AlphaMode,

    pub draw_mode: Option<assets::MaterialDrawMode>,

    pub cull_mode: Option<bevy::render::render_resource::Face>,
    pub env_map_parms: [f32; 4],

    pub lighting_lookup_scale: [f32; 4],
    pub atlas_lookup: [f32; 4],

    pub sort_key: u8,

    pub material_sorted_index: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LodRampArgs {
    pub scale_mid: Option<f32>,
    pub bias_mid: Option<f32>,
    pub scale_last: Option<f32>,

    pub world_unit: Option<f32>,

    pub t5: assets::t5_lod::LodParmsAxis,

    pub t5_no_lod_cull_out: bool,
}

impl LodRampArgs {
    #[must_use]
    pub fn call_dists(self, dist: f32) -> (f32, f32) {
        match (self.scale_mid, self.bias_mid, self.scale_last) {
            (Some(s), Some(b), Some(l)) => xmodel_lod_scaled_dists(dist, s, b, l),
            (Some(s), Some(b), None) => {
                let (mid, _) = xmodel_lod_scaled_dists(dist, s, b, 0.0);
                (mid, dist)
            }
            (None, None, Some(l)) => {
                let (_, last) = xmodel_lod_scaled_dists(dist, 0.0, 0.0, l);
                (dist, last)
            }
            _ => (dist, dist),
        }
    }
}

pub fn smodel_camera_lod(
    lod: Option<assets::ModelLodSelector>,
    origin: [f32; 3],
    scale: f32,
    eye: Option<Vec3>,
    ramp: LodRampArgs,
) -> Option<u8> {
    let (Some(eye), Some(lod)) = (eye, lod) else {
        return Some(0);
    };
    match lod {
        assets::ModelLodSelector::Iw4 {
            lod_start,
            num_lods,
            lod_dist,
        } => {
            let d = xmodel_lod_camera_dist(
                eye.x - origin[0],
                eye.y - origin[1],
                eye.z - origin[2],
                scale,
                ramp.world_unit,
            );
            let (mid, last) = ramp.call_dists(d);
            xmodel_get_lod_for_dist(lod_start, num_lods, lod_dist, mid, last)
        }

        assets::ModelLodSelector::T5 { num_lods, lod_dist } => {
            let dx = eye.x - origin[0];
            let dy = eye.y - origin[1];
            let dz = eye.z - origin[2];
            let d = (dx * dx + dy * dy + dz * dz).sqrt();
            let (ramped, base) = ramp.t5.smodel_call_dists(d, scale);
            assets::t5_lod::xmodel_get_lod_for_dist(
                num_lods,
                lod_dist,
                ramped,
                base,
                ramp.t5_no_lod_cull_out,
            )
        }
    }
}

pub fn runtime_cull_face(cull_mode: Option<u8>) -> Option<bevy::render::render_resource::Face> {
    match cull_mode {
        Some(0) => Some(bevy::render::render_resource::Face::Back),
        Some(1) => Some(bevy::render::render_resource::Face::Front),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct AuthoredMaps {
    pub color: Option<Handle<Image>>,
    pub specular: Option<Handle<Image>>,
    pub alpha_mode: AlphaMode,

    pub draw_mode: Option<assets::MaterialDrawMode>,
    pub cull_mode: Option<bevy::render::render_resource::Face>,
    pub env_map_parms: [f32; 4],
}

pub fn runtime_maps(
    material: Option<assets::MaterialIndex>,
    catalog: &RuntimeMaterialCatalog,
    exact_handles: &[Option<Handle<Image>>],
) -> AuthoredMaps {
    let runtime = material.and_then(|id| catalog.derived(id));
    let tex = |semantic: u8| {
        runtime.and_then(|m| {
            m.texture_semantic(semantic)
                .and_then(|id| exact_handles.get(id.0 as usize).and_then(Clone::clone))
        })
    };
    AuthoredMaps {
        color: tex(assets::TS_COLOR_MAP),
        specular: tex(assets::TS_SPECULAR_MAP),
        alpha_mode: AlphaMode::Opaque,
        draw_mode: None,
        cull_mode: runtime.and_then(|m| runtime_cull_face(m.cull_mode)),
        env_map_parms: runtime.map(|m| m.env_map_parms()).unwrap_or([0.0; 4]),
    }
}

#[derive(Resource, Clone, Copy, Debug)]
pub struct LodRampSkinnedDvar {
    pub scale_mid: Option<f32>,
    pub bias_mid: Option<f32>,
    pub scale_last: Option<f32>,
    pub world_unit: Option<f32>,
    pub t5_scale: f32,
    pub t5_bias: f32,
    pub t5_fov_threshold: f32,
    pub t5: assets::t5_lod::LodParmsAxis,
}

impl Default for LodRampSkinnedDvar {
    fn default() -> Self {
        Self {
            scale_mid: None,
            bias_mid: None,
            scale_last: None,
            world_unit: None,
            t5_scale: assets::t5_lod::R_LOD_SCALE_SKINNED_DEFAULT,
            t5_bias: assets::t5_lod::R_LOD_BIAS_SKINNED_DEFAULT,
            t5_fov_threshold: assets::t5_lod::R_FOV_SCALE_THRESHOLD_DEFAULT,
            t5: assets::t5_lod::LodParmsAxis::default(),
        }
    }
}

impl LodRampSkinnedDvar {
    #[must_use]
    pub fn args(self) -> LodRampArgs {
        LodRampArgs {
            scale_mid: self.scale_mid,
            bias_mid: self.bias_mid,
            scale_last: self.scale_last,
            world_unit: self.world_unit,
            t5: self.t5,
            t5_no_lod_cull_out: false,
        }
    }
}
